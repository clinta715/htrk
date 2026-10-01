use super::*;

// ── Audio-thread Processor (real CLAP processing) ──

/// Real CLAP processor that calls the plugin's process() function each callback.
/// Wraps a `StartedPluginAudioProcessor<HtrkHost>` and pre-allocates the
/// audio I/O buffers + event buffers in `new()` for allocation-free processing.
pub struct ClapPluginProcessor {
    pub(super) processor: Option<StartedPluginAudioProcessor<HtrkHost>>,
    pub(super) descriptor: PluginDescriptor,
    pub(super) sample_rate: f64,
    pub(super) max_block: usize,

    // Pre-allocated I/O buffers, sized to max_block
    pub(super) in_l: Vec<f32>,
    pub(super) in_r: Vec<f32>,
    pub(super) out_l: Vec<f32>,
    pub(super) out_r: Vec<f32>,

    // Pre-allocated AudioPorts containers
    pub(super) input_ports: AudioPorts,
    pub(super) output_ports: AudioPorts,

    // Pre-allocated event buffers (allocated once at construction; RT-safe)
    pub(super) _input_event_buffer: clack_host::events::io::EventBuffer,
    pub(super) output_event_buffer: clack_host::events::io::EventBuffer,

    // Consumer half of the parameter ring, fed by the main-thread handle.
    pub(super) param_rx: ParamReceiver,
    // Parameter changes originating on the audio thread (sequencer
    // automation). Not shared across threads, so a plain Vec suffices.
    pub(super) pending_params: Vec<ParamChange>,
    // Scratch vector for drained param changes. Allocated once (capacity
    // covers both sources); cleared between process() calls.
    pub(super) param_scratch: Vec<ParamChange>,

    /// Queued note-on/off events from the sequencer, drained in process().
    /// Tuples: (note_on, midi_channel, key, velocity)
    pub(super) note_events: std::collections::VecDeque<(bool, u8, u8, u8)>,
    /// Monotonically increasing note ID counter for CLAP note tracking.
    pub(super) next_note_id: u32,
}

impl ClapPluginProcessor {
    pub fn new(
        processor: StartedPluginAudioProcessor<HtrkHost>,
        descriptor: PluginDescriptor,
        sample_rate: f64,
        max_block: usize,
        param_rx: ParamReceiver,
    ) -> Self {
        let max_block = max_block.max(1);
        ClapPluginProcessor {
            processor: Some(processor),
            descriptor,
            sample_rate,
            max_block,
            in_l: vec![0.0; max_block],
            in_r: vec![0.0; max_block],
            out_l: vec![0.0; max_block],
            out_r: vec![0.0; max_block],
            input_ports: AudioPorts::with_capacity(2, 1),
            output_ports: AudioPorts::with_capacity(2, 1),
            _input_event_buffer: clack_host::events::io::EventBuffer::with_capacity(0),
            output_event_buffer: clack_host::events::io::EventBuffer::with_capacity(0),
            param_rx,
            pending_params: Vec::with_capacity(64),
            param_scratch: Vec::with_capacity(128),
            note_events: std::collections::VecDeque::new(),
            next_note_id: 0,
        }
    }

    /// Stop processing and return the StoppedPluginAudioProcessor so the handle can
    /// call `instance.deactivate(stopped)` on the main thread. Consumes self.
    /// The Option field is `take()`n so the Drop impl doesn't double-stop.
    pub fn stop(mut self) -> clack_host::process::StoppedPluginAudioProcessor<HtrkHost> {
        self.processor
            .take()
            .expect("ClapPluginProcessor::stop called twice")
            .stop_processing()
    }
}

