use std::sync::Arc;

use crate::audio::voice::{EnvelopeState, Voice};
use crate::sequencer::effect::Effect;
use crate::sequencer::instrument::{
    DuplicateCheckAction, DuplicateCheckType, Envelope, EnvelopeFlags, EnvelopePoint, NewNoteAction,
};
use crate::sequencer::module::{Module, MAX_VOICES};
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::player::{ChannelState, PlayMode, SequencerState};
use crate::sequencer::sample::{Sample, VibratoWaveform, LoopType};

const VIBRATO_TABLE_SIZE: usize = 64;

const VIBRATO_SINE_TABLE: [f32; VIBRATO_TABLE_SIZE] = [
    0.0, 24.0, 49.0, 74.0, 97.0, 120.0, 141.0, 161.0, 180.0, 197.0, 212.0, 224.0, 235.0,
    244.0, 250.0, 253.0, 255.0, 253.0, 250.0, 244.0, 235.0, 224.0, 212.0, 197.0, 180.0,
    161.0, 141.0, 120.0, 97.0, 74.0, 49.0, 24.0, 0.0, -24.0, -49.0, -74.0, -97.0, -120.0,
    -141.0, -161.0, -180.0, -197.0, -212.0, -224.0, -235.0, -244.0, -250.0, -253.0, -255.0,
    -253.0, -250.0, -244.0, -235.0, -224.0, -212.0, -197.0, -180.0, -161.0, -141.0, -120.0,
    -97.0, -74.0, -49.0, -24.0,
];

const VIBRATO_RAMP_TABLE: [f32; VIBRATO_TABLE_SIZE] = {
    let mut table = [0f32; VIBRATO_TABLE_SIZE];
    let mut i = 0;
    while i < VIBRATO_TABLE_SIZE {
        table[i] = (i as f32) * 255.0 / 63.0 - 128.0;
        i += 1;
    }
    table
};

pub struct SequencerEngine {
    pub state: SequencerState,
    pub voices: Vec<Voice>,
    next_voice: usize,
    module: Option<Arc<Module>>,
    output_sample_rate: f64,
    global_volume: f32,
}

fn quantize_to_semitone(freq: f64) -> f64 {
    let nearest = (freq.log2() * 12.0).round();
    2.0_f64.powf(nearest / 12.0)
}

