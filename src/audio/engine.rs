use std::sync::Arc;

use ringbuf::{traits::*, HeapRb, HeapCons, HeapProd};

use crate::audio::commands::{AudioCommand, InterpolationType, LimiterMode};
use crate::audio::mixer;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::sendfx;
use crate::audio::sequencer_engine::SequencerEngine;
use crate::sequencer::effect::SendEffectType;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::sequencer::module::Module;
use crate::sequencer::module::COMMAND_BUFFER_SIZE;
use crate::sequencer::module::DEFAULT_CHANNELS;
use crate::sequencer::instrument::NewNoteAction;
use crate::sequencer::note::Note;

const OUTPUT_SAMPLE_RATE: u32 = 48000;
const BUFFER_SIZE: usize = 256;
pub(crate) const PREVIEW_VOICE_INDEX: usize = 255;

pub struct AudioEngine {
    sequencer: SequencerEngine,
    module: Option<Arc<Module>>,
    command_rx: HeapCons<AudioCommand>,
    playback_state: Arc<AtomicPlaybackState>,
    master_volume: f32,
    interpolation: InterpolationType,
    limiter_mode: LimiterMode,
    limiter_gain: f32,
    output_sample_rate: f64,
    output_channels: u16,
    mix_left: Vec<f32>,
    mix_right: Vec<f32>,
    ch_mix: Vec<f32>,
    pre_ch_mix: Vec<f32>,
    send_buses: Vec<SendBus>,
    muted_cache: Vec<bool>,
    solo_cache: Vec<bool>,
    effective_mute_cache: Vec<bool>,
    ch_peak_cache: Vec<f32>,
    send_fx_left: Vec<f32>,
    send_fx_right: Vec<f32>,
}

struct SendBus {
    buffer: Vec<f32>, // stereo-interleaved: [L0, R0, L1, R1, ...]
    return_level: f32,
    pre_fader: bool,
    effect: Option<Box<dyn crate::audio::sendfx::SendEffect>>,
}

pub struct CommandSender {
    tx: HeapProd<AudioCommand>,
}

impl CommandSender {
    pub fn send(&mut self, cmd: AudioCommand) -> bool {
        self.tx.try_push(cmd).is_ok()
    }
}

pub fn create_engine_and_sender(
    playback_state: Arc<AtomicPlaybackState>,
    sample_rate: u32,
    channels: u16,
) -> (AudioEngine, CommandSender) {
    let rb = HeapRb::<AudioCommand>::new(COMMAND_BUFFER_SIZE);
    let (tx, rx) = rb.split();

    let engine = AudioEngine {
        sequencer: SequencerEngine::new(sample_rate as f64),
        module: None,
        command_rx: rx,
        playback_state,
        master_volume: 0.5,
        interpolation: InterpolationType::Linear,
        limiter_mode: LimiterMode::HardClip,
        limiter_gain: 1.0,
        output_sample_rate: sample_rate as f64,
        output_channels: channels.max(1),
        mix_left: vec![0.0; BUFFER_SIZE],
        mix_right: vec![0.0; BUFFER_SIZE],
        ch_mix: vec![0.0; DEFAULT_CHANNELS * BUFFER_SIZE * 2],
        pre_ch_mix: vec![0.0; DEFAULT_CHANNELS * BUFFER_SIZE * 2],
        muted_cache: vec![false; DEFAULT_CHANNELS],
        solo_cache: vec![false; DEFAULT_CHANNELS],
        effective_mute_cache: vec![false; DEFAULT_CHANNELS],
        ch_peak_cache: vec![0.0; DEFAULT_CHANNELS],
        send_fx_left: vec![0.0; BUFFER_SIZE],
        send_fx_right: vec![0.0; BUFFER_SIZE],
        send_buses: {
            let configs = [SendEffectType::Delay, SendEffectType::Reverb, SendEffectType::None, SendEffectType::None];
            let returns = [0.5, 0.0, 0.0, 0.0];
            let mut buses = Vec::with_capacity(NUM_SEND_BUSES);
            for (i, &effect_type) in configs.iter().enumerate() {
                buses.push(SendBus {
                    buffer: vec![0.0; BUFFER_SIZE * 2],
                    return_level: returns[i],
                    pre_fader: false,
                    effect: sendfx::create_send_effect(effect_type, sample_rate as f32),
                });
            }
            buses
        },
    };

    let sender = CommandSender { tx };
    (engine, sender)
}

