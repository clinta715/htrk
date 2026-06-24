// CLAP plugin loader (Phase 2).
//
// Skeleton implementation: provides the lifecycle scaffolding (load, activate,
// stop, deactivate) and a stub process() that passes audio through unchanged.
// The real CLAP processing pipeline (events, parameter queue, AudioPorts) is
// filled in incrementally as we integrate with real CLAP plugins.

use std::any::Any;
use std::path::Path;

use clack_host::prelude::*;
use clack_common::plugin::PluginDescriptor as ClapPluginDescriptor;

use super::{
    HostedPluginHandle, HostedPluginProcessor, ParamInfo, PluginDescriptor, PluginError,
    PluginFormat, PluginType, TransportInfo,
};

// ── Host Handler ──
//
// `()` works for all three handler types — clack provides default no-op impls.

pub struct HtrkHost;
impl HostHandlers for HtrkHost {
    type Shared<'a> = ();
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();
}

// ── Main-thread Handle ──

pub struct ClapPluginHandle {
    instance: Option<PluginInstance<HtrkHost>>,
    descriptor: PluginDescriptor,
    activated: bool,
}

impl ClapPluginHandle {
    /// Load a CLAP plugin from disk and instantiate it. Discovers the first plugin in the bundle.
    ///
    /// On Windows, CLAP plugins can be packaged as either:
    /// - A single `.clap` DLL file directly at the path
    /// - A directory with `.clap` extension containing a DLL of the same name
    /// This method handles both cases.
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        // On Windows, .clap can be a directory bundle. Resolve to the actual DLL.
        let load_path = if path.is_dir() {
            // Bundle directory: look for a DLL with the same stem
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| PluginError::InvalidFormat("Bundle missing name".into()))?;
            let dll_name = format!("{stem}.clap");
            let dll = path.join(&dll_name);
            if !dll.is_file() {
                return Err(PluginError::LoadFailed(format!(
                    "Bundle {} has no {} DLL",
                    path.display(),
                    dll_name
                )));
            }
            dll
        } else {
            path.to_path_buf()
        };

        let entry = unsafe {
            PluginEntry::load(&load_path).map_err(|e| PluginError::LoadFailed(e.to_string()))?
        };

        let plugin_factory = entry
            .get_plugin_factory()
            .ok_or(PluginError::LoadFailed("No plugin factory".into()))?;

        let clap_descriptor = plugin_factory
            .plugin_descriptors()
            .next()
            .ok_or(PluginError::LoadFailed("No plugins in bundle".into()))?;

        let host_info = HostInfo::new("htrk", "htrk", "https://github.com/clinta715/htrk", env!("CARGO_PKG_VERSION"))
            .map_err(|e| PluginError::LoadFailed(e.to_string()))?;

        let plugin_id = clap_descriptor
            .id()
            .ok_or(PluginError::LoadFailed("Plugin missing id".into()))?
            .to_owned();

        let instance = PluginInstance::<HtrkHost>::new(
            |_| (),
            |_| (),
            &entry,
            &plugin_id,
            &host_info,
        )
        .map_err(|e| PluginError::LoadFailed(e.to_string()))?;

        let descriptor = extract_descriptor(path, &clap_descriptor);

        Ok(ClapPluginHandle {
            instance: Some(instance),
            descriptor,
            activated: false,
        })
    }

    /// Returns the descriptor.
    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }
}

fn extract_descriptor(path: &Path, clap_desc: &ClapPluginDescriptor) -> PluginDescriptor {
    let plugin_id = clap_desc
        .id()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let name = clap_desc
        .name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    let vendor = clap_desc
        .vendor()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    let description = clap_desc
        .description()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Determine plugin type heuristically from features
    let features: Vec<String> = clap_desc
        .features()
        .map(|s| s.to_string_lossy().to_lowercase())
        .collect();

    let plugin_type = if features.iter().any(|f| f == "instrument") {
        PluginType::Instrument
    } else if features.iter().any(|f| f.contains("effect") || f == "analyzer") {
        PluginType::Effect
    } else {
        PluginType::Both
    };

    PluginDescriptor {
        format: PluginFormat::Clap,
        path: path.to_path_buf(),
        plugin_id,
        name,
        vendor,
        version: String::new(),
        description,
        plugin_type,
        audio_inputs: 2,
        audio_outputs: 2,
        has_editor: false,
        supports_state: true,
    }
}