fn fastrand() -> f32 {
    static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(123456789);
    let mut x = SEED.load(std::sync::atomic::Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    SEED.store(x, std::sync::atomic::Ordering::Relaxed);
    (x >> 40) as f32 / 16777216.0
}

impl SequencerEngine {
    pub fn new(output_sample_rate: f64) -> Self {
        SequencerEngine {
            state: SequencerState::default(),
            voices: vec![Voice::default(); MAX_VOICES],
            next_voice: 0,
            module: None,
            output_sample_rate,
            global_volume: 1.0,
        }
    }

    pub fn load_module(&mut self, module: Arc<Module>) {
        self.stop();
        self.module = Some(module);
    }

    pub fn play(&mut self) {
        if self.module.is_none() {
            return;
        }
        self.stop_playback_state();

        let module = self.module.as_ref().unwrap();
        self.state.bpm = module.initial_bpm;
        self.state.speed = module.initial_speed;
        self.state.global_volume = module.initial_global_volume;
        self.state.master_volume = 1.0;
        self.state.samples_per_tick = compute_samples_per_tick(self.state.bpm, self.output_sample_rate);

        self.state.channels.clear();
        self.state.channels.resize(64, ChannelState::default());
        for i in 0..64 {
            self.state.channels[i].channel_panning = module.channel_panning[i];
            self.state.channels[i].channel_volume = module.channel_volume[i];
        }

        self.state.current_order = 0;
        self.state.current_row = 0;
        self.state.current_pattern = self.get_pattern_for_order(0);
        self.state.pattern_break_row = None;
        self.state.position_jump_order = None;
        self.state.pattern_delay_ticks = 0;
        self.state.row_delay_active = false;
        self.state.pattern_loop_start = None;
        self.state.pattern_loop_count = 0;

        self.state.playing = true;
        self.state.paused = false;
        self.state.current_tick = 0;
        self.state.sample_counter = 0.0;

        self.process_tick_zero();

        self.state.current_tick = 1;
    }

    pub fn play_from(&mut self, order: u16, row: u16) {
        if self.module.is_none() {
            return;
        }
        self.stop_playback_state();

        let module = self.module.as_ref().unwrap();
        self.state.bpm = module.initial_bpm;
        self.state.speed = module.initial_speed;
        self.state.global_volume = module.initial_global_volume;
        self.state.master_volume = 1.0;
        self.state.samples_per_tick = compute_samples_per_tick(self.state.bpm, self.output_sample_rate);

        self.state.channels.clear();
        self.state.channels.resize(64, ChannelState::default());
        for i in 0..64 {
            self.state.channels[i].channel_panning = module.channel_panning[i];
            self.state.channels[i].channel_volume = module.channel_volume[i];
        }

        let max_order = self.get_order_count().saturating_sub(1) as u16;
        self.state.current_order = order.min(max_order);
        self.state.current_row = row as u8;
        self.state.current_pattern = self.get_pattern_for_order(self.state.current_order);
        self.state.current_tick = 0;
        self.state.pattern_break_row = None;
        self.state.position_jump_order = None;
        self.state.pattern_delay_ticks = 0;
        self.state.row_delay_active = false;
        self.state.pattern_loop_start = None;
        self.state.pattern_loop_count = 0;

        self.state.playing = true;
        self.state.paused = false;
        self.state.sample_counter = 0.0;

        self.process_tick_zero();

        self.state.current_tick = 1;
    }

    pub fn stop(&mut self) {
        self.stop_playback_state();
        for voice in &mut self.voices {
            voice.deactivate();
        }
    }

    fn stop_playback_state(&mut self) {
        self.state.playing = false;
        self.state.paused = false;
        self.state.current_tick = 0;
        self.state.sample_counter = 0.0;
    }

    pub fn pause(&mut self) {
        self.state.paused = true;
    }

    #[allow(dead_code)]
    pub fn resume(&mut self) {
        self.state.paused = false;
    }

    pub fn advance(&mut self, samples_to_generate: usize) {
        if !self.state.playing || self.state.paused {
            return;
        }

        let mut samples_remaining = samples_to_generate;

        while samples_remaining > 0 {
            let samples_per_tick = self.state.samples_per_tick;
            if samples_per_tick <= 0.0 {
                break;
            }
            let samples_until_tick = (samples_per_tick - self.state.sample_counter).ceil() as usize;
            if samples_until_tick == 0 {
                self.process_tick();
                self.state.sample_counter = 0.0;
                continue;
            }

            if samples_remaining < samples_until_tick {
                self.state.sample_counter += samples_remaining as f64;
                break;
            }

            samples_remaining -= samples_until_tick;
            self.process_tick();
            self.state.sample_counter = 0.0;
        }
    }

    fn process_tick(&mut self) {
        let tick = self.state.current_tick;
        let speed = self.state.speed;

        if tick == 0 {
            self.process_tick_zero();
        } else {
            self.process_effects_tick();
        }

        self.advance_envelopes();

        self.state.current_tick += 1;

        if self.state.current_tick >= speed {
            self.advance_row();
        }
    }

    fn process_tick_zero(&mut self) {
        let pattern_index = self.state.current_pattern as usize;
        let row = self.state.current_row as usize;

        let cells: Vec<(usize, Cell)> = {
            let module = match self.module.as_ref() {
                Some(m) => m,
                None => return,
            };

            if pattern_index >= module.patterns.len() {
                static STOP_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let n = STOP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n < 3 {
                    eprintln!("[TICK0] STOP: pattern_index={} >= patterns.len()={}, stopping playback", pattern_index, module.patterns.len());
                }
                self.stop();
                return;
            }

            let pattern = &module.patterns[pattern_index];
            if row >= pattern.num_rows {
                self.advance_row();
                return;
            }

            let mut result = Vec::new();
            for ch in 0..64 {
                if ch >= pattern.data[row].len() {
                    break;
                }
                let cell = pattern.data[row][ch];
                if cell.is_empty() {
                    continue;
                }
                result.push((ch, cell));
            }
            result
        };

        let cells_count = cells.len();
        {
            static ZERO_LOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = ZERO_LOG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 5 {
                eprintln!("[TICK0] #{} pat={} row={} cells={}", n, pattern_index, row, cells_count);
            }
        }
        for (ch, cell) in cells {
            self.process_cell(ch, &cell);
        }

        #[cfg(feature = "audio_debug")]
        eprintln!(
            "[TICK0] pat={} row={} cells_processed={} active_voices={}",
            pattern_index,
            row,
            cells_count,
            self.voices.iter().filter(|v| v.active).count()
        );
        #[cfg(feature = "audio_debug")]
        for (i, v) in self.voices.iter().enumerate() {
            if v.active {
                eprintln!(
                    "[VOICE {}] ch={:?} note={:?} sr={:.0} freq={:.2} delta={:.6} vol={:.4} pan={:.4} pos={:.2} sample_len={}",
                    i,
                    v.channel,
                    v.note,
                    v.sample_rate,
                    v.current_frequency,
                    v.sample_delta,
                    v.final_volume,
                    v.final_panning,
                    v.position,
                    v.sample.as_ref().map(|s| s.len()).unwrap_or(0)
                );
            }
        }
    }

    fn process_cell(&mut self, channel: usize, cell: &Cell) {
        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };
        self.process_cell_with_module(channel, cell, &module);
    }

    fn process_cell_with_module(&mut self, channel: usize, cell: &Cell, module: &Module) {
        if channel >= self.state.channels.len() {
            return;
        }

        if cell.instrument.is_some() {
            self.state.channels[channel].last_instrument = cell.instrument.unwrap();
        }

        let instrument_idx = self.state.channels[channel].last_instrument as usize;
        let has_instruments = !module.instruments.is_empty();

        let (sample_idx, remapped_key) = if has_instruments && instrument_idx > 0 && instrument_idx < module.instruments.len() {
            let inst = &module.instruments[instrument_idx];
            match cell.note {
                Note::On(key) if (key as usize) < 120 => {
                    let idx = inst.sample_map[key as usize] as usize;
                    let rk = inst.note_map[key as usize];
                    #[cfg(feature = "audio_debug")]
                    eprintln!(
                        "[CELL] ch={} inst_idx={} key={} sample_map[{}]={} note_map[{}]={} -> sample_idx={}",
                        channel, instrument_idx, key, key, inst.sample_map[key as usize], key, rk, idx
                    );
                    (idx, if rk < 120 { rk } else { key })
                }
                _ => (self.state.channels[channel].last_sample as usize, {
                    match cell.note { Note::On(k) => k, _ => 0 }
                }),
            }
        } else {
            #[cfg(feature = "audio_debug")]
            eprintln!(
                "[CELL] ch={} inst_idx={} has_instruments={} -> sample_idx=inst_idx={}",
                channel, instrument_idx, has_instruments, instrument_idx
            );
            (instrument_idx, match cell.note { Note::On(k) => k, _ => 0 })
        };

        if sample_idx > 0 && sample_idx < module.samples.len() {
            self.state.channels[channel].last_sample = sample_idx as u8;
        }

        let sample = if sample_idx > 0 && sample_idx < module.samples.len() {
            Some(&module.samples[sample_idx])
        } else {
            None
        };

        if cell.instrument.is_some() {
            if let Some(s) = sample {
                self.state.channels[channel].channel_volume = s.default_volume.min(64);
            }
        }

        if let Some(vol) = cell.volume {
            self.apply_volume_column(channel, vol);
        }

        // Apply SetVolume effect BEFORE triggering the note so the voice
        // fires at the correct volume (MOD format Cxx effect)
        if let Effect::SetVolume { volume } = &cell.effect {
            self.state.channels[channel].channel_volume = (*volume).min(64);
            self.state.channels[channel].row_volume = (*volume).min(64);
        }

        let is_tone_portamento = matches!(
            cell.effect,
            Effect::TonePortamento { .. } | Effect::TonePortamentoVolumeSlide { .. }
                | Effect::VolPortamento { .. }
        );

        match cell.note {
            Note::On(key) => {
                self.state.channels[channel].last_note = Note::On(key);

                if is_tone_portamento {
                    let target_freq = self.compute_portamento_target(
                        channel, key, remapped_key, sample, sample_idx, module,
                    );
                    self.state.channels[channel].portamento_target_period = Some(target_freq);
                } else {
                    self.trigger_channel_note(channel, key, remapped_key, sample, sample_idx, cell, module);
                }
            }
            Note::Off => {
                self.handle_note_off(channel);
            }
            Note::Cut => {
                self.cut_channel_voices(channel);
            }
            Note::Fade => {
                self.fade_channel_voices(channel);
            }
            Note::None => {}
        }

        self.apply_effect(channel, &cell.effect, true);
    }

    fn trigger_channel_note(
        &mut self,
        channel: usize,
        note_key: u8,
        remapped_key: u8,
        sample: Option<&Sample>,
        sample_idx: usize,
        cell: &Cell,
        module: &Module,
    ) {
        #[cfg(feature = "audio_debug")]
        eprintln!(
            "[TRIGGER] ch={} key={} sample_idx={} has_sample={}",
            channel, note_key, sample_idx, sample.is_some()
        );
        if sample.is_none() || sample_idx == 0 {
            #[cfg(feature = "audio_debug")]
            eprintln!("[TRIGGER] EARLY RETURN: sample_idx={} has_sample={}", sample_idx, sample.is_some());
            return;
        }
        let sample = sample.unwrap();

        {
            static TRIG_LOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = TRIG_LOG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 20 {
                let freq = match Note::On(remapped_key).frequency() {
                    Some(f) => f,
                    None => 0.0,
                };
                let playback_freq = compute_playback_frequency(freq, sample.sample_rate, sample.relative_note, sample.fine_tune);
                let delta = if self.output_sample_rate > 0.0 { playback_freq / self.output_sample_rate } else { 0.0 };
                eprintln!(
                    "[TRIGGER] #{} ch={} key={} remapped={} sample_idx={} data_len={} sr={} delta={:.6} pb_freq={:.1} vol={:.3} pan={:.3} ch_vol={} glob_vol={} def_vol={} loop={:?} start={} end={}",
                    n, channel, note_key, remapped_key, sample_idx, sample.data.len(), sample.sample_rate,
                    delta, playback_freq,
                    self.compute_channel_volume(channel),
                    self.compute_channel_panning(channel),
                    self.state.channels[channel].channel_volume,
                    self.state.global_volume,
                    sample.default_volume,
                    sample.loop_type,
                    sample.loop_start,
                    sample.loop_end,
                );
            }
        }

        let instrument_idx = self.state.channels[channel].last_instrument as usize;
        let nna = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            module.instruments[instrument_idx].nna
        } else {
            NewNoteAction::NoteCut
        };
        let dct = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            module.instruments[instrument_idx].duplicate_check_type
        } else {
            DuplicateCheckType::Disabled
        };
        let dca = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            module.instruments[instrument_idx].duplicate_check_action
        } else {
            DuplicateCheckAction::NoteCut
        };

        let fade_out_rate = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            module.instruments[instrument_idx].fade_out
        } else {
            0
        };

        self.handle_nna(channel, nna, dct, dca, instrument_idx, sample_idx);

        {
            static DBG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = DBG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 200 {
                let active = self.voices.iter().filter(|v| v.active).count();
                let ch_active = self.voices.iter().filter(|v| v.active && v.channel == Some(channel)).count();
                eprintln!("[NOTE] n={} ch={} note={:?} inst={} sample={} active={} ch_active={} nna={:?}",
                    n, channel, Note::On(note_key), instrument_idx, sample_idx, active, ch_active, nna);
                for (i, v) in self.voices.iter().enumerate() {
                    if v.active && v.channel == Some(channel) {
                        eprintln!("  v{} ch={:?} pos={:.1} vol={:.4} loop={:?} inst={:?} samp={:?}",
                            i, v.channel, v.position, v.final_volume, v.loop_type, v.instrument_index, v.sample_index);
                    }
                }
            }
        }

        let freq = match Note::On(remapped_key).frequency() {
            Some(f) => f,
            None => return,
        };

        let fine_tune_offset = if channel < self.state.channels.len() {
            self.state.channels[channel].fine_tune_offset
        } else {
            0
        };
        let playback_freq = compute_playback_frequency(
            freq,
            sample.sample_rate,
            sample.relative_note,
            sample.fine_tune.saturating_add(fine_tune_offset),
        );

        let mut vol = self.compute_channel_volume(channel);
        let mut pan = self.compute_channel_panning(channel);

        // Apply instrument random volume/panning and pitch-pan separation
        if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            let inst = &module.instruments[instrument_idx];
            if inst.random_volume > 0 {
                let r = fastrand();
                vol *= 1.0 - (inst.random_volume as f32 / 100.0) * r;
            }
            if inst.random_panning > 0 {
                let r = fastrand();
                pan += (r - 0.5) * 2.0 * (inst.random_panning as f32 / 100.0);
                pan = pan.clamp(0.0, 1.0);
            }
            if inst.pitch_pan_separation != 0 {
                let center = inst.pitch_pan_center as i16;
                let note_i16 = note_key as i16;
                let sep = inst.pitch_pan_separation as f32 / 96.0;
                pan += (note_i16 - center) as f32 * sep;
                pan = pan.clamp(0.0, 1.0);
            }
        }

        #[cfg(feature = "audio_debug")]
        eprintln!(
            "[TRIGGER] ch={} note_freq={:.2} pb_freq={:.2} remapped={}->{} sample_sr={} sample_len={} vol={:.4} pan={:.4} rel_note={} fine_tune={}",
            channel, freq, playback_freq, note_key, remapped_key, sample.sample_rate, sample.data.len(), vol, pan, sample.relative_note, sample.fine_tune
        );

        let sample_offset = if let Effect::SetSampleOffset { offset } = cell.effect {
            let off = offset as usize;
            if sample.loop_start < sample.loop_end && off >= sample.loop_end {
                sample.loop_start + (off - sample.loop_start) % (sample.loop_end - sample.loop_start)
            } else {
                off.min(sample.data.len().saturating_sub(1))
            }
        } else {
            0
        };

        let voice_idx = self.allocate_voice(channel);
        self.voices[voice_idx].trigger(
            sample.data.clone(),
            sample.sample_rate as f64,
            sample.loop_type,
            sample.loop_start,
            sample.loop_end,
            playback_freq,
            self.output_sample_rate,
            vol,
            pan,
            sample_offset,
            Some(instrument_idx as u8),
            Some(sample_idx as u8),
            Note::On(note_key),
            nna,
            fade_out_rate,
        );
        self.voices[voice_idx].channel = Some(channel);
        if sample.loop_type == LoopType::Backward {
            self.voices[voice_idx].direction = -1.0;
            if sample_offset == 0 {
                self.voices[voice_idx].position = (sample.data.len().max(1) - 1) as f64;
            }
        }

        if let Effect::SetSampleOffset { offset } = cell.effect {
            self.state.channels[channel].last_sample_offset = offset;
        }

        if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            let inst = &module.instruments[instrument_idx];
            let carry_vol = inst.volume_envelope.as_ref().map_or(false, |e| e.flags.carry);
            let carry_pan = inst.panning_envelope.as_ref().map_or(false, |e| e.flags.carry);
            let carry_pitch = inst.pitch_envelope.as_ref().map_or(false, |e| e.flags.carry);

            // Find previous voice on this channel for envelope carry
            let mut prev_vol_pos = None;
            let mut prev_pan_pos = None;
            let mut prev_pitch_pos = None;
            if carry_vol || carry_pan || carry_pitch {
                for v in &self.voices {
                    if v.active && v.channel == Some(channel) {
                        if carry_vol { prev_vol_pos = v.vol_env.as_ref().map(|e| e.position); }
                        if carry_pan { prev_pan_pos = v.pan_env.as_ref().map(|e| e.position); }
                        if carry_pitch { prev_pitch_pos = v.pitch_env.as_ref().map(|e| e.position); }
                        break;
                    }
                }
            }

            if let Some(ref vol_env) = inst.volume_envelope {
                if vol_env.flags.enabled {
                    let pos = if carry_vol { prev_vol_pos.unwrap_or(0.0) } else { 0.0 };
                    self.voices[voice_idx].vol_env = Some(EnvelopeState {
                        envelope: Arc::new(vol_env.clone()),
                        current_point: 0,
                        position: pos,
                        released: false,
                        finished: false,
                    });
                }
            }
            if let Some(ref pan_env) = inst.panning_envelope {
                if pan_env.flags.enabled {
                    let pos = if carry_pan { prev_pan_pos.unwrap_or(0.0) } else { 0.0 };
                    self.voices[voice_idx].pan_env = Some(EnvelopeState {
                        envelope: Arc::new(pan_env.clone()),
                        current_point: 0,
                        position: pos,
                        released: false,
                        finished: false,
                    });
                }
            }
            if let Some(ref pitch_env) = inst.pitch_envelope {
                if pitch_env.flags.enabled {
                    self.voices[voice_idx].pitch_env = Some(EnvelopeState {
                        envelope: Arc::new(pitch_env.clone()),
                        current_point: 0,
                        position: 0.0,
                        released: false,
                        finished: false,
                    });
                }
            }
            self.voices[voice_idx].fade_out_rate = inst.fade_out;
        }
    }

    fn handle_nna(&mut self, channel: usize, nna: NewNoteAction,
        dct: DuplicateCheckType, dca: DuplicateCheckAction,
        instr_idx: usize, sample_idx: usize) {
        let mut indices: Vec<usize> = Vec::new();

        // Apply duplicate check first (DCT/DCA)
        if dct != DuplicateCheckType::Disabled {
            for (i, voice) in self.voices.iter().enumerate() {
                if !voice.active || voice.channel != Some(channel) {
                    continue;
                }
                let matches = match dct {
                    DuplicateCheckType::Note => {
                        voice.note == self.state.channels[channel].last_note
                    }
                    DuplicateCheckType::Sample => voice.sample_index == Some(sample_idx as u8),
                    DuplicateCheckType::Instrument => voice.instrument_index == Some(instr_idx as u8),
                    _ => false,
                };
                if matches {
                    indices.push(i);
                }
            }
            for voice_idx in &indices {
                match dca {
                    DuplicateCheckAction::NoteCut => { self.voices[*voice_idx].deactivate(); }
                    DuplicateCheckAction::NoteOff => {
                        self.voices[*voice_idx].note_off = true;
                        if let Some(ref mut env) = self.voices[*voice_idx].vol_env { env.released = true; }
                        if let Some(ref mut env) = self.voices[*voice_idx].pan_env { env.released = true; }
                        if let Some(ref mut env) = self.voices[*voice_idx].pitch_env { env.released = true; }
                    }
                    DuplicateCheckAction::NoteFade => {
                        self.voices[*voice_idx].fading = true;
                        if let Some(ref mut env) = self.voices[*voice_idx].vol_env { env.released = true; }
                    }
                }
            }
        }

        // Standard NNA handling for remaining voices on channel
        indices.clear();
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.active && voice.channel == Some(channel) {
                indices.push(i);
            }
        }

        for voice_idx in indices {
            match nna {
                NewNoteAction::NoteCut => {
                    {
                        static DBG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                        let n = DBG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if n < 200 {
                            eprintln!("[NNA] cut voice={} ch={}", voice_idx, channel);
                        }
                    }
                    self.voices[voice_idx].deactivate();
                }
                NewNoteAction::Continue => {}
                NewNoteAction::NoteOff => {
                    self.voices[voice_idx].note_off = true;
                    if let Some(ref mut env) = self.voices[voice_idx].vol_env {
                        env.released = true;
                    }
                    if let Some(ref mut env) = self.voices[voice_idx].pan_env {
                        env.released = true;
                    }
                    if let Some(ref mut env) = self.voices[voice_idx].pitch_env {
                        env.released = true;
                    }
                }
                NewNoteAction::NoteFade => {
                    self.voices[voice_idx].fading = true;
                    if let Some(ref mut env) = self.voices[voice_idx].vol_env {
                        env.released = true;
                    }
                }
            }
        }
    }

    fn handle_note_off(&mut self, channel: usize) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.note_off = true;
                if let Some(ref mut env) = voice.vol_env {
                    env.released = true;
                }
                if let Some(ref mut env) = voice.pan_env {
                    env.released = true;
                }
                if let Some(ref mut env) = voice.pitch_env {
                    env.released = true;
                }
            }
        }
    }

    fn cut_channel_voices(&mut self, channel: usize) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.deactivate();
            }
        }
    }

    fn fade_channel_voices(&mut self, channel: usize) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.fading = true;
                if let Some(ref mut env) = voice.vol_env {
                    env.released = true;
                }
            }
        }
    }

    fn apply_volume_column(&mut self, channel: usize, vol: u8) {
        let ch_state = &mut self.state.channels[channel];

        if vol <= 64 {
            ch_state.row_volume = vol;
            ch_state.channel_volume = vol;
            return;
        }

        match vol {
            65..=74 => {
                let amount = (vol - 65) as f32 / 9.0;
                let new_vol = (ch_state.channel_volume as f32 + amount * 64.0 / 9.0).min(64.0) as u8;
                ch_state.channel_volume = new_vol;
            }
            75..=84 => {
                let amount = (vol - 75) as f32;
                let new_vol = (ch_state.channel_volume as f32 - amount * 64.0 / 9.0).max(0.0) as u8;
                ch_state.channel_volume = new_vol;
            }
            85..=94 => {
                ch_state.last_tone_portamento_speed = vol - 85;
            }
            95..=104 => {
                ch_state.last_vibrato_speed = vol - 95;
            }
            105..=124 => {
                ch_state.row_volume = vol - 118;
            }
            125..=127 => {
                ch_state.row_volume = vol;
            }
            128..=192 => {
                let pan = vol - 128;
                ch_state.channel_panning = (pan as f32 / 64.0 * 255.0).min(255.0) as u8;
            }
            193..=207 => {
                let speed = vol - 193;
                ch_state.last_portamento_up_speed = speed;
                self.apply_portamento_up(channel, speed);
            }
            208..=222 => {
                let speed = vol - 208;
                ch_state.last_portamento_down_speed = speed;
                self.apply_portamento_down(channel, speed);
            }
            _ => {}
        }
    }

    fn apply_effect(&mut self, channel: usize, effect: &Effect, is_row_start: bool) {
        match effect {
            Effect::None => {}

            Effect::SetSpeed { speed } => {
                if *speed > 0 {
                    self.state.speed = *speed;
                }
            }
            Effect::SetTempo { bpm } => {
                if *bpm >= 32 {
                    self.state.bpm = *bpm as u16;
                    self.state.samples_per_tick =
                        compute_samples_per_tick(self.state.bpm, self.output_sample_rate);
                }
            }

            Effect::SetVolume { volume } => {
                self.state.channels[channel].channel_volume = (*volume).min(64);
                self.state.channels[channel].row_volume = (*volume).min(64);
                let vol = self.compute_channel_volume(channel);
                for voice in &mut self.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = vol;
                        voice.channel_volume = 1.0;
                    }
                }
            }

            Effect::SetPanning { pan } => {
                self.state.channels[channel].channel_panning = (*pan).min(255);
            }

            Effect::SetPanPosition { pan } => {
                self.state.channels[channel].channel_panning = (*pan).min(255);
            }

            Effect::SetSampleOffset { offset } => {
                self.state.channels[channel].last_sample_offset = *offset;
            }

            Effect::PositionJump { order } => {
                self.state.position_jump_order = Some(*order);
            }

            Effect::PatternBreak { row } => {
                self.state.pattern_break_row = Some(*row);
            }

            Effect::SetGlobalVolume { volume } => {
                self.state.global_volume = (*volume).min(128);
                self.global_volume = self.state.global_volume as f32 / 128.0;
            }

            Effect::PatternDelay { ticks } => {
                if !self.state.row_delay_active {
                    self.state.pattern_delay_ticks = *ticks;
                    self.state.row_delay_active = true;
                }
            }

            Effect::PortamentoUp { speed } => {
                if *speed > 0 {
                    self.state.channels[channel].last_portamento_up_speed = *speed;
                }
                if is_row_start {
                    let s = self.state.channels[channel].last_portamento_up_speed;
                    self.apply_portamento_up(channel, s);
                }
            }

            Effect::PortamentoDown { speed } => {
                if *speed > 0 {
                    self.state.channels[channel].last_portamento_down_speed = *speed;
                }
                if is_row_start {
                    let s = self.state.channels[channel].last_portamento_down_speed;
                    self.apply_portamento_down(channel, s);
                }
            }

            Effect::FinePortamentoUp { speed } => {
                if *speed > 0 && is_row_start {
                    self.apply_portamento_up(channel, *speed);
                }
            }

            Effect::FinePortamentoDown { speed } => {
                if *speed > 0 && is_row_start {
                    self.apply_portamento_down(channel, *speed);
                }
            }

            Effect::TonePortamento { speed } => {
                if *speed > 0 {
                    self.state.channels[channel].last_tone_portamento_speed = *speed;
                }
            }

            Effect::VolumeSlide { up, down } => {
                if *up > 0 {
                    self.state.channels[channel].last_volume_slide_up = *up;
                }
                if *down > 0 {
                    self.state.channels[channel].last_volume_slide_down = *down;
                }
                if is_row_start {
                    self.apply_volume_slide(channel);
                }
            }

            Effect::FineVolumeSlideUp { amount } => {
                let vol = &mut self.state.channels[channel].channel_volume;
                *vol = (*vol as u16 + *amount as u16).min(64) as u8;
                let v = *vol as f32 / 64.0 * self.state.global_volume as f32 / 128.0;
                for voice in &mut self.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = v;
                        voice.channel_volume = 1.0;
                    }
                }
            }

            Effect::FineVolumeSlideDown { amount } => {
                let vol = &mut self.state.channels[channel].channel_volume;
                *vol = vol.saturating_sub(*amount);
                let v = *vol as f32 / 64.0 * self.state.global_volume as f32 / 128.0;
                for voice in &mut self.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = v;
                        voice.channel_volume = 1.0;
                    }
                }
            }

            Effect::Vibrato { speed, depth } => {
                if *speed > 0 {
                    self.state.channels[channel].last_vibrato_speed = *speed;
                }
                if *depth > 0 {
                    self.state.channels[channel].last_vibrato_depth = *depth;
                }
            }

            Effect::Tremolo { speed, depth } => {
                if *speed > 0 {
                    self.state.channels[channel].last_tremolo_speed = *speed;
                }
                if *depth > 0 {
                    self.state.channels[channel].last_tremolo_depth = *depth;
                }
            }

            Effect::Arpeggio { note1, note2 } => {
                self.state.channels[channel].last_arpeggio = (*note1, *note2);
            }

            Effect::TonePortamentoVolumeSlide { up } => {
                let param = *up as u8;
                let up_val = param >> 4;
                let down_val = param & 0x0F;
                if up_val > 0 {
                    self.state.channels[channel].last_volume_slide_up = up_val;
                }
                if down_val > 0 {
                    self.state.channels[channel].last_volume_slide_down = down_val;
                }
            }

            Effect::VibratoVolumeSlide { up } => {
                let param = *up as u8;
                let up_val = param >> 4;
                let down_val = param & 0x0F;
                if up_val > 0 {
                    self.state.channels[channel].last_volume_slide_up = up_val;
                }
                if down_val > 0 {
                    self.state.channels[channel].last_volume_slide_down = down_val;
                }
            }

            Effect::ExtendedEffect { param } => {
                let sub = param >> 4;
                let val = param & 0x0F;
                match sub {
                    0x1 => {
                        if is_row_start {
                            self.apply_portamento_up(channel, val << 4);
                        }
                    }
                    0x2 => {
                        if is_row_start {
                            self.apply_portamento_down(channel, val << 4);
                        }
                    }
                    0x8 => {
                        self.state.channels[channel].channel_panning = (val << 4).min(255);
                    }
                    0x9 => {
                        if val > 0 {
                            self.state.channels[channel].last_retrigger_interval = val;
                        }
                    }
                    0xA => {
                        self.state.channels[channel].channel_volume =
                            (self.state.channels[channel].channel_volume as u16 + val as u16).min(64) as u8;
                    }
                    0xB => {
                        self.state.channels[channel].channel_volume =
                            self.state.channels[channel].channel_volume.saturating_sub(val);
                    }
                    0xC => {
                        self.set_channel_cutoff_tick(channel, val);
                    }
                    0xD => {
                        self.set_channel_delay_tick(channel, val);
                    }
                    _ => {}
                }
            }

            Effect::GlissandoControl { on } => {
                self.state.channels[channel].glissando = *on;
            }

            Effect::VibratoWaveform { waveform } => {
                let w = match waveform & 0x03 {
                    0 => VibratoWaveform::Sine,
                    1 => VibratoWaveform::Square,
                    2 => VibratoWaveform::Ramp,
                    3 => VibratoWaveform::Random,
                    _ => VibratoWaveform::Sine,
                };
                for voice in &mut self.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.vibrato_waveform = w;
                    }
                }
            }

            Effect::TremoloWaveform { waveform } => {
                let w = match waveform & 0x03 {
                    0 => VibratoWaveform::Sine,
                    1 => VibratoWaveform::Square,
                    2 => VibratoWaveform::Ramp,
                    3 => VibratoWaveform::Random,
                    _ => VibratoWaveform::Sine,
                };
                for voice in &mut self.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.tremolo_waveform = w;
                    }
                }
            }

            Effect::SetFineTune { tune } => {
                if channel < self.state.channels.len() {
                    self.state.channels[channel].fine_tune_offset = *tune as i8;
                }
                for voice in &mut self.voices {
                    if voice.active && voice.channel == Some(channel) {
                        let detune = (*tune as f64 - 8.0) / 128.0;
                        voice.current_frequency = voice.base_frequency * 2.0_f64.powf(detune / 12.0);
                        voice.sample_delta = voice.current_frequency / self.output_sample_rate;
                    }
                }
            }

            Effect::PatternLoop { count } => {
                if *count == 0 {
                    if self.state.pattern_loop_count == 0 {
                        self.state.pattern_loop_start = Some((self.state.current_order, self.state.current_row));
                    }
                } else if self.state.pattern_loop_count == 0 {
                    self.state.pattern_loop_count = *count;
                }
            }

            Effect::Retrigger { interval } => {
                if *interval > 0 {
                    self.state.channels[channel].last_retrigger_interval = *interval;
                }
            }

            Effect::NoteCutAfter { ticks } => {
                if *ticks == 0 {
                    self.cut_channel_voices(channel);
                } else {
                    self.set_channel_cutoff_tick(channel, *ticks);
                }
            }

            Effect::NoteDelay { ticks } => {
                if *ticks == 0 {
                    // ProTracker: delay by 1 tick when ticks=0
                    self.set_channel_delay_tick(channel, 1);
                } else {
                    self.set_channel_delay_tick(channel, *ticks);
                }
            }

            Effect::SetEnvelopePosition { tick } => {
                self.set_envelope_position(channel, *tick);
            }

            Effect::Panbrello { speed, depth } => {
                if *speed > 0 {
                    self.state.channels[channel].last_panbrello_speed = *speed;
                }
                if *depth > 0 {
                    self.state.channels[channel].last_panbrello_depth = *depth;
                }
            }

            Effect::Tremor { ontime, offtime } => {
                self.state.channels[channel].tremor_ontime = *ontime;
                self.state.channels[channel].tremor_offtime = *offtime;
                self.state.channels[channel].tremor_counter = 0;
                self.state.channels[channel].tremor_active = true;
            }

            Effect::GlobalVolumeSlide { up, down } => {
                let new_vol = self.state.global_volume as i16
                    + *up as i16
                    - (*down).unsigned_abs() as i16;
                self.state.global_volume = new_vol.clamp(0, 128) as u8;
                self.global_volume = self.state.global_volume as f32 / 128.0;
            }

            Effect::SetPanning16 { pan } => {
                self.state.channels[channel].channel_panning = *pan;
            }

            Effect::VolSetVolume { vol } => {
                self.state.channels[channel].channel_volume = (*vol).min(64);
            }
            Effect::VolFineSlideUp { amount } => {
                self.state.channels[channel].channel_volume =
                    (self.state.channels[channel].channel_volume as u16 + *amount as u16).min(64) as u8;
            }
            Effect::VolFineSlideDown { amount } => {
                self.state.channels[channel].channel_volume =
                    self.state.channels[channel].channel_volume.saturating_sub(*amount);
            }
            Effect::VolSlideUp { amount } => {
                self.state.channels[channel].channel_volume =
                    (self.state.channels[channel].channel_volume as u16 + *amount as u16).min(64) as u8;
            }
            Effect::VolSlideDown { amount } => {
                self.state.channels[channel].channel_volume =
                    self.state.channels[channel].channel_volume.saturating_sub(*amount);
            }
            Effect::VolPortamento { speed } => {
                if *speed > 0 {
                    self.state.channels[channel].last_tone_portamento_speed = *speed;
                }
                if self.state.channels[channel].portamento_target_period.is_none() {
                    if let Note::On(key) = self.state.channels[channel].last_note {
                        let module = match self.module.as_ref() {
                            Some(m) => m.clone(),
                            None => return,
                        };
                        let inst_idx = self.state.channels[channel].last_instrument as usize;
                        let has_inst = !module.instruments.is_empty();
                        if has_inst && inst_idx > 0 && inst_idx < module.instruments.len() {
                            let sample_idx = module.instruments[inst_idx].sample_map[key as usize] as usize;
                            let rk = module.instruments[inst_idx].note_map[key as usize];
                            let sample = if sample_idx > 0 && sample_idx < module.samples.len() {
                                Some(&module.samples[sample_idx])
                            } else {
                                None
                            };
                            let target = self.compute_portamento_target(channel, key, rk, sample, sample_idx, &module);
                            self.state.channels[channel].portamento_target_period = Some(target);
                        }
                    }
                }
            }
            Effect::VolVibrato { speed } => {
                if *speed > 0 {
                    self.state.channels[channel].last_vibrato_speed = *speed;
                }
            }
        }
    }

    fn process_effects_tick(&mut self) {
        let tick = self.state.current_tick;
        let _speed = self.state.speed;

        for ch in 0..self.state.channels.len() {
            let (port_up, port_down, tp_speed, tp_has_target,
                 vol_up, vol_down, vib_speed, vib_depth,
                 trem_speed, trem_depth, arp1, arp2,
                 retrigger_interval, panbrello_speed, panbrello_depth,
                 tremor_ontime, tremor_offtime) = {
                let ch_state = &self.state.channels[ch];
                (
                    ch_state.last_portamento_up_speed,
                    ch_state.last_portamento_down_speed,
                    ch_state.last_tone_portamento_speed,
                    ch_state.portamento_target_period.is_some(),
                    ch_state.last_volume_slide_up,
                    ch_state.last_volume_slide_down,
                    ch_state.last_vibrato_speed,
                    ch_state.last_vibrato_depth,
                    ch_state.last_tremolo_speed,
                    ch_state.last_tremolo_depth,
                    ch_state.last_arpeggio.0,
                    ch_state.last_arpeggio.1,
                    ch_state.last_retrigger_interval,
                    ch_state.last_panbrello_speed,
                    ch_state.last_panbrello_depth,
                    ch_state.tremor_ontime,
                    ch_state.tremor_offtime,
                )
            };

            if port_up > 0 {
                self.apply_portamento_up(ch, port_up);
            }
            if port_down > 0 {
                self.apply_portamento_down(ch, port_down);
            }
            if tp_speed > 0 && tp_has_target {
                self.apply_tone_portamento(ch, tp_speed);
            }
            if vol_up > 0 || vol_down > 0 {
                self.apply_volume_slide(ch);
            }
            if vib_speed > 0 || vib_depth > 0 {
                self.apply_vibrato(ch, vib_speed, vib_depth);
            }
            if trem_speed > 0 || trem_depth > 0 {
                self.apply_tremolo(ch, trem_speed, trem_depth);
            }
            if arp1 > 0 || arp2 > 0 {
                self.apply_arpeggio(ch, tick, arp1, arp2);
            }
            if retrigger_interval > 0 && tick > 0 && tick % retrigger_interval == 0 {
                self.retrigger_channel_note(ch);
            }
            if panbrello_speed > 0 || panbrello_depth > 0 {
                self.apply_panbrello(ch, panbrello_speed, panbrello_depth);
            }
            if tremor_ontime > 0 || tremor_offtime > 0 {
                self.apply_tremor(ch, tick, tremor_ontime, tremor_offtime);
            }

            if let Some(cutoff) = self.get_channel_cutoff_tick(ch) {
                if tick == cutoff as u8 {
                    self.cut_channel_voices(ch);
                }
            }
            if let Some(delay) = self.get_channel_delay_tick(ch) {
                if tick == delay as u8 {
                    self.trigger_delayed_note(ch);
                }
            }
        }
    }

    fn apply_portamento_up(&mut self, channel: usize, speed: u8) {
        let slide = speed as f64;
        let factor = 2.0_f64.powf(slide / (12.0 * 64.0));
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.current_frequency *= factor;
                voice.sample_delta = voice.current_frequency / self.output_sample_rate;
            }
        }
    }

    fn apply_portamento_down(&mut self, channel: usize, speed: u8) {
        let slide = speed as f64;
        let factor = 2.0_f64.powf(slide / (12.0 * 64.0));
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.current_frequency /= factor;
                voice.sample_delta = voice.current_frequency / self.output_sample_rate;
            }
        }
    }

    fn apply_tone_portamento(&mut self, channel: usize, speed: u8) {
        let target = match self.state.channels[channel].portamento_target_period {
            Some(t) => t,
            None => return,
        };
        let slide = speed as f64 / (12.0 * 64.0);
        let glissando = self.state.channels[channel].glissando;

        for voice in &mut self.voices {
            if !voice.active || voice.channel != Some(channel) {
                continue;
            }
            let mut current = voice.current_frequency;
            if (current - target).abs() < 0.5 {
                voice.current_frequency = target;
            } else if current < target {
                current = current * 2.0_f64.powf(slide);
                if current > target { current = target; }
            } else {
                current = current / 2.0_f64.powf(slide);
                if current < target { current = target; }
            }
            if glissando {
                current = quantize_to_semitone(current);
            }
            voice.current_frequency = current;
            voice.sample_delta = voice.current_frequency / self.output_sample_rate;
        }
    }

    fn apply_volume_slide(&mut self, channel: usize) {
        let (up, down) = {
            let ch = &self.state.channels[channel];
            (ch.last_volume_slide_up, ch.last_volume_slide_down)
        };
        if up == 0 && down == 0 {
            return;
        }

        let ch = &mut self.state.channels[channel];
        if up > 0 {
            ch.channel_volume = (ch.channel_volume as u16 + up as u16).min(64) as u8;
        }
        if down > 0 {
            ch.channel_volume = ch.channel_volume.saturating_sub(down);
        }
        ch.row_volume = ch.channel_volume;
        let vol = ch.channel_volume.min(64) as f32 / 64.0 * self.state.global_volume as f32 / 128.0;
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol;
                voice.channel_volume = 1.0;
            }
        }

        let vol = ch.channel_volume as f32 / 64.0;
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.channel_volume = vol;
            }
        }
    }

    fn apply_vibrato(&mut self, channel: usize, speed: u8, depth: u8) {
        if depth == 0 {
            return;
        }
        let depth_f = depth as f32 / 64.0;

        for voice in &mut self.voices {
            if !voice.active || voice.channel != Some(channel) {
                continue;
            }
            let table_val = get_vibrato_value(voice.vibrato_waveform, voice.vibrato_phase);
            let vibrato_offset = table_val * depth_f;
            let freq_mod = 2.0_f64.powf(vibrato_offset as f64 / (12.0 * 16.0));
            voice.sample_delta = (voice.base_frequency * freq_mod) / self.output_sample_rate;
            voice.vibrato_phase = (voice.vibrato_phase + speed as f32) % VIBRATO_TABLE_SIZE as f32;
            voice.vibrato_speed = speed;
            voice.vibrato_depth = depth;
        }
    }

    fn apply_tremolo(&mut self, channel: usize, speed: u8, depth: u8) {
        if depth == 0 {
            return;
        }
        let depth_f = depth as f32 / 64.0;

        for voice in &mut self.voices {
            if !voice.active || voice.channel != Some(channel) {
                continue;
            }
            let table_val = get_vibrato_value(voice.tremolo_waveform, voice.tremolo_phase);
            voice.tremolo_volume = table_val * depth_f;
            voice.tremolo_phase = (voice.tremolo_phase + speed as f32) % VIBRATO_TABLE_SIZE as f32;
            voice.tremolo_speed = speed;
            voice.tremolo_depth = depth;
        }
    }

    fn apply_arpeggio(&mut self, channel: usize, tick: u8, note1: u8, note2: u8) {
        let arp_tick = tick % 3;
        let semitone_offset = match arp_tick {
            0 => 0,
            1 => note1,
            2 => note2,
            _ => 0,
        };
        if semitone_offset == 0 {
            return;
        }
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
let freq_mod = 2.0_f64.powf(semitone_offset as f64 / 12.0);
                voice.sample_delta = (voice.base_frequency * freq_mod) / self.output_sample_rate;
            }
        }
    }

    fn apply_panbrello(&mut self, channel: usize, speed: u8, depth: u8) {
        if depth == 0 {
            return;
        }
        let depth_f = depth as f32 / 64.0;

        for voice in &mut self.voices {
            if !voice.active || voice.channel != Some(channel) {
                continue;
            }
            let table_val = get_vibrato_value(VibratoWaveform::Sine, voice.panbrello_phase);
            let pan_offset = table_val * depth_f / 255.0;
            voice.panbrello_phase = (voice.panbrello_phase + speed as f32) % VIBRATO_TABLE_SIZE as f32;
            voice.panbrello_speed = speed;
            voice.panbrello_depth = depth;
            voice.final_panning = (voice.base_panning + pan_offset).clamp(0.0, 1.0);
        }
    }

    fn apply_tremor(&mut self, channel: usize, _tick: u8, ontime: u8, offtime: u8) {
        if ontime == 0 && offtime == 0 {
            return;
        }
        let cycle = ontime as u16 + offtime as u16;
        if cycle == 0 {
            return;
        }
        let counter = self.state.channels[channel].tremor_counter as u16;
        let phase = counter % cycle;
        let mute = phase >= ontime as u16;
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.tremor_mute = mute;
            }
        }
        self.state.channels[channel].tremor_counter = self.state.channels[channel].tremor_counter.wrapping_add(1);
    }

    fn retrigger_channel_note(&mut self, channel: usize) {
        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };

        let (note, sample_idx, instrument_idx) = {
            let ch = &self.state.channels[channel];
            (ch.last_note, ch.last_sample, ch.last_instrument)
        };

        if sample_idx == 0 || (sample_idx as usize) >= module.samples.len() {
            return;
        }
        let sample = &module.samples[sample_idx as usize];
        let _note_key = match note {
            Note::On(key) => key,
            _ => return,
        };
        let freq = match note.frequency() {
            Some(f) => f,
            None => return,
        };
        let playback_freq = compute_playback_frequency(freq, sample.sample_rate, sample.relative_note, sample.fine_tune);
        let vol = self.compute_channel_volume(channel);
        let pan = self.compute_channel_panning(channel);

        let voice_idx = self.allocate_voice(channel);
        self.voices[voice_idx].trigger(
            sample.data.clone(), sample.sample_rate as f64, sample.loop_type,
            sample.loop_start, sample.loop_end, playback_freq, self.output_sample_rate,
            vol, pan, 0, Some(instrument_idx), Some(sample_idx),
            note, NewNoteAction::NoteCut, 0,
        );
        self.voices[voice_idx].channel = Some(channel);
    }

    fn set_channel_cutoff_tick(&mut self, channel: usize, ticks: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.cutoff_tick = Some(ticks as u16);
            }
        }
    }

    fn set_channel_delay_tick(&mut self, channel: usize, ticks: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.delay_tick = Some(ticks as u16);
            }
        }
    }

    fn get_channel_cutoff_tick(&self, channel: usize) -> Option<u16> {
        for voice in &self.voices {
            if voice.active && voice.channel == Some(channel) {
                return voice.cutoff_tick;
            }
        }
        None
    }

    fn get_channel_delay_tick(&self, channel: usize) -> Option<u16> {
        for voice in &self.voices {
            if voice.active && voice.channel == Some(channel) {
                return voice.delay_tick;
            }
        }
        None
    }

    fn trigger_delayed_note(&mut self, channel: usize) {
        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };

        let (note, sample_idx, instrument_idx) = {
            let ch = &self.state.channels[channel];
            (ch.last_note, ch.last_sample, ch.last_instrument)
        };

        if sample_idx == 0 || (sample_idx as usize) >= module.samples.len() {
            return;
        }
        let sample = &module.samples[sample_idx as usize];
        let _note_key = match note {
            Note::On(key) => key,
            _ => return,
        };
        let freq = match note.frequency() {
            Some(f) => f,
            None => return,
        };
        let playback_freq = compute_playback_frequency(freq, sample.sample_rate, sample.relative_note, sample.fine_tune);
        let vol = self.compute_channel_volume(channel);
        let pan = self.compute_channel_panning(channel);

        let voice_idx = self.allocate_voice(channel);
        self.voices[voice_idx].trigger(
            sample.data.clone(), sample.sample_rate as f64, sample.loop_type,
            sample.loop_start, sample.loop_end, playback_freq, self.output_sample_rate,
            vol, pan, 0, Some(instrument_idx), Some(sample_idx),
            note, NewNoteAction::NoteCut, 0,
        );
        self.voices[voice_idx].channel = Some(channel);
    }

    fn set_envelope_position(&mut self, channel: usize, tick: u16) {
        for voice in &mut self.voices {
            if !voice.active || voice.channel != Some(channel) {
                continue;
            }
            if let Some(ref mut env) = voice.vol_env {
                env.position = tick as f32;
                env.current_point = 0;
                for (i, pt) in env.envelope.points.iter().enumerate() {
                    if pt.tick as f32 <= env.position {
                        env.current_point = i;
                    }
                }
            }
        }
    }

    fn advance_envelopes(&mut self) {
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            if let Some(ref mut env) = voice.vol_env {
                advance_single_envelope(env);
                let env_val = evaluate_envelope(env);
                voice.envelope_volume = env_val / 64.0;
            }

            if let Some(ref mut env) = voice.pan_env {
                advance_single_envelope(env);
                let env_val = evaluate_envelope(env);
                voice.envelope_panning = (env_val as f32 - 32.0) / 32.0;
            }

            if let Some(ref mut env) = voice.pitch_env {
                advance_single_envelope(env);
                let env_val = evaluate_envelope(env);
                let pitch_offset = (env_val as f64 - 32.0) / 32.0;
                let freq_mod = 2.0_f64.powf(pitch_offset / 12.0);
                voice.sample_delta = voice.current_frequency * freq_mod / self.output_sample_rate;
            }

            if voice.fading && voice.fade_out_rate > 0 {
                voice.fade_out_volume -= voice.fade_out_rate as f32 / 4096.0;
                if voice.fade_out_volume <= 0.0 {
                    voice.fade_out_volume = 0.0;
                    voice.deactivate();
                    continue;
                }
            }

            if voice.note_off {
                let env_done = voice.vol_env.as_ref().map_or(true, |e| {
                    e.finished || !e.envelope.flags.enabled
                });
                if env_done {
                    if voice.fade_out_rate == 0 {
                        voice.deactivate();
                        continue;
                    } else if !voice.fading {
                        voice.fading = true;
                    }
                }
            }

            if voice.tremor_mute {
                voice.final_volume = 0.0;
            } else {
                voice.final_volume = voice.base_volume
                    * voice.envelope_volume
                    * voice.channel_volume
                    * voice.global_volume
                    * voice.fade_out_volume;
            }

            if voice.tremolo_depth > 0 {
                voice.final_volume *= 1.0 + voice.tremolo_volume;
            }

            voice.final_panning = (voice.base_panning + voice.envelope_panning).clamp(0.0, 1.0);
        }
    }

    fn advance_row(&mut self) {
        self.state.current_tick = 0;

        for voice in &mut self.voices {
            if voice.active {
                voice.cutoff_tick = None;
                voice.delay_tick = None;
            }
        }

        if self.state.row_delay_active && self.state.pattern_delay_ticks > 0 {
            self.state.pattern_delay_ticks -= 1;
            return;
        }
        self.state.row_delay_active = false;
        self.state.pattern_delay_ticks = 0;

        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => {
                self.stop();
                return;
            }
        };

        if let Some(target_order) = self.state.position_jump_order.take() {
            if (target_order as usize) < module.order_list.len() {
                self.state.current_order = target_order as u16;
                self.state.current_pattern = self.get_pattern_for_order(target_order as u16);
                let target_row = self.state.pattern_break_row.take().unwrap_or(0);
                self.state.current_row = target_row;
                return;
            } else {
                self.stop();
                return;
            }
        }

        if let Some(target_row) = self.state.pattern_break_row.take() {
            self.state.current_order += 1;
            if (self.state.current_order as usize) >= module.order_list.len() {
                self.handle_song_end();
                return;
            }
            self.state.current_pattern = self.get_pattern_for_order(self.state.current_order);
            self.state.current_row = target_row;
            return;
        }

        let pattern_idx = self.state.current_pattern as usize;
        let pattern_rows = if pattern_idx < module.patterns.len() {
            module.patterns[pattern_idx].num_rows
        } else {
            64
        };

        if self.state.pattern_loop_count > 0 {
            if let Some((loop_order, loop_row)) = self.state.pattern_loop_start {
                self.state.current_order = loop_order;
                self.state.current_row = loop_row;
                self.state.current_pattern = self.get_pattern_for_order(loop_order);
                self.state.pattern_loop_count -= 1;
                if self.state.pattern_loop_count == 0 {
                    self.state.pattern_loop_start = None;
                }
                return;
            }
        }

        let next_row = self.state.current_row as usize + 1;
        if next_row >= pattern_rows {
            self.state.current_order += 1;
            if (self.state.current_order as usize) >= module.order_list.len() {
                self.handle_song_end();
                return;
            }
            self.state.current_pattern = self.get_pattern_for_order(self.state.current_order);
            self.state.current_row = 0;
        } else {
            self.state.current_row = next_row as u8;
        }
    }

    fn handle_song_end(&mut self) {
        match self.state.play_mode {
            PlayMode::Once | PlayMode::Order => {
                self.stop();
            }
            PlayMode::Loop => {
                self.state.current_order = 0;
                self.state.current_row = 0;
                self.state.current_pattern = self.get_pattern_for_order(0);
            }
            PlayMode::Pattern => {
                self.state.current_row = 0;
            }
        }
    }

    fn get_pattern_for_order(&self, order: u16) -> u8 {
        let module = match self.module.as_ref() {
            Some(m) => m,
            None => return 0,
        };
        let order_idx = order as usize;
        if order_idx < module.order_list.len() {
            let pat_idx = module.order_list[order_idx] as usize;
            if pat_idx < module.patterns.len() {
                return pat_idx as u8;
            }
        }
        0
    }

    fn get_order_count(&self) -> usize {
        self.module.as_ref().map_or(0, |m| m.order_list.len())
    }

    fn allocate_voice(&mut self, channel: usize) -> usize {
        let start = self.next_voice;

        // First pass: prefer inactive voice on same channel
        for i in 0..MAX_VOICES {
            let idx = (start + i) % MAX_VOICES;
            let voice = &self.voices[idx];
            if !voice.active && voice.channel == Some(channel) {
                self.next_voice = (idx + 1) % MAX_VOICES;
                return idx;
            }
        }

        // Second pass: any inactive voice
        for i in 0..MAX_VOICES {
            let idx = (start + i) % MAX_VOICES;
            if !self.voices[idx].active {
                self.next_voice = (idx + 1) % MAX_VOICES;
                return idx;
            }
        }

        let mut best_fading = None;
        let mut best_same_channel = None;
        let mut best_any = None;

        for i in 0..MAX_VOICES {
            let idx = (start + i) % MAX_VOICES;
            let voice = &self.voices[idx];

            if best_any.is_none() {
                best_any = Some(idx);
            }

            if voice.channel == Some(channel) && best_same_channel.is_none() {
                best_same_channel = Some(idx);
            }

            if voice.fading && voice.channel == Some(channel) && best_fading.is_none() {
                best_fading = Some(idx);
                break;
            }
        }

        let chosen = best_fading
            .or(best_same_channel)
            .or(best_any)
            .unwrap_or(start);

        self.next_voice = (chosen + 1) % MAX_VOICES;
        chosen
    }

    fn compute_channel_volume(&self, channel: usize) -> f32 {
        if channel >= self.state.channels.len() {
            return 0.0;
        }
        let ch = &self.state.channels[channel];
        let vol = ch.channel_volume.min(64) as f32 / 64.0;
        let global = self.state.global_volume as f32 / 128.0;
        vol * global
    }

    fn compute_channel_panning(&self, channel: usize) -> f32 {
        if channel >= self.state.channels.len() {
            return 0.5;
        }
        self.state.channels[channel].channel_panning as f32 / 255.0
    }

    fn compute_portamento_target(
        &self,
        channel: usize,
        _note_key: u8,
        remapped_key: u8,
        sample: Option<&Sample>,
        sample_idx: usize,
        module: &Module,
    ) -> f64 {
        let freq = match Note::On(remapped_key).frequency() {
            Some(f) => f,
            None => return 0.0,
        };

        let s = if sample_idx > 0 && sample_idx < module.samples.len() {
            &module.samples[sample_idx]
        } else {
            match sample {
                Some(s) => s,
                None => return freq,
            }
        };

        compute_playback_frequency(freq, s.sample_rate, s.relative_note, s.fine_tune)
    }
}