impl AudioEngine {
    pub fn process_callback(&mut self, output: &mut [f32]) {
        self.process_commands();

        let frame_count = output.len() / self.output_channels as usize;
        if frame_count == 0 {
            return;
        }

        {
            static CB_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let _count = CB_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            #[cfg(feature = "audio_debug")]
            if _count < 3 {
                debug_log!("[CALLBACK] #{} playing={}", _count, self.sequencer.state.playing);
            }
        }

        if self.mix_left.len() != frame_count {
            self.mix_left.resize(frame_count, 0.0);
            self.mix_right.resize(frame_count, 0.0);
            self.send_fx_left.resize(frame_count, 0.0);
            self.send_fx_right.resize(frame_count, 0.0);
            let num_ch = self.sequencer.state.channels.len();
            let total_size = num_ch * frame_count * 2;
            self.ch_mix.resize(total_size, 0.0);
            self.pre_ch_mix.resize(total_size, 0.0);
            for bus in &mut self.send_buses {
                bus.buffer.resize(frame_count * 2, 0.0);
            }
        }

        // Ensure ch_mix buffers match sequencer channel count
        let num_ch = self.sequencer.state.channels.len();
        let total_size = num_ch * frame_count * 2;
        if self.ch_mix.len() != total_size {
            self.ch_mix.resize(total_size, 0.0);
        }
        if self.pre_ch_mix.len() != total_size {
            self.pre_ch_mix.resize(total_size, 0.0);
        }

        for s in self.mix_left.iter_mut() {
            *s = 0.0;
        }
        for s in self.mix_right.iter_mut() {
            *s = 0.0;
        }
        for s in self.ch_mix.iter_mut() {
            *s = 0.0;
        }
        for s in self.pre_ch_mix.iter_mut() {
            *s = 0.0;
        }

        // Compute mute/solo state once per callback (cannot change mid-buffer)
        let num_ch = self.sequencer.state.channels.len();
        self.muted_cache.resize(num_ch, false);
        self.solo_cache.resize(num_ch, false);
        self.effective_mute_cache.resize(num_ch, false);
        for (i, ch) in self.sequencer.state.channels.iter().enumerate() {
            self.muted_cache[i] = ch.muted;
            self.solo_cache[i] = ch.solo;
        }
        let has_solo = self.solo_cache.iter().any(|&s| s);
        for (i, (&muted, &solo)) in self.muted_cache.iter().zip(self.solo_cache.iter()).enumerate() {
            self.effective_mute_cache[i] = muted || (has_solo && !solo);
        }

        if self.sequencer.state.playing {
            let mut samples_done = 0;
            while samples_done < frame_count && self.sequencer.state.playing {
                let samples_per_tick = self.sequencer.state.clock.samples_per_tick.max(1.0);

                // If sample_counter reached or exceeded samples_per_tick, it's time for a new tick
                if self.sequencer.state.clock.sample_counter >= samples_per_tick {
                    self.sequencer.process_tick();
                    self.sequencer.state.clock.sample_counter -= samples_per_tick;

                    if !self.sequencer.state.playing {
                        break;
                    }
                }

                let samples_remaining_in_tick = samples_per_tick - self.sequencer.state.clock.sample_counter;
                let samples_remaining_in_buffer = (frame_count - samples_done) as f64;

                let chunk_f = samples_remaining_in_tick.min(samples_remaining_in_buffer);
                let chunk = chunk_f.ceil() as usize;
                let chunk = chunk.min(frame_count - samples_done);

                if chunk == 0 {
                    break;
                }

                mixer::mix_voices_per_channel(
                    &mut self.sequencer.voice_pool.voices,
                    &mut self.mix_left[samples_done..samples_done + chunk],
                    &mut self.mix_right[samples_done..samples_done + chunk],
                    &mut self.ch_mix,
                    &mut self.pre_ch_mix,
                    samples_done,
                    chunk,
                    frame_count,
                    self.master_volume,
                    self.interpolation,
                    &self.effective_mute_cache,
                    self.output_sample_rate as f32,
                    num_ch,
                );

                self.sequencer.state.clock.sample_counter += chunk as f64;
                samples_done += chunk;
            }
        } else {
            // Sequencer is stopped, but still render any active voices so that
            // preview/jam notes (and release tails) are audible.
            mixer::mix_voices_per_channel(
                &mut self.sequencer.voice_pool.voices,
                &mut self.mix_left[..frame_count],
                &mut self.mix_right[..frame_count],
                &mut self.ch_mix,
                &mut self.pre_ch_mix,
                0,
                frame_count,
                frame_count,
                self.master_volume,
                self.interpolation,
                &self.effective_mute_cache,
                self.output_sample_rate as f32,
                num_ch,
            );
        }
        #[cfg(feature = "audio_debug")]
        {
            static PEAK_LOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = PEAK_LOG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 20 {
                let peak_l = self.mix_left[..frame_count].iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                let peak_r = self.mix_right[..frame_count].iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                debug_log!("[PEAK] #{}: L={:.6} R={:.6}", n, peak_l, peak_r);
            }
        }

        // ── Send bus processing ──
        if !self.send_buses.is_empty() {
            let bpm = self.sequencer.state.clock.bpm;
            let sample_rate = self.output_sample_rate as f32;
            let channels = &self.sequencer.state.channels;

            for bus in self.send_buses.iter_mut() {
                bus.buffer[..frame_count * 2].fill(0.0);
            }

            // Tap channels into send buses
            for ch in 0..channels.len() {
                for (si, bus) in self.send_buses.iter_mut().enumerate() {
                    let level = channels[ch].send_levels[si] * channels[ch].auto_send_factor[si];
                    if level <= 0.0 { continue; }
                    let src = if bus.pre_fader {
                        &self.pre_ch_mix
                    } else {
                        &self.ch_mix
                    };
                    let base = ch * 2 * frame_count;
                    for i in 0..frame_count {
                        let src_idx = base + i;
                        let dst_idx = i * 2;
                        bus.buffer[dst_idx] += src[src_idx] * level;
                        bus.buffer[dst_idx + 1] += src[src_idx + frame_count] * level;
                    }
                }
            }

            // Process each bus through its effect and mix back
            for bus in &mut self.send_buses {
                if let Some(ref mut fx) = bus.effect {
                    let left = &mut self.send_fx_left[..frame_count];
                    let right = &mut self.send_fx_right[..frame_count];
                    for i in 0..frame_count {
                        left[i] = bus.buffer[i * 2];
                        right[i] = bus.buffer[i * 2 + 1];
                    }
                    fx.process(left, right, bpm, sample_rate);
                    for i in 0..frame_count {
                        bus.buffer[i * 2] = left[i];
                        bus.buffer[i * 2 + 1] = right[i];
                    }
                }
                if bus.return_level > 0.0 {
                    let rl = bus.return_level;
                    for i in 0..frame_count {
                        self.mix_left[i] += bus.buffer[i * 2] * rl;
                        self.mix_right[i] += bus.buffer[i * 2 + 1] * rl;
                    }
                }
            }
        }

        self.apply_pending_send_params();

        match self.limiter_mode {
            LimiterMode::HardClip => {
                mixer::buffer_wide_limit(
                    &mut self.mix_left[..frame_count],
                    &mut self.mix_right[..frame_count],
                );
            }
            LimiterMode::SoftKnee => {
                mixer::soft_knee_limit(
                    &mut self.mix_left[..frame_count],
                    &mut self.mix_right[..frame_count],
                );
            }
            LimiterMode::SoftKneeSmooth => {
                mixer::soft_knee_limit_smoothed(
                    &mut self.mix_left[..frame_count],
                    &mut self.mix_right[..frame_count],
                    &mut self.limiter_gain,
                );
            }
        }

        self.capture_monitoring(frame_count);

        let ch = self.output_channels as usize;
        for (i, frame) in output.chunks_exact_mut(ch).enumerate() {
            if i < frame_count {
                frame[0] = self.mix_left[i];
                if ch > 1 {
                    frame[1] = self.mix_right[i];
                }
                for c in 2..ch {
                    frame[c] = 0.0;
                }
            }
        }

        self.update_playback_state();
    }