impl Drop for ClapPluginProcessor {
    /// Defensive cleanup: if the processor is dropped without `stop()`
    /// being called first (e.g. the audio engine drops the SendBus's
    /// plugin slot because the user removed the plugin without
    /// round-tripping through `deactivate`), make sure the
    /// `StartedPluginAudioProcessor` is properly stopped so the
    /// audio-thread side of the plugin is released.
    ///
    /// The returned `StoppedPluginAudioProcessor` cannot be sent back
    /// to the main thread from Drop, so it's dropped here (a small
    /// leak). The main thread's `PluginInstance::drop()` will still
    /// call the plugin's `destroy` callback and release the plugin's
    /// own resources, so the leak is limited to the wrapper struct.
    fn drop(&mut self) {
        if let Some(processor) = self.processor.take() {
            let stopped = processor.stop_processing();
            tracing::debug!(
                "[plugin] ClapPluginProcessor dropped without explicit stop(); \
                 audio-thread resources released but StoppedPluginAudioProcessor \
                 leaked (main thread cannot call deactivate)"
            );
            drop(stopped);
        }
    }
}

impl std::fmt::Debug for ClapPluginProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClapPluginProcessor")
            .field("name", &self.descriptor.name)
            .field("sample_rate", &self.sample_rate)
            .field("max_block", &self.max_block)
            .finish()
    }
}

impl HostedPluginProcessor for ClapPluginProcessor {
    fn stop(self: Box<Self>) -> Box<dyn std::any::Any> {
        Box::new(ClapPluginProcessor::stop(*self))
    }

    fn send_note_on(&mut self, midi_channel: u8, key: u8, velocity: u8) {
        self.note_events
            .push_back((true, midi_channel, key, velocity));
    }

    fn send_note_off(&mut self, midi_channel: u8, key: u8) {
        self.note_events.push_back((false, midi_channel, key, 0));
    }