fn compute_samples_per_tick(bpm: u16, sample_rate: f64) -> f64 {
    if bpm == 0 {
        return 0.0;
    }
    sample_rate * 5.0 / (bpm as f64 * 2.0)
}

fn get_vibrato_value(waveform: VibratoWaveform, phase: f32) -> f32 {
    let idx = (phase as usize) % VIBRATO_TABLE_SIZE;
    match waveform {
        VibratoWaveform::Sine => VIBRATO_SINE_TABLE[idx],
        VibratoWaveform::Square => {
            if idx < VIBRATO_TABLE_SIZE / 2 {
                255.0
            } else {
                -255.0
            }
        }
        VibratoWaveform::Ramp => VIBRATO_RAMP_TABLE[idx],
        VibratoWaveform::Random => {
            let val = (idx as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((val >> 33) as i32 - 256) as f32
        }
    }
}

fn advance_single_envelope(env: &mut EnvelopeState) {
    if env.finished {
        return;
    }

    let points = &env.envelope.points;
    if points.is_empty() {
        env.finished = true;
        return;
    }

    env.position += 1.0;

    if env.current_point + 1 < points.len() {
        let next_point = points[env.current_point + 1];
        if env.position >= next_point.tick as f32 {
            env.current_point += 1;
        }
    }

    if !env.released && env.envelope.flags.sustain {
        if let Some(sustain_idx) = env.envelope.sustain_point {
            if env.current_point >= sustain_idx && sustain_idx < points.len() {
                let sustain_tick = points[sustain_idx].tick as f32;
                if env.position >= sustain_tick {
                    env.position = sustain_tick;
                    return;
                }
            }
        }
    }

    if env.envelope.flags.loop_ {
        if let (Some(loop_start), Some(loop_end)) = (env.envelope.loop_start, env.envelope.loop_end) {
            if loop_start < loop_end && loop_end < points.len() {
                let loop_end_tick = points[loop_end].tick as f32;
                if env.position >= loop_end_tick {
                    env.position = points[loop_start].tick as f32;
                    env.current_point = loop_start;
                }
            }
        }
    }

    if !env.envelope.flags.loop_ && env.current_point >= points.len() - 1 {
        let last_tick = points.last().map(|p| p.tick).unwrap_or(0) as f32;
        if env.position >= last_tick {
            env.finished = true;
        }
    }
}

fn evaluate_envelope(env: &EnvelopeState) -> f32 {
    let points = &env.envelope.points;
    if points.is_empty() {
        return 64.0;
    }

    let current_idx = env.current_point.min(points.len() - 1);
    let current_point = points[current_idx];

    if current_idx + 1 >= points.len() {
        return current_point.value as f32;
    }

    let next_point = points[current_idx + 1];
    if next_point.tick <= current_point.tick {
        return current_point.value as f32;
    }
    let tick_range = (next_point.tick - current_point.tick) as f32;

    let t = ((env.position - current_point.tick as f32) / tick_range).clamp(0.0, 1.0);
    let val_range = next_point.value as f32 - current_point.value as f32;
    current_point.value as f32 + val_range * t
}

fn compute_playback_frequency(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_samples_per_tick_default() {
        let spt = compute_samples_per_tick(125, 48000.0);
        assert!((spt - 960.0).abs() < 1.0);
    }

    #[test]
    fn compute_samples_per_tick_140_bpm() {
        let spt = compute_samples_per_tick(140, 48000.0);
        let expected = 48000.0 * 5.0 / (140.0 * 2.0);
        assert!((spt - expected).abs() < 1.0);
    }

    #[test]
    fn sequencer_engine_new() {
        let engine = SequencerEngine::new(48000.0);
        assert_eq!(engine.voices.len(), MAX_VOICES);
        assert!(!engine.state.playing);
    }

    #[test]
    fn advance_envelope_linear() {
        let env = Envelope {
            points: vec![
                EnvelopePoint { tick: 0, value: 0 },
                EnvelopePoint { tick: 10, value: 64 },
            ],
            sustain_point: None,
            loop_start: None,
            loop_end: None,
            flags: EnvelopeFlags {
                enabled: true,
                sustain: false,
                loop_: false,
                carry: false,
            },
        };

        let mut state = EnvelopeState {
            envelope: Arc::new(env),
            current_point: 0,
            position: 0.0,
            released: false,
            finished: false,
        };

        assert!((evaluate_envelope(&state) - 0.0).abs() < 0.1);

        for _ in 0..5 {
            advance_single_envelope(&mut state);
        }
        assert!((evaluate_envelope(&state) - 32.0).abs() < 1.0);

        for _ in 0..5 {
            advance_single_envelope(&mut state);
        }
        assert!((evaluate_envelope(&state) - 64.0).abs() < 1.0);
    }

    #[test]
    fn advance_envelope_sustain() {
        let env = Envelope {
            points: vec![
                EnvelopePoint { tick: 0, value: 0 },
                EnvelopePoint { tick: 5, value: 64 },
                EnvelopePoint { tick: 10, value: 0 },
            ],
            sustain_point: Some(1),
            loop_start: None,
            loop_end: None,
            flags: EnvelopeFlags {
                enabled: true,
                sustain: true,
                loop_: false,
                carry: false,
            },
        };

        let mut state = EnvelopeState {
            envelope: Arc::new(env),
            current_point: 0,
            position: 0.0,
            released: false,
            finished: false,
        };

        for _ in 0..20 {
            advance_single_envelope(&mut state);
        }
        assert!(!state.finished);

        state.released = true;
        for _ in 0..10 {
            advance_single_envelope(&mut state);
        }
        assert!(state.finished);
    }

    #[test]
    fn vibrato_sine_table_range() {
        for &val in &VIBRATO_SINE_TABLE {
            assert!(val >= -255.0 && val <= 255.0);
        }
    }

    #[test]
    fn compute_playback_frequency_basic() {
        let freq = compute_playback_frequency(440.0, 8363, 0, 0);
        assert!(freq > 0.0);
    }

    #[test]
    fn compute_playback_frequency_c5_produces_c5speed() {
        let c5_freq = 440.0 * 2.0_f64.powf((60.0 - 69.0) / 12.0);
        let freq = compute_playback_frequency(c5_freq, 8363, 0, 0);
        assert!((freq - 8363.0).abs() < 1.0,
            "C-5 note at c5speed=8363 should produce playback freq ~8363, got {:.1}", freq);
    }

    #[test]
    fn allocate_voice_finds_inactive() {
        let mut engine = SequencerEngine::new(48000.0);
        let idx = engine.allocate_voice(0);
        assert!(!engine.voices[idx].active);
    }

    #[test]
    fn mod_playback_produces_audio() {
        use crate::formats::modfile::ModHandler;
        use crate::formats::FormatHandler;

        let sample_data: &[u8] = &[0x00, 0x40, 0x7F, 0x40, 0x00, 0xC0, 0x7F, 0xC0];
        let sample_len_words = (sample_data.len() / 2) as u16;
        let pattern_size = 64 * 4 * 4;
        let total_size = 1084 + pattern_size + sample_data.len();
        let mut data = vec![0u8; total_size];

        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"M.K.");

        let s0_base = 20;
        data[s0_base + 22] = (sample_len_words >> 8) as u8;
        data[s0_base + 23] = (sample_len_words & 0xFF) as u8;
        data[s0_base + 25] = 64;

        data[1084 + pattern_size..1084 + pattern_size + sample_data.len()].copy_from_slice(sample_data);

        let period_c3: u16 = 428;
        data[1084] = ((period_c3 >> 8) & 0x0F) as u8;
        data[1085] = (period_c3 & 0xFF) as u8;
        data[1086] = 0x10;
        data[1087] = 0x00;

        let handler = ModHandler;
        let module = Arc::new(handler.load(&data).unwrap());

        let cell = module.patterns[0].cell(0, 0);
        assert!(cell.instrument.is_some(), "MOD cell should have instrument");
        assert!(matches!(cell.note, Note::On(_)), "MOD cell should have note, got {:?}", cell.note);

        if let Note::On(key) = cell.note {
            assert!(key >= 36 && key <= 119, "Note key should be in valid MIDI range, got {}", key);
        }

        assert!(!module.samples[1].data.is_empty(), "Sample 1 should have data");
        assert_eq!(module.samples[1].sample_rate, 8363);

        let mut engine = SequencerEngine::new(48000.0);
        engine.load_module(module.clone());
        engine.play();

        assert!(engine.state.playing, "Engine should be playing after play()");

        let active_after_play = engine.voices.iter().filter(|v| v.active).count();
        assert!(active_after_play > 0, "Should have at least 1 active voice after play, got {}", active_after_play);

        let voice = engine.voices.iter().find(|v| v.active).unwrap();
        assert!(voice.sample.is_some(), "Active voice should have sample data");
        let sample_ref = voice.sample.as_ref().unwrap();
        assert!(!sample_ref.is_empty(), "Sample data should not be empty");
        assert!(voice.sample_delta > 0.0, "Sample delta should be positive, got {}", voice.sample_delta);
        assert!(voice.final_volume > 0.0, "Final volume should be positive, got {}", voice.final_volume);

        engine.advance(4800);

        let mut left = vec![0.0f32; 4800];
        let mut right = vec![0.0f32; 4800];
        crate::audio::mixer::mix_voices(
            &mut engine.voices,
            &mut left,
            &mut right,
            1.0,
            crate::audio::commands::InterpolationType::Linear,
            &[],
        );

        let max_sample = left.iter().chain(right.iter()).map(|&s| s.abs()).fold(0.0f32, f32::max);
        assert!(max_sample > 0.0001, "MOD playback should produce audio output, max sample = {:.6}", max_sample);
    }

    #[test]
    fn mod_looping_sample_sustains_audio() {
        use crate::formats::modfile::ModHandler;
        use crate::formats::FormatHandler;

        let sample_data: Vec<u8> = vec![64, 32, 16, 224, 192, 240, 100, 50, 156, 206, 0, 0];
        let sample_len_words = (sample_data.len() / 2) as u16;
        let pattern_size = 64 * 4 * 4;
        let total_size = 1084 + pattern_size + sample_data.len();
        let mut data = vec![0u8; total_size];

        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"M.K.");

        let s0_base = 20;
        data[s0_base + 22] = (sample_len_words >> 8) as u8;
        data[s0_base + 23] = (sample_len_words & 0xFF) as u8;
        data[s0_base + 25] = 64;
        let loop_start_words: u16 = 2;
        let loop_length_words: u16 = 2;
        data[s0_base + 26] = (loop_start_words >> 8) as u8;
        data[s0_base + 27] = (loop_start_words & 0xFF) as u8;
        data[s0_base + 28] = (loop_length_words >> 8) as u8;
        data[s0_base + 29] = (loop_length_words & 0xFF) as u8;

        data[1084 + pattern_size..1084 + pattern_size + sample_data.len()]
            .copy_from_slice(&sample_data);

        let period_c3: u16 = 428;
        data[1084] = ((period_c3 >> 8) & 0x0F) as u8;
        data[1085] = (period_c3 & 0xFF) as u8;
        data[1086] = 0x10;
        data[1087] = 0x00;

        let handler = ModHandler;
        let module = Arc::new(handler.load(&data).unwrap());

        assert_eq!(module.samples[1].loop_type, crate::sequencer::LoopType::Forward,
            "MOD sample should have forward loop");
        assert_eq!(module.samples[1].loop_start, 4);
        assert_eq!(module.samples[1].loop_end, 8);

        let mut engine = SequencerEngine::new(48000.0);
        engine.load_module(module.clone());
        engine.play();

        engine.advance(2000);

        let mut left = vec![0.0f32; 512];
        let mut right = vec![0.0f32; 512];
        crate::audio::mixer::mix_voices(
            &mut engine.voices,
            &mut left,
            &mut right,
            1.0,
            crate::audio::commands::InterpolationType::Linear,
            &[],
        );

        let active_voices = engine.voices.iter().filter(|v| v.active).count();
        assert!(active_voices > 0, "Voice should still be active after passing sample end");

        let voice = engine.voices.iter().find(|v| v.active).unwrap();
        assert!(voice.position >= 4.0 && voice.position < 8.0,
            "Position {:.3} should be within loop range [4, 8) after mixing", voice.position);

        let max_sample = left.iter().chain(right.iter()).map(|&s| s.abs()).fold(0.0f32, f32::max);
        assert!(max_sample > 0.0001,
            "Looping MOD sample should produce sustained audio, max sample = {:.6}", max_sample);
    }

    #[test]
    fn note_off_stops_looping_sample() {
        use crate::formats::modfile::ModHandler;
        use crate::formats::FormatHandler;

        let sample_data: Vec<u8> = vec![64, 32, 16, 224, 192, 240, 100, 50, 156, 206, 0, 0];
        let sample_len_words = (sample_data.len() / 2) as u16;
        let pattern_size = 64 * 4 * 4;
        let total_size = 1084 + pattern_size + sample_data.len();
        let mut data = vec![0u8; total_size];

        data[950] = 1;
        data[952] = 0;
        data[1080..1084].copy_from_slice(b"M.K.");

        let s0_base = 20;
        data[s0_base + 22] = (sample_len_words >> 8) as u8;
        data[s0_base + 23] = (sample_len_words & 0xFF) as u8;
        data[s0_base + 25] = 64;
        data[s0_base + 26] = 0;
        data[s0_base + 27] = 2;
        data[s0_base + 28] = 0;
        data[s0_base + 29] = 2;

        data[1084 + pattern_size..1084 + pattern_size + sample_data.len()]
            .copy_from_slice(&sample_data);

        let period_c3: u16 = 428;
        data[1084] = ((period_c3 >> 8) & 0x0F) as u8;
        data[1085] = (period_c3 & 0xFF) as u8;
        data[1086] = 0x10;
        data[1087] = 0x00;

        let handler = ModHandler;
        let module = Arc::new(handler.load(&data).unwrap());

        let mut engine = SequencerEngine::new(48000.0);
        engine.load_module(module.clone());
        engine.play();

        let mut left = vec![0.0f32; 256];
        let mut right = vec![0.0f32; 256];
        crate::audio::mixer::mix_voices(
            &mut engine.voices,
            &mut left,
            &mut right,
            1.0,
            crate::audio::commands::InterpolationType::Linear,
            &[],
        );

        let active_voice = engine.voices.iter_mut().find(|v| v.active).unwrap();
        assert!(active_voice.active, "Voice should be active before note-off");

        active_voice.note_off = true;

        engine.advance(2000);

        let still_active = engine.voices.iter().any(|v| v.active);
        assert!(!still_active, "Voice should be deactivated after note-off with fade_out_rate=0");
    }
}