    fn process_commands(&mut self) {
        while let Some(cmd) = self.command_rx.try_pop() {
            #[cfg(feature = "audio_debug")]
            debug_log!("[AUDIO CMD] {:?}", cmd);
            match cmd {
                AudioCommand::Play => {
                    self.sequencer.play();
                    #[cfg(feature = "audio_debug")]
                    {
                        let num_active = self.sequencer.voice_pool.voices.iter().filter(|v| v.active).count();
                        let ch_vols: Vec<u8> = self.sequencer.state.channels.iter().take(4).map(|c| c.channel_volume).collect();
                        let ch_pans: Vec<u8> = self.sequencer.state.channels.iter().take(4).map(|c| c.channel_panning).collect();
                        debug_log!("[AUDIO] Play command: playing={}, module={}, active_voices={}, ch_vol={:?}, ch_pan={:?}",
                            self.sequencer.state.playing, self.module.is_some(), num_active, ch_vols, ch_pans);
                    }
                }
                AudioCommand::Stop => {
                    self.sequencer.stop();
                    #[cfg(feature = "audio_debug")]
                    debug_log!("[AUDIO] Stop command");
                }
                AudioCommand::Pause => {
                    self.sequencer.pause();
                }
                AudioCommand::LoadModule(module) => {
                    #[cfg(feature = "audio_debug")]
                    let _debug_info = {
                        let samples_with_data = module.samples.iter().filter(|s| !s.data.is_empty()).count();
                        let fmt = format!("{:?}", module.format);
                        let order_len = module.order_list.len();
                        let first_order = module.order_list.first().copied().unwrap_or(0);
                        let has_non_empty_pattern = module.patterns.iter().any(|p| p.data.iter().any(|row| row.iter().any(|c| !c.is_empty())));
                        (fmt, samples_with_data, order_len, first_order, has_non_empty_pattern)
                    };
                    self.module = Some(module.clone());
                    self.sequencer.load_module(module.clone());
                    // Rebuild send buses from module config
                    let buf_size = self.mix_left.len().max(BUFFER_SIZE);
                    self.send_buses = module.send_bus_config.iter().enumerate().map(|(i, &effect_type)| {
                        let bus = SendBus {
                            buffer: vec![0.0; buf_size * 2],
                            return_level: module.send_return_levels.get(i).copied().unwrap_or(0.0),
                            pre_fader: module.send_pre_fader.get(i).copied().unwrap_or(false),
                            effect: sendfx::create_send_effect(effect_type, self.output_sample_rate as f32),
                        };
                        bus
                    }).collect();
                    #[cfg(feature = "audio_debug")]
                    debug_log!("[AUDIO] Module loaded: format={} {}/{} samples have data, {} instruments, {} patterns ({} rows, non_empty={}), order_list_len={}, first_order={}, BPM={} speed={}",
                        _debug_info.0,
                        _debug_info.1,
                        self.module.as_ref().map(|m| m.samples.len()).unwrap_or(0),
                        self.module.as_ref().map(|m| m.instruments.len()).unwrap_or(0),
                        self.module.as_ref().map(|m| m.patterns.len()).unwrap_or(0),
                        self.module.as_ref().map(|m| m.patterns.first().map(|p| p.num_rows).unwrap_or(0)).unwrap_or(0),
                        _debug_info.4,
                        _debug_info.2,
                        _debug_info.3,
                        self.sequencer.state.clock.bpm,
                        self.sequencer.state.clock.speed);
                }
                AudioCommand::SetMasterVolume(vol) => {
                    self.master_volume = vol.clamp(0.0, 2.0);
                }
                AudioCommand::PlayFrom { order, row } => {
                    self.sequencer.play_from(order, row);
                }
                AudioCommand::SetBPM(bpm) => {
                    self.sequencer.state.clock.set_bpm(bpm);
                }
                AudioCommand::SetSpeed(speed) => {
                    self.sequencer.state.clock.set_speed(speed);
                }
                AudioCommand::SetChannelMuted { channel, muted } => {
                    if channel < self.sequencer.state.channels.len() {
                        self.sequencer.state.channels[channel].muted = muted;
                    }
                }
                AudioCommand::SetChannelSolo { channel, solo } => {
                    if channel < self.sequencer.state.channels.len() {
                        self.sequencer.state.channels[channel].solo = solo;
                    }
                }
                AudioCommand::SetPlayMode(mode) => {
                    self.sequencer.state.play_mode = mode;
                }
                AudioCommand::SetInterpolation(interp) => {
                    self.interpolation = interp;
                }
                AudioCommand::SetLimiterMode(mode) => {
                    self.limiter_mode = mode;
                    self.limiter_gain = 1.0;
                }
                AudioCommand::TriggerPreviewNote { sample_index, note_key, volume, panning } => {
                    self.trigger_preview_note(sample_index, note_key, volume, panning);
                }
                AudioCommand::PreviewBuffer { data, sample_rate, note_key, volume, panning } => {
                    self.trigger_preview_buffer(data, sample_rate, note_key, volume, panning);
                }
                AudioCommand::SetSendLevel { channel, send_index, level } => {
                    if channel < self.sequencer.state.channels.len() && send_index < self.send_buses.len() {
                        self.sequencer.state.channels[channel].send_levels[send_index] = level;
                    }
                }
                AudioCommand::SetSendReturnLevel { send_index, level } => {
                    if send_index < self.send_buses.len() {
                        self.send_buses[send_index].return_level = level;
                    }
                }
                AudioCommand::SetSendEffectType { send_index, effect_type } => {
                    if send_index < self.send_buses.len() {
                        self.send_buses[send_index].effect = sendfx::create_send_effect(effect_type, self.output_sample_rate as f32);
                    }
                }
                AudioCommand::SetSendFxParam { send_index, param, value } => {
                    if send_index < self.send_buses.len() {
                        if let Some(ref mut fx) = self.send_buses[send_index].effect {
                            fx.set_param(param, value);
                        }
                    }
                }
                AudioCommand::SetSendPreFader { send_index, pre_fader } => {
                    if send_index < self.send_buses.len() {
                        self.send_buses[send_index].pre_fader = pre_fader;
                    }
                }
            }
        }
    }