    fn process(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        frame_count: usize,
        _transport: &TransportInfo,
    ) {
        use clack_common::events::event_types::{NoteOffEvent, NoteOnEvent, ParamValueEvent};
        use clack_common::events::{Match, Pckn};
        use clack_common::utils::{ClapId, Cookie};
        use clack_host::process::audio_buffers::{
            AudioPortBuffer, AudioPortBufferType, InputChannel,
        };

        // Drain parameter ring and note queue ONCE per callback. All events
        // are delivered in the first sub-block at time 0 (matching the
        // previous single-block behavior); subsequent sub-blocks get an empty
        // event buffer so notes/params are not duplicated across chunks.
        self.param_scratch.clear();
        // Audio-thread-originated changes (automation) first, then whatever
        // the main thread queued since the last callback.
        self.param_scratch.append(&mut self.pending_params);
        self.param_rx.drain_into(&mut self.param_scratch, 64);
        let cookie = Cookie::default();
        let pckn = Pckn::new(0u16, 0u16, 0u16, Match::All);

        let mut first_events = clack_host::events::io::EventBuffer::with_capacity(
            (self.param_scratch.len() + self.note_events.len()).max(1),
        );
        for change in self.param_scratch.iter() {
            let ev =
                ParamValueEvent::new(0, ClapId::from(change.param_id), pckn, change.value, cookie);
            first_events.push(&ev);
        }
        while let Some((note_on, midi_ch, key, velocity)) = self.note_events.pop_front() {
            if note_on {
                let note_id = self.next_note_id;
                self.next_note_id = self.next_note_id.wrapping_add(1);
                let note_pckn = Pckn::new(0u16, midi_ch as u16, key as u16, note_id);
                let ev = NoteOnEvent::new(0, note_pckn, (velocity as f64) / 127.0);
                first_events.push(&ev);
            } else {
                let note_pckn = Pckn::new(0u16, midi_ch as u16, key as u16, Match::All);
                let ev = NoteOffEvent::new(0, note_pckn, 0.0);
                first_events.push(&ev);
            }
        }
        let empty_events = clack_host::events::io::EventBuffer::with_capacity(0);

        // Process in sub-blocks of `max_block`. The audio callback can hand
        // us more frames than the plugin was activated for (e.g. a WASAPI
        // shared-mode 1024-frame buffer vs. a 512-sample max_block), so we
        // chunk the callback into max_block-sized pieces and invoke the
        // plugin once per chunk. Without this, frames past `max_block` would
        // be left as silence, producing a chopped/intermittent output.
        let block_len = self.max_block;
        let mut offset = 0usize;
        let mut first = true;
        while offset < frame_count {
            let chunk = (frame_count - offset).min(block_len);

            // Copy this chunk's input into the pre-allocated buffers and
            // zero the tail (the plugin always sees a full block_len).
            for i in 0..chunk {
                self.in_l[i] = input_l[offset + i];
                self.in_r[i] = input_r[offset + i];
                self.out_l[i] = 0.0;
                self.out_r[i] = 0.0;
            }
            for i in chunk..block_len {
                self.in_l[i] = 0.0;
                self.in_r[i] = 0.0;
                self.out_l[i] = 0.0;
                self.out_r[i] = 0.0;
            }

            // Rebuild clack audio port buffers around the internal buffers.
            // Raw pointers are used (as in the original) so the &mut [f32]
            // slices don't form a tracked borrow of `self`, which would
            // conflict with `self.processor.as_mut()` below.
            let in_l_ptr = self.in_l.as_mut_ptr();
            let in_r_ptr = self.in_r.as_mut_ptr();
            let out_l_ptr = self.out_l.as_mut_ptr();
            let out_r_ptr = self.out_r.as_mut_ptr();
            // SAFETY: the four buffers each hold exactly `block_len` f32s
            // (allocated in `new()` and never resized).
            let in_l_slice: &mut [f32] =
                unsafe { std::slice::from_raw_parts_mut(in_l_ptr, block_len) };
            let in_r_slice: &mut [f32] =
                unsafe { std::slice::from_raw_parts_mut(in_r_ptr, block_len) };
            let out_l_slice: &mut [f32] =
                unsafe { std::slice::from_raw_parts_mut(out_l_ptr, block_len) };
            let out_r_slice: &mut [f32] =
                unsafe { std::slice::from_raw_parts_mut(out_r_ptr, block_len) };

            let input_audio = self.input_ports.with_input_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_input_only([
                    InputChannel::variable(in_l_slice),
                    InputChannel::variable(in_r_slice),
                ]),
            }]);

            let mut output_audio = self.output_ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only([out_l_slice, out_r_slice]),
            }]);

            let input_events = if first {
                clack_host::events::io::InputEvents::from_buffer(&first_events)
            } else {
                clack_host::events::io::InputEvents::from_buffer(&empty_events)
            };
            let mut output_events =
                clack_host::events::io::OutputEvents::from_buffer(&mut self.output_event_buffer);

            if let Some(processor) = self.processor.as_mut() {
                let _ = processor.process(
                    &input_audio,
                    &mut output_audio,
                    &input_events,
                    &mut output_events,
                    None,
                    None,
                );
            }

            // Copy this chunk's plugin output back to the caller's buffers.
            output_l[offset..offset + chunk].copy_from_slice(&self.out_l[..chunk]);
            output_r[offset..offset + chunk].copy_from_slice(&self.out_r[..chunk]);

            offset += chunk;
            first = false;
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        // Audio-thread-local change: queue it for the next process() call.
        // When the small queue is saturated the newest value wins.
        if self.pending_params.len() >= 64 {
            self.pending_params.remove(0);
        }
        self.pending_params.push(ParamChange {
            param_id,
            value: value as f64,
        });
    }

    fn get_parameter(&self, _param_id: u32) -> f32 {
        // Plugin parameter values are stored in the plugin itself;
        // reading them from the host requires a main-thread call
        // (plugin_handle is !Send). The processor doesn't have a copy
        // of the values — callers should go through the handle for
        // reads. Return 0.0 as a placeholder.
        0.0
    }

    fn parameter_count(&self) -> u32 {
        // The processor doesn't cache the count; the handle does.
        // Return 0 as a safe default — callers needing the count
        // should go through the handle.
        0
    }
    fn latency(&self) -> u32 {
        0
    }
    fn name(&self) -> &str {
        &self.descriptor.name
    }
}