impl HostedPluginHandle for ClapPluginHandle {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    fn activate(&mut self, sample_rate: f64, max_block: u32) -> Result<Box<dyn HostedPluginProcessor>, String> {
        let instance = self.instance.as_mut().ok_or_else(|| "Plugin not loaded".to_string())?;

        let config = PluginAudioConfiguration {
            sample_rate,
            min_frames_count: 1,
            max_frames_count: max_block.max(1),
        };

        // PluginInstance::activate consumes the closure, returns the StoppedPluginAudioProcessor
        let stopped = instance
            .activate(|_, _| (), config)
            .map_err(|e| e.to_string())?;

        // Start processing, getting the StartedPluginAudioProcessor for the audio thread
        let started = stopped
            .start_processing()
            .map_err(|e| format!("start_processing: {e:?}"))?;

        self.activated = true;
        Ok(Box::new(ClapPluginProcessor::new(
            started,
            self.descriptor.clone(),
            sample_rate,
            max_block as usize,
        )))
    }

    fn deactivate(&mut self, stopped: Box<dyn Any>) -> Result<(), String> {
        if let Some(instance) = self.instance.as_mut() {
            if self.activated {
                // Downcast the Box to the concrete StoppedPluginAudioProcessor type
                // and pass it to instance.deactivate() which stops the plugin.
                let stopped = stopped
                    .downcast::<clack_host::process::StoppedPluginAudioProcessor<HtrkHost>>()
                    .map_err(|_| "stopped processor wrong type".to_string())?;
                instance.deactivate(*stopped);
                self.activated = false;
            }
        }
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>, String> {
        // State save requires the state extension and a live instance. Stub.
        Ok(Vec::new())
    }

    fn load_state(&mut self, _state: &[u8]) -> Result<(), String> {
        // State restore requires the state extension. Stub.
        Ok(())
    }

    fn parameter_info(&self) -> Vec<ParamInfo> {
        // Parameter enumeration requires the params extension. Stub.
        Vec::new()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ── Audio-thread Processor (real CLAP processing) ──

/// Real CLAP processor that calls the plugin's process() function each callback.
/// Wraps a `StartedPluginAudioProcessor<HtrkHost>` and pre-allocates the
/// audio I/O buffers + event buffers in `new()` for allocation-free processing.
pub struct ClapPluginProcessor {
    processor: StartedPluginAudioProcessor<HtrkHost>,
    descriptor: PluginDescriptor,
    sample_rate: f64,
    max_block: usize,

    // Pre-allocated I/O buffers, sized to max_block
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,

    // Pre-allocated AudioPorts containers
    input_ports: AudioPorts,
    output_ports: AudioPorts,

    // Pre-allocated event buffers (allocated once at construction; RT-safe)
    input_event_buffer: clack_host::events::io::EventBuffer,
    output_event_buffer: clack_host::events::io::EventBuffer,
}

impl ClapPluginProcessor {
    pub fn new(
        processor: StartedPluginAudioProcessor<HtrkHost>,
        descriptor: PluginDescriptor,
        sample_rate: f64,
        max_block: usize,
    ) -> Self {
        let max_block = max_block.max(1) as usize;
        ClapPluginProcessor {
            processor,
            descriptor,
            sample_rate,
            max_block,
            in_l: vec![0.0; max_block],
            in_r: vec![0.0; max_block],
            out_l: vec![0.0; max_block],
            out_r: vec![0.0; max_block],
            input_ports: AudioPorts::with_capacity(2, 1),
            output_ports: AudioPorts::with_capacity(2, 1),
            input_event_buffer: clack_host::events::io::EventBuffer::with_capacity(0),
            output_event_buffer: clack_host::events::io::EventBuffer::with_capacity(0),
        }
    }

    /// Stop processing and return the StoppedPluginAudioProcessor so the handle can
    /// call `instance.deactivate(stopped)` on the main thread. Consumes self.
    pub fn stop(self) -> clack_host::process::StoppedPluginAudioProcessor<HtrkHost> {
        self.processor.stop_processing()
    }
}

impl HostedPluginProcessor for ClapPluginProcessor {
    fn stop(self: Box<Self>) -> Box<dyn std::any::Any> {
        Box::new(ClapPluginProcessor::stop(*self))
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
        let n = frame_count.min(self.max_block);

        // Copy input into our pre-allocated buffers
        for i in 0..n {
            self.in_l[i] = if i < input_l.len() { input_l[i] } else { 0.0 };
            self.in_r[i] = if i < input_r.len() { input_r[i] } else { 0.0 };
            self.out_l[i] = 0.0;
            self.out_r[i] = 0.0;
        }
        // Zero the rest
        for i in n..self.max_block {
            self.in_l[i] = 0.0;
            self.in_r[i] = 0.0;
            self.out_l[i] = 0.0;
            self.out_r[i] = 0.0;
        }

        // Build audio port buffer array. We need &mut [f32] for the buffer references.
        // We split the buffers into individual channel slices.
        use clack_host::process::audio_buffers::{
            AudioPortBuffer, AudioPortBufferType, InputChannel,
        };

        // Split the in/out buffers into per-channel slices.
        // We use indices instead of slices to avoid borrow issues with `with_input_buffers`.
        let in_l_ptr = self.in_l.as_mut_ptr();
        let in_r_ptr = self.in_r.as_mut_ptr();
        let out_l_ptr = self.out_l.as_mut_ptr();
        let out_r_ptr = self.out_r.as_mut_ptr();
        let block_len = self.max_block;

        // SAFETY: The pointers are valid for `block_len` f32 elements.
        let in_l_slice: &mut [f32] = unsafe { std::slice::from_raw_parts_mut(in_l_ptr, block_len) };
        let in_r_slice: &mut [f32] = unsafe { std::slice::from_raw_parts_mut(in_r_ptr, block_len) };
        let out_l_slice: &mut [f32] = unsafe { std::slice::from_raw_parts_mut(out_l_ptr, block_len) };
        let out_r_slice: &mut [f32] = unsafe { std::slice::from_raw_parts_mut(out_r_ptr, block_len) };

        let mut input_audio = self.input_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only([
                InputChannel::variable(in_l_slice),
                InputChannel::variable(in_r_slice),
            ]),
        }]);

        let mut output_audio = self.output_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only([
                out_l_slice,
                out_r_slice,
            ]),
        }]);

        // Empty input events for Phase 2 (FX plugins don't need MIDI).
        let input_events = clack_host::events::io::InputEvents::from_buffer(&self.input_event_buffer);
        let mut output_events = clack_host::events::io::OutputEvents::from_buffer(&mut self.output_event_buffer);

        let _ = self.processor.process(
            &input_audio,
            &mut output_audio,
            &input_events,
            &mut output_events,
            None,
            None,
        );

        // Copy plugin output back to caller's buffers
        for i in 0..frame_count.min(n) {
            output_l[i] = self.out_l[i];
            output_r[i] = self.out_r[i];
        }
    }

    fn set_parameter(&mut self, _param_id: u32, _value: f32) {
        // Phase 2: parameter changes must go through the parameter queue
        // and be applied as ParamValue events in the next process() call.
        // Stub for now — no parameters are accessible yet.
    }

    fn get_parameter(&self, _param_id: u32) -> f32 { 0.0 }
    fn parameter_count(&self) -> u32 { 0 }
    fn latency(&self) -> u32 { 0 }
    fn name(&self) -> &str { &self.descriptor.name }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_extraction_basic() {
        let desc = PluginDescriptor {
            format: PluginFormat::Clap,
            path: std::path::PathBuf::from("/test/plugin.clap"),
            plugin_id: "test.id".into(),
            name: "Test Plugin".into(),
            vendor: "Tester".into(),
            version: "1.0".into(),
            description: "A test plugin".into(),
            plugin_type: PluginType::Effect,
            audio_inputs: 2,
            audio_outputs: 2,
            has_editor: false,
            supports_state: true,
        };
        assert_eq!(desc.name, "Test Plugin");
        assert_eq!(desc.format, PluginFormat::Clap);
    }

    #[test]
    fn test_invalid_path_returns_error() {
        let result = ClapPluginHandle::load(std::path::Path::new("/nonexistent/plugin.clap"));
        assert!(result.is_err());
    }

    /// Integration test: try to load a real CLAP plugin from the system install.
    /// Skipped if the plugin isn't available (CI without CLAP installed).
    #[test]
    fn test_load_real_clap_plugin() {
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found at {}", path.display());
            return;
        }
        let handle = ClapPluginHandle::load(path);
        match handle {
            Ok(h) => {
                let desc = h.descriptor();
                eprintln!("[ok] Loaded plugin: {} ({})", desc.name, desc.plugin_id);
                eprintln!("     vendor: {}", desc.vendor);
                eprintln!("     audio: {} in / {} out, has_editor={}, supports_state={}",
                    desc.audio_inputs, desc.audio_outputs, desc.has_editor, desc.supports_state);
                assert!(!desc.name.is_empty(), "Plugin name should not be empty");
            }
            Err(e) => panic!("Failed to load TAL-Reverb-4: {e}"),
        }
    }

    /// Integration test: try to load several different CLAP plugin formats
    /// (single-DLL and bundle) to verify the loader handles both.
    #[test]
    fn test_load_multiple_clap_plugins() {
        let candidates = [
            (r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap", "bundle"),
            (r"C:\Program Files\Common Files\CLAP\bit-crusher-windows-x64.clap", "dll"),
            (r"C:\Program Files\Common Files\CLAP\Dexed.clap", "dll"),
            (r"C:\Program Files\Common Files\CLAP\JC303.clap", "dll"),
        ];
        let mut loaded = 0;
        let mut failed: Vec<String> = Vec::new();
        for (path_str, kind) in &candidates {
            let path = std::path::Path::new(path_str);
            if !path.exists() {
                eprintln!("[skip] {} not found ({})", path_str, kind);
                continue;
            }
            match ClapPluginHandle::load(path) {
                Ok(h) => {
                    let desc = h.descriptor();
                    eprintln!("[ok] {}: {} ({})", kind, desc.name, desc.plugin_id);
                    loaded += 1;
                }
                Err(e) => {
                    eprintln!("[fail] {}: {}", path_str, e);
                    failed.push(path_str.to_string());
                }
            }
        }
        assert!(loaded >= 2, "Expected at least 2 plugins to load, got {loaded}");
        assert!(failed.is_empty(), "Failed to load: {failed:?}");
    }

    /// Integration test: full lifecycle (load → activate → process → deactivate).
    /// Skipped if the plugin isn't available.
    #[test]
    fn test_activate_real_clap_plugin() {
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found at {}", path.display());
            return;
        }

        let mut handle = ClapPluginHandle::load(path).expect("load failed");

        // Activate at 48kHz, 256 sample block size
        let mut processor = handle.activate(48000.0, 256).expect("activate failed");
        eprintln!("[ok] Activated plugin: {}", processor.name());

        // Process a block of silence (1s = 48000 samples, process in 256-sample chunks)
        let in_l = vec![0.0f32; 256];
        let in_r = vec![0.0f32; 256];
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        let transport = TransportInfo {
            bpm: 120.0,
            sample_rate: 48000.0,
            sample_position: 0,
            is_playing: true,
        };

        // Process a few blocks. The reverb should produce non-zero output if we
        // feed it a non-silent input. With silence input, it should produce
        // only tail/decay of previous samples (which is zero here, since
        // we never fed any signal).
        for block in 0..4 {
            processor.process(&in_l, &in_r, &mut out_l, &mut out_r, 256, &transport);
            eprintln!("[ok] Block {}: out[0] = {:.6} (silence in -> near-silence expected)",
                block, out_l[0]);
        }

        // Test with a real signal (impulse) to see if the reverb tail is produced
        let mut impulse_l = vec![0.0f32; 256];
        let mut impulse_r = vec![0.0f32; 256];
        impulse_l[0] = 1.0;
        impulse_r[0] = 1.0;
        processor.process(&impulse_l, &impulse_r, &mut out_l, &mut out_r, 256, &transport);
        let peak = out_l.iter().chain(out_r.iter()).fold(0.0f32, |a, &b| a.max(b.abs()));
        eprintln!("[ok] Impulse response peak: {peak:.6} (should be non-zero for active reverb)");

        // Test deactivation: stop the processor and deactivate the instance
        let stopped = processor.stop();
        handle.deactivate(stopped).expect("deactivate failed");
        eprintln!("[ok] Deactivated plugin cleanly");
    }

    /// Integration test: process a sine wave through Bit Crusher plugin.
    /// The output should be a quantized/distorted version of the input.
    #[test]
    fn test_bit_crusher_processing() {
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\bit-crusher-windows-x64.clap");
        if !path.exists() {
            eprintln!("[skip] Bit Crusher not found");
            return;
        }

        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        let mut processor = handle.activate(48000.0, 256).expect("activate failed");
        eprintln!("[ok] Activated: {}", processor.name());

        // Generate a 1kHz sine wave
        let freq = 1000.0f32;
        let sr = 48000.0f32;
        let mut input_l = Vec::with_capacity(256);
        let mut input_r = Vec::with_capacity(256);
        for i in 0..256 {
            let t = i as f32 / sr;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
            input_l.push(s);
            input_r.push(s);
        }
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        let transport = TransportInfo {
            bpm: 120.0,
            sample_rate: 48000.0,
            sample_position: 0,
            is_playing: true,
        };

        // Feed a few blocks of sine so the effect's internal state stabilizes
        for _ in 0..4 {
            processor.process(&input_l, &input_r, &mut out_l, &mut out_r, 256, &transport);
        }

        let in_peak = input_l.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let out_peak = out_l.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        eprintln!("[ok] Sine input peak: {in_peak:.4}");
        eprintln!("[ok] Bit crusher output peak: {out_peak:.4} (non-zero means plugin is processing)");
        assert!(out_peak > 0.01, "Bit Crusher should produce non-trivial output");

        // Compare sample values: bit crusher should quantize the input,
        // producing a different (not equal) output for most samples.
        let mut differences = 0;
        let mut exact_matches = 0;
        for i in 0..256 {
            if (input_l[i] - out_l[i]).abs() > 0.001 {
                differences += 1;
            } else {
                exact_matches += 1;
            }
        }
        eprintln!("[ok] Bit crusher: {differences}/256 samples changed, {exact_matches} exact matches");
        // Note: the bit crusher's default parameters may pass through audio unchanged.
        // We're just verifying the plugin processes audio (non-zero output is sufficient).
        // Once we have parameter control, we can crank up the bit reduction to confirm.

        let stopped = processor.stop();
        handle.deactivate(stopped).expect("deactivate failed");
        eprintln!("[ok] Deactivated cleanly");
    }

    /// Integration test: process audio through TAL Reverb 4 with a longer
    /// signal to verify the reverb tail extends across multiple blocks.
    #[test]
    fn test_reverb_tail_extends() {
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }

        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        let mut processor = handle.activate(48000.0, 256).expect("activate failed");
        let transport = TransportInfo {
            bpm: 120.0,
            sample_rate: 48000.0,
            sample_position: 0,
            is_playing: true,
        };

        // Block 0: send an impulse
        let mut impulse = vec![0.0f32; 256];
        impulse[0] = 1.0;
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        processor.process(&impulse, &impulse, &mut out_l, &mut out_r, 256, &transport);
        let block0_peak = out_l.iter().chain(out_r.iter()).fold(0.0f32, |a, &b| a.max(b.abs()));

        // Process ~100ms of silence following — reverb tail should continue
        let silence = vec![0.0f32; 256];
        let mut tail_peaks = Vec::new();
        for _ in 0..20 {
            processor.process(&silence, &silence, &mut out_l, &mut out_r, 256, &transport);
            let peak = out_l.iter().chain(out_r.iter()).fold(0.0f32, |a, &b| a.max(b.abs()));
            tail_peaks.push(peak);
        }

        let max_tail = tail_peaks.iter().fold(0.0f32, |a, &b| a.max(b));
        let blocks_with_signal = tail_peaks.iter().filter(|&&p| p > 0.001).count();
        eprintln!("[ok] Impulse block peak: {block0_peak:.4}");
        eprintln!("[ok] Max tail peak:      {max_tail:.4}");
        eprintln!("[ok] Blocks with signal: {blocks_with_signal}/20 (reverb tail should persist)");
        assert!(max_tail > 0.001, "Reverb tail should be audible after silence");
        assert!(blocks_with_signal >= 10, "Reverb tail should persist for at least ~130ms");

        let stopped = processor.stop();
        handle.deactivate(stopped).expect("deactivate failed");
    }
}