    fn update_playback_state(&self) {
        use crate::audio::playback_state::MAX_CHANNELS;
        use crate::sequencer::note::Note;

        let state = &self.sequencer.state;
        self.playback_state
            .playing
            .store(state.playing, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .bpm
            .store(state.clock.bpm, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .speed
            .store(state.clock.speed, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .current_order
            .store(state.current_order as u16, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .current_row
            .store(state.current_row as u16, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .current_pattern
            .store(state.current_pattern as u16, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .current_tick
            .store(state.clock.current_tick, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .set_play_mode(state.play_mode);

        let active = self.sequencer.voice_pool.voices.iter().filter(|v| v.active).count();
        self.playback_state
            .active_voices
            .store(active as u8, std::sync::atomic::Ordering::Relaxed);

        let num_ch = state.channels.len();
        for ch in 0..num_ch.min(MAX_CHANNELS) {
            let ch_state = &state.channels[ch];
            let note_val = match ch_state.last_note {
                Note::On(key) => key as u16,
                Note::Off => 0xFF,
                Note::Cut => 0xFE,
                Note::Fade => 0xFD,
                Note::None => 0,
            };
            self.playback_state.set_channel_note(ch, note_val);
            self.playback_state.set_channel_instrument(ch, ch_state.last_instrument as u16);
        }

        self.playback_state.clear_all_sample_positions();
        self.playback_state.clear_all_env_positions();
        let preview_voice = &self.sequencer.voice_pool.voices[PREVIEW_VOICE_INDEX];
        if preview_voice.active {
            if let Some(si) = preview_voice.sample_index {
                self.playback_state.set_preview_sample_position(Some(preview_voice.position));
                self.playback_state.set_preview_sample_index(Some(si));
            }
        }
        for voice in &self.sequencer.voice_pool.voices {
            if voice.active {
                if let (Some(ch), Some(si)) = (voice.channel, voice.sample_index) {
                    if ch < MAX_CHANNELS {
                        self.playback_state.set_channel_sample_position(ch, Some(voice.position));
                        self.playback_state.set_channel_sample_index(ch, Some(si));
                    }
                }
                if let Some(ch) = voice.channel {
                    if ch < MAX_CHANNELS {
                        if let Some(instr) = voice.instrument_index {
                            self.playback_state.set_channel_env_instrument(ch, Some(instr));
                        }
                        let env_sets: [(usize, Option<&crate::audio::voice::EnvelopeState>); 4] = [
                            (0, voice.vol_env.as_ref()),
                            (1, voice.pan_env.as_ref()),
                            (2, voice.pitch_env.as_ref()),
                            (3, voice.filter_env.as_ref()),
                        ];
                        for (env_type, env_opt) in &env_sets {
                            if let Some(env) = env_opt {
                                if !env.finished {
                                    self.playback_state.set_channel_env_pos(*env_type, ch, Some(env.position));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn capture_monitoring(&mut self, frame_count: usize) {
        let mut peak_l = 0.0f32;
        let mut peak_r = 0.0f32;
        for i in 0..frame_count {
            let al = self.mix_left[i].abs();
            let ar = self.mix_right[i].abs();
            if al > peak_l { peak_l = al; }
            if ar > peak_r { peak_r = ar; }
        }
        self.playback_state.master_peak_left.store(peak_l.to_bits(), std::sync::atomic::Ordering::Relaxed);
        self.playback_state.master_peak_right.store(peak_r.to_bits(), std::sync::atomic::Ordering::Relaxed);

        let num_ch = self.sequencer.state.channels.len();
        self.ch_peak_cache.resize(num_ch, 0.0);
        self.ch_peak_cache.fill(0.0);
        for voice in &self.sequencer.voice_pool.voices {
            if voice.active {
                if let Some(ch) = voice.channel {
                    if ch < self.ch_peak_cache.len() {
                        let vol = voice.final_volume * self.master_volume;
                        if vol > self.ch_peak_cache[ch] {
                            self.ch_peak_cache[ch] = vol;
                        }
                    }
                }
            }
        }
        for (ch, peak) in self.ch_peak_cache.iter().enumerate() {
            if ch < crate::audio::playback_state::MAX_CHANNELS {
                self.playback_state.channel_peaks[ch].store(peak.to_bits(), std::sync::atomic::Ordering::Relaxed);
            }
        }

        for ch in 0..num_ch.min(crate::audio::playback_state::MAX_CHANNELS) {
            let base = ch * 2 * frame_count;
            let left = &self.ch_mix[base..base + frame_count];
            let right = &self.ch_mix[base + frame_count..base + 2 * frame_count];
            self.playback_state.write_channel_scope(
                ch,
                left,
                right,
            );
        }
        self.playback_state.finish_channel_scope_write(frame_count);
    }

    #[allow(dead_code)]
    pub fn trigger_note(
        &mut self,
        sample_index: usize,
        note: crate::sequencer::note::Note,
        volume: f32,
        panning: f32,
        sample_offset: usize,
    ) {
        let module = match &self.module {
            Some(m) => m.clone(),
            None => return,
        };

        if sample_index >= module.samples.len() {
            return;
        }

        let sample = &module.samples[sample_index];
        self.trigger_note_with_sample(sample, note, volume, panning, sample_offset);
    }

    #[allow(dead_code)]
    pub fn trigger_note_with_sample(
        &mut self,
        sample: &crate::sequencer::sample::Sample,
        note: crate::sequencer::note::Note,
        volume: f32,
        panning: f32,
        sample_offset: usize,
    ) {
        let _note_key = match note {
            crate::sequencer::note::Note::On(key) => key,
            _ => return,
        };

        let freq = match note.frequency() {
            Some(f) => f,
            None => return,
        };

        let playback_freq = compute_playback_frequency(
            freq,
            sample.sample_rate,
            sample.relative_note,
            sample.fine_tune,
        );

        let voice = &mut self.sequencer.voice_pool.voices[0];
        voice.trigger(
            sample.data.clone(),
            sample.sample_rate as f64,
            sample.loop_type,
            sample.loop_start,
            sample.loop_end,
            playback_freq,
            self.output_sample_rate,
            volume,
            panning,
            sample_offset,
            None,
            None,
            note,
            crate::sequencer::instrument::NewNoteAction::NoteCut,
            0,
        );
    }

    pub fn trigger_preview_note(&mut self, sample_index: usize, note_key: u8, volume: f32, panning: f32) {
        let module = match &self.module {
            Some(m) => m,
            None => return,
        };
        if sample_index >= module.samples.len() {
            return;
        }
        let sample = &module.samples[sample_index];
        let note = Note::On(note_key);
        let freq = match note.frequency() {
            Some(f) => f,
            None => return,
        };
        let playback_freq = compute_playback_frequency(
            freq,
            sample.sample_rate,
            sample.relative_note,
            sample.fine_tune,
        );
        let voice = &mut self.sequencer.voice_pool.voices[PREVIEW_VOICE_INDEX];
        voice.trigger(
            sample.data.clone(),
            sample.sample_rate as f64,
            sample.loop_type,
            sample.loop_start,
            sample.loop_end,
            playback_freq,
            self.output_sample_rate,
            volume,
            panning,
            0,
            None,
            Some(sample_index as u8),
            note,
            NewNoteAction::NoteCut,
            0,
        );
    }

    pub fn trigger_preview_buffer(&mut self, data: Arc<Vec<f32>>, sample_rate: u32, note_key: u8, volume: f32, panning: f32) {
        if data.is_empty() {
            return;
        }
        let note = Note::On(note_key);
        let freq = match note.frequency() {
            Some(f) => f,
            None => return,
        };
        let playback_freq = compute_playback_frequency(freq, sample_rate, 0, 0);
        let voice = &mut self.sequencer.voice_pool.voices[PREVIEW_VOICE_INDEX];
        voice.trigger(
            data,
            sample_rate as f64,
            crate::sequencer::sample::LoopType::None,
            0,
            0,
            playback_freq,
            self.output_sample_rate,
            volume,
            panning,
            0,
            None,
            None,
            note,
            NewNoteAction::NoteCut,
            0,
        );
    }

    fn apply_pending_send_params(&mut self) {
        for (send_index, param, value) in self.sequencer.pending_send_fx_params.drain(..) {
            if send_index < self.send_buses.len() {
                if let Some(ref mut fx) = self.send_buses[send_index].effect {
                    fx.set_param(param, value);
                }
            }
        }
    }
}

#[allow(dead_code)]
pub fn compute_playback_frequency(
    note_freq: f64,
    sample_c5speed: u32,
    relative_note: i8,
    fine_tune: i8,
) -> f64 {
    let base_rate = crate::sequencer::module::BASE_NOTE_RATE;
    let sample_rate = sample_c5speed as f64;
    let pitch_multiplier = 2.0_f64.powf(
        (relative_note as f64 + fine_tune as f64 / 128.0) / 12.0,
    );
    (note_freq / base_rate) * sample_rate * pitch_multiplier
}

#[allow(dead_code)]
pub struct AudioDevice {
    stream: Option<cpal::Stream>,
    sample_rate: u32,
}

#[allow(dead_code)]
impl AudioDevice {
    pub fn new() -> crate::errors::AudioResult<Self> {
        Ok(AudioDevice {
            stream: None,
            sample_rate: OUTPUT_SAMPLE_RATE,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn stop(&mut self) {
        self.stream = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::module::MAX_VOICES;

    #[test]
    fn create_engine_and_sender_works() {
        let state = Arc::new(AtomicPlaybackState::default());
        let (engine, _sender) = create_engine_and_sender(state.clone(), 48000, 2);
        assert_eq!(engine.sequencer.voice_pool.voices.len(), MAX_VOICES);
        assert!(engine.module.is_none());
    }

    #[test]
    fn send_play_command() {
        let state = Arc::new(AtomicPlaybackState::default());
        let (mut engine, mut sender) = create_engine_and_sender(state.clone(), 48000, 2);

        let mut module = crate::sequencer::Module::default();
        module.order_list = vec![0];
        module.patterns.push(crate::sequencer::Pattern::new(64));
        sender.send(AudioCommand::LoadModule(Arc::new(module)));
        engine.process_callback(&mut [0.0f32; 512]);

        assert!(sender.send(AudioCommand::Play));

        let mut output = vec![0.0f32; 512];
        engine.process_callback(&mut output);

        assert!(state.playing.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn send_stop_command() {
        let state = Arc::new(AtomicPlaybackState::default());
        let (mut engine, mut sender) = create_engine_and_sender(state.clone(), 48000, 2);

        sender.send(AudioCommand::Play);
        engine.process_callback(&mut [0.0f32; 512]);

        sender.send(AudioCommand::Stop);
        engine.process_callback(&mut [0.0f32; 512]);

        assert!(!state.playing.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn process_callback_outputs_silence_when_not_playing() {
        let state = Arc::new(AtomicPlaybackState::default());
        let (mut engine, _) = create_engine_and_sender(state, 48000, 2);

        let mut output = vec![1.0f32; 512];
        engine.process_callback(&mut output);

        assert!(output.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn load_module_command() {
        let state = Arc::new(AtomicPlaybackState::default());
        let (mut engine, mut sender) = create_engine_and_sender(state, 48000, 2);

        let module = Arc::new(crate::sequencer::Module::default());
        sender.send(AudioCommand::LoadModule(module));

        engine.process_callback(&mut [0.0f32; 512]);

        assert!(engine.module.is_some());
    }

    #[test]
    fn command_buffer_handles_backpressure_without_drop() {
        let state = Arc::new(AtomicPlaybackState::default());
        let (mut engine, mut sender) = create_engine_and_sender(state.clone(), 48000, 2);

        // Fill buffer to capacity — leave 1 slot for our test command
        let fill_count = COMMAND_BUFFER_SIZE - 1;
        for i in 0..fill_count {
            assert!(
                sender.send(AudioCommand::SetMasterVolume(0.5)),
                "fill iteration {i}: buffer should accept {fill_count} entries"
            );
        }

        // The buffer is now full. Send with retry (simulating send_command pattern).
        let module = Arc::new(crate::sequencer::Module::default());
        let mut delivered = false;
        for _ in 0..=100 {
            if sender.send(AudioCommand::LoadModule(module.clone())) {
                delivered = true;
                break;
            }
            // Drain a bit so the retry can succeed
            engine.process_callback(&mut [0.0f32; 512]);
        }
        assert!(delivered, "LoadModule should eventually deliver after draining");

        // Process remaining commands to ensure LoadModule is consumed
        for _ in 0..(fill_count / 256 + 2) {
            engine.process_callback(&mut [0.0f32; 512]);
        }
        assert!(engine.module.is_some(), "LoadModule must be consumed by engine");
    }

    #[test]
    fn compute_playback_frequency_c5_at_c5speed() {
        let c5_freq = 440.0 * 2.0_f64.powf((60.0 - 69.0) / 12.0);
        let freq = compute_playback_frequency(c5_freq, 8363, 0, 0);
        assert!((freq - 8363.0).abs() < 1.0,
            "C-5 at c5speed=8363 should produce ~8363 Hz, got {}", freq);
    }

    #[test]
    fn compute_playback_frequency_a4_at_c5speed() {
        let a4_freq = 440.0;
        let freq = compute_playback_frequency(a4_freq, 8363, 0, 0);
        let expected = 8363.0 * 2.0_f64.powf(9.0 / 12.0);
        assert!((freq - expected).abs() < 1.0,
            "A-4 at c5speed=8363 should produce ~{:.1} Hz, got {:.1}", expected, freq);
    }

    #[test]
    fn compute_playback_frequency_with_relative_note() {
        let c5_freq = 440.0 * 2.0_f64.powf((60.0 - 69.0) / 12.0);
        let freq_up = compute_playback_frequency(c5_freq, 8363, 12, 0);
        let freq_octave_up = 8363.0 * 2.0;
        assert!((freq_up - freq_octave_up).abs() < 2.0,
            "C-5 + 1 octave should double frequency, got {} vs expected {}", freq_up, freq_octave_up);
    }

    #[test]
    fn play_with_module() {
        let state = Arc::new(AtomicPlaybackState::default());
        let (mut engine, mut sender) = create_engine_and_sender(state.clone(), 48000, 2);

        let mut module = crate::sequencer::Module::default();
        module.order_list = vec![0];
        let sample_data: Vec<f32> = (0..4800)
            .map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48000.0).sin() as f32)
            .collect();
        module.samples.push(crate::sequencer::Sample {
            data: Arc::new(sample_data),
            sample_rate: 48000,
            ..Default::default()
        });
        module.patterns.push(crate::sequencer::Pattern::new(64));

        sender.send(AudioCommand::LoadModule(Arc::new(module)));
        engine.process_callback(&mut [0.0f32; 512]);

        sender.send(AudioCommand::Play);
        engine.process_callback(&mut [0.0f32; 512]);

        assert!(state.playing.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn preview_note_audible_while_stopped() {
        let state = Arc::new(AtomicPlaybackState::default());
        let (mut engine, mut sender) = create_engine_and_sender(state, 48000, 2);

        let mut module = crate::sequencer::Module::default();
        let sample_data: Vec<f32> = (0..4800)
            .map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48000.0).sin() as f32)
            .collect();
        module.samples.push(crate::sequencer::Sample {
            data: Arc::new(sample_data),
            sample_rate: 48000,
            ..Default::default()
        });
        let preview_idx = module.samples.len() - 1;

        sender.send(AudioCommand::LoadModule(Arc::new(module)));
        engine.process_callback(&mut [0.0f32; 512]); // consume LoadModule

        // Stopped with no active voices -> silence.
        let mut output = vec![0.0f32; 512];
        engine.process_callback(&mut output);
        assert!(output.iter().all(|&s| s == 0.0),
            "should be silent when stopped with no active voices");

        // Trigger a preview note while still stopped.
        engine.trigger_preview_note(preview_idx, 60, 0.75, 0.5);

        // The preview voice must render even though the sequencer is stopped.
        let mut output = vec![0.0f32; 512];
        engine.process_callback(&mut output);
        assert!(output.iter().any(|&s| s != 0.0),
            "preview note must be audible while the sequencer is stopped");
    }
}
