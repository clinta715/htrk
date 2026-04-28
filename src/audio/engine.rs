use std::sync::Arc;

use ringbuf::{traits::*, HeapRb, HeapCons, HeapProd};

use crate::audio::commands::{AudioCommand, InterpolationType};
use crate::audio::mixer;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::sequencer_engine::SequencerEngine;
use crate::sequencer::module::Module;
use crate::sequencer::module::COMMAND_BUFFER_SIZE;

const OUTPUT_SAMPLE_RATE: u32 = 48000;
const BUFFER_SIZE: usize = 256;

pub struct AudioEngine {
    sequencer: SequencerEngine,
    module: Option<Arc<Module>>,
    command_rx: HeapCons<AudioCommand>,
    playback_state: Arc<AtomicPlaybackState>,
    master_volume: f32,
    interpolation: InterpolationType,
    output_sample_rate: f64,
    output_channels: u16,
    mix_left: Vec<f32>,
    mix_right: Vec<f32>,
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
        master_volume: 1.0,
        interpolation: InterpolationType::Linear,
        output_sample_rate: sample_rate as f64,
        output_channels: channels.max(1),
        mix_left: vec![0.0; BUFFER_SIZE],
        mix_right: vec![0.0; BUFFER_SIZE],
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
            let count = CB_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count < 5 {
                eprintln!("[AUDIO] callback #{}: frames={} playing={}", count, frame_count, self.sequencer.state.playing);
            }
        }

        if self.mix_left.len() != frame_count {
            self.mix_left.resize(frame_count, 0.0);
            self.mix_right.resize(frame_count, 0.0);
        }

        for s in self.mix_left.iter_mut() {
            *s = 0.0;
        }
        for s in self.mix_right.iter_mut() {
            *s = 0.0;
        }

        if self.sequencer.state.playing {
            self.sequencer.advance(frame_count);

            {
                static LOG_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let count = LOG_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count < 20 {
                    let num_active = self.sequencer.voices.iter().filter(|v| v.active).count();
                    eprintln!("[MIX] #{}: active_voices={}", count, num_active);
                    if num_active > 0 {
                        if let Some(v) = self.sequencer.voices.iter().find(|v| v.active) {
                            eprintln!(
                                "  v0: ch={:?} delta={:.6} pos={:.1} vol={:.3} pan={:.3} sample_len={}",
                                v.channel, v.sample_delta, v.position,
                                v.final_volume, v.final_panning,
                                v.sample.as_ref().map(|s| s.len()).unwrap_or(0)
                            );
                        }
                    }
                }
            }

            let muted_channels: Vec<bool> = self.sequencer.state.channels.iter().map(|ch| ch.muted).collect();
            let solo_channels: Vec<bool> = self.sequencer.state.channels.iter().map(|ch| ch.solo).collect();
            let has_solo = solo_channels.iter().any(|&s| s);
            let effective_mute: Vec<bool> = if has_solo {
                muted_channels.iter().zip(solo_channels.iter())
                    .map(|(muted, solo)| *muted || !*solo)
                    .collect()
            } else {
                muted_channels
            };

            mixer::mix_voices(
                &mut self.sequencer.voices,
                &mut self.mix_left[..frame_count],
                &mut self.mix_right[..frame_count],
                self.master_volume,
                self.interpolation,
                &effective_mute,
            );

            {
                static PEAK_LOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let n = PEAK_LOG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n < 20 {
                    let peak_l = self.mix_left[..frame_count].iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                    let peak_r = self.mix_right[..frame_count].iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                    eprintln!("[PEAK] #{}: L={:.6} R={:.6}", n, peak_l, peak_r);
                }
            }

        }

        mixer::brick_wall_limit(
            &mut self.mix_left[..frame_count],
            &mut self.mix_right[..frame_count],
        );

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
            eprintln!("[AUDIO CMD] {:?}", cmd);
            match cmd {
                AudioCommand::Play => {
                    self.sequencer.play();
                    let num_active = self.sequencer.voices.iter().filter(|v| v.active).count();
                    let ch_vols: Vec<u8> = self.sequencer.state.channels.iter().take(4).map(|c| c.channel_volume).collect();
                    let ch_pans: Vec<u8> = self.sequencer.state.channels.iter().take(4).map(|c| c.channel_panning).collect();
                    eprintln!("[AUDIO] Play command: playing={}, module={}, active_voices={}, ch_vol={:?}, ch_pan={:?}",
                        self.sequencer.state.playing, self.module.is_some(), num_active, ch_vols, ch_pans);
                }
                AudioCommand::Stop => {
                    self.sequencer.stop();
                    eprintln!("[AUDIO] Stop command");
                }
                AudioCommand::Pause => {
                    self.sequencer.pause();
                }
                AudioCommand::LoadModule(module) => {
                    let samples_with_data = module.samples.iter().filter(|s| !s.data.is_empty()).count();
                    let fmt = format!("{:?}", module.format);
                    let order_len = module.order_list.len();
                    let first_order = module.order_list.first().copied().unwrap_or(0);
                    let has_non_empty_pattern = module.patterns.iter().any(|p| p.data.iter().any(|row| row.iter().any(|c| !c.is_empty())));
                    self.module = Some(module.clone());
                    self.sequencer.load_module(module);
                    eprintln!("[AUDIO] Module loaded: format={} {}/{} samples have data, {} instruments, {} patterns ({} rows, non_empty={}), order_list_len={}, first_order={}, BPM={} speed={}",
                        fmt,
                        samples_with_data,
                        self.module.as_ref().map(|m| m.samples.len()).unwrap_or(0),
                        self.module.as_ref().map(|m| m.instruments.len()).unwrap_or(0),
                        self.module.as_ref().map(|m| m.patterns.len()).unwrap_or(0),
                        self.module.as_ref().map(|m| m.patterns.first().map(|p| p.num_rows).unwrap_or(0)).unwrap_or(0),
                        has_non_empty_pattern,
                        order_len,
                        first_order,
                        self.sequencer.state.bpm,
                        self.sequencer.state.speed);
                }
                AudioCommand::SetMasterVolume(vol) => {
                    self.master_volume = vol.clamp(0.0, 2.0);
                }
                AudioCommand::SetInterpolation(interp) => {
                    self.interpolation = interp;
                }
                AudioCommand::PlayFrom { order, row } => {
                    self.sequencer.play_from(order, row);
                }
                AudioCommand::SetBPM(bpm) => {
                    self.sequencer.state.bpm = bpm;
                    self.sequencer.state.samples_per_tick =
                        self.output_sample_rate * 5.0 / (bpm as f64 * 2.0);
                }
                AudioCommand::SetSpeed(speed) => {
                    self.sequencer.state.speed = speed;
                }
                AudioCommand::SetGlobalVolume(vol) => {
                    self.sequencer.state.global_volume = vol;
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
                AudioCommand::SetPatternCell { order, row, channel, cell } => {
                    let _ = (order, row, channel, cell);
                }
                AudioCommand::SeekTo { order, row } => {
                    self.sequencer.play_from(order, row);
                }
            }
        }
    }

    fn update_playback_state(&self) {
        let state = &self.sequencer.state;
        self.playback_state
            .playing
            .store(state.playing, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .bpm
            .store(state.bpm, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .speed
            .store(state.speed, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .current_order
            .store(state.current_order as u16, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .current_row
            .store(state.current_row as u16, std::sync::atomic::Ordering::Relaxed);
        self.playback_state
            .current_pattern
            .store(state.current_pattern as u16, std::sync::atomic::Ordering::Relaxed);

        let active = self.sequencer.voices.iter().filter(|v| v.active).count();
        self.playback_state
            .active_voices
            .store(active as u8, std::sync::atomic::Ordering::Relaxed);
    }

    fn capture_monitoring(&self, frame_count: usize) {
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

        let mut ch_peaks = [0.0f32; crate::audio::playback_state::MAX_CHANNELS];
        for voice in &self.sequencer.voices {
            if voice.active {
                if let Some(ch) = voice.channel {
                    if ch < ch_peaks.len() {
                        let vol = voice.final_volume * self.master_volume;
                        if vol > ch_peaks[ch] {
                            ch_peaks[ch] = vol;
                        }
                    }
                }
            }
        }
        for (ch, peak) in ch_peaks.iter().enumerate() {
            self.playback_state.channel_peaks[ch].store(peak.to_bits(), std::sync::atomic::Ordering::Relaxed);
        }

        self.playback_state.write_scope(&self.mix_left[..frame_count], &self.mix_right[..frame_count]);
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

        let voice = &mut self.sequencer.voices[0];
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
        let (mut engine, _sender) = create_engine_and_sender(state.clone(), 48000, 2);
        assert_eq!(engine.sequencer.voices.len(), MAX_VOICES);
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
}
