use std::sync::Arc;
use crate::sequencer::module::{Module, MAX_VOICES};
use crate::sequencer::player::SequencerState;
use crate::sequencer::instrument::{NewNoteAction, DuplicateCheckType, DuplicateCheckAction};
use crate::sequencer::period::period_to_frequency;
use crate::audio::voice::Voice;

pub struct VoicePool {
    pub voices: Vec<Voice>,
    pub next_voice: usize,
}

impl VoicePool {
    pub fn new() -> Self {
        VoicePool {
            voices: vec![Voice::default(); MAX_VOICES],
            next_voice: 0,
        }
    }

    pub fn voice(&self, index: usize) -> &Voice {
        &self.voices[index]
    }

    pub fn voice_mut(&mut self, index: usize) -> &mut Voice {
        &mut self.voices[index]
    }

    pub fn num_active(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    pub fn allocate_voice(&mut self, channel: usize) -> usize {
        let start = self.next_voice;

        for i in 0..MAX_VOICES {
            let idx = (start + i) % MAX_VOICES;
            let voice = &self.voices[idx];
            if !voice.active && voice.channel == Some(channel) {
                self.next_voice = (idx + 1) % MAX_VOICES;
                return idx;
            }
        }

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

    pub fn advance_envelopes(&mut self, is_xm: bool, state: &SequencerState, output_sample_rate: f64, module: Option<&Arc<Module>>) {
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            if let Some(ref mut env) = voice.vol_env {
                crate::audio::effects::advance_single_envelope(env);
                let env_val = crate::audio::effects::evaluate_envelope(env);
                voice.envelope_volume = env_val / 64.0;
            }

            if let Some(ref mut env) = voice.pan_env {
                crate::audio::effects::advance_single_envelope(env);
                let env_val = crate::audio::effects::evaluate_envelope(env);
                voice.envelope_panning = (env_val as f32 - 32.0) / 32.0;
            }

            if let Some(ref mut env) = voice.pitch_env {
                crate::audio::effects::advance_single_envelope(env);
                let env_val = crate::audio::effects::evaluate_envelope(env);
                let pitch_offset = (env_val as f64 - 32.0) / 32.0;
                let freq_mod = 2.0_f64.powf(pitch_offset / 12.0);
                voice.sample_delta = voice.current_frequency * freq_mod / output_sample_rate;
            }

            if let Some(ref mut env) = voice.filter_env {
                crate::audio::effects::advance_single_envelope(env);
                let env_val = crate::audio::effects::evaluate_envelope(env);
                voice.envelope_filter_cutoff = env_val / 64.0;
            }

            if let Some(ch_idx) = voice.channel {
                if let Some(ch) = state.channels.get(ch_idx) {
                    voice.filter_cutoff *= ch.auto_filter_cutoff;
                    voice.filter_resonance = (voice.filter_resonance + ch.auto_filter_resonance).clamp(0.0, 1.0);
                }
            }

            // XM volume/panning calculation (FT2-compatible)
            if is_xm {
                if !voice.env_sustain_active {
                    if voice.fade_out_speed_i32 > voice.fade_out_amp {
                        voice.fade_out_amp = 0;
                        voice.fade_out_speed_i32 = 0;
                    } else {
                        voice.fade_out_amp -= voice.fade_out_speed_i32;
                    }
                }

                let out_vol = if let Some(ch_idx) = voice.channel {
                    if ch_idx < state.channels.len() {
                        state.channels[ch_idx].channel_volume.min(64) as u32
                    } else {
                        0
                    }
                } else {
                    0
                };
                let fade_amp = voice.fade_out_amp as u32;
                let glob_vol = state.global_volume.max(1) as u32;

                let has_vol_env = voice.vol_env.as_ref().map_or(false, |e| {
                    e.envelope.flags.enabled
                });

                let vol = if has_vol_env {
                    let env_val = (voice.envelope_volume * 64.0).round() as u32;
                    (((env_val as u64) * (out_vol as u64) * (fade_amp as u64)) >> 18) as u32
                } else {
                    (((out_vol as u64) * 16 * (fade_amp as u64)) >> 16) as u32
                };
                let vol = (((vol as u64) * (glob_vol as u64)) >> 7) as u32;
                voice.final_volume = (vol as f32 / 256.0).clamp(0.0, 1.0);

                if let Some(ch_idx) = voice.channel {
                    if ch_idx < state.channels.len() {
                        voice.final_volume *= state.channels[ch_idx].auto_volume_factor;
                    }
                }

                let out_pan = if let Some(ch_idx) = voice.channel {
                    if ch_idx < state.channels.len() {
                        state.channels[ch_idx].old_pan
                    } else {
                        128
                    }
                } else {
                    128
                };
                voice.final_panning = out_pan as f32 / 255.0;

                if let Some(ref pan_env_ref) = voice.pan_env {
                    if pan_env_ref.envelope.flags.enabled {
                        let env_pan_val = (crate::audio::effects::evaluate_envelope(pan_env_ref) as i32 - 32) * 256;
                        let pan_tmp = (out_pan as i32 - 128).abs() + 128;
                        let pan_tmp_scaled = pan_tmp * 8;
                        let pan_add = (env_pan_val * pan_tmp_scaled) >> 16;
                        let final_pan = (out_pan as i32 + pan_add).clamp(0, 255);
                        voice.final_panning = final_pan as f32 / 255.0;
                    }
                }
                if let Some(ch_idx) = voice.channel {
                    if ch_idx < state.channels.len() {
                        voice.final_panning = (voice.final_panning + state.channels[ch_idx].auto_pan_offset).clamp(0.0, 1.0);
                    }
                }
            } else {
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
                        * voice.fade_out_volume;
                }

                if voice.tremolo_depth > 0 {
                    voice.final_volume *= 1.0 + voice.tremolo_volume;
                }
                if let Some(ch_idx) = voice.channel {
                    if ch_idx < state.channels.len() {
                        voice.final_volume *= state.channels[ch_idx].auto_volume_factor;
                    }
                }
                let mut pan = voice.base_panning + voice.envelope_panning;
                if let Some(ch_idx) = voice.channel {
                    if ch_idx < state.channels.len() {
                        pan += state.channels[ch_idx].auto_pan_offset;
                    }
                }
                voice.final_panning = pan.clamp(0.0, 1.0);
            }

            voice.final_volume *= state.auto_global_vol_factor;

            // Auto-vibrato for XM
            if is_xm {
                if let Some(module) = module {
                    let linear_flag = module.flags.linear_slides;
                    let inst_idx = voice.instrument_index.unwrap_or(0) as usize;
                    if inst_idx > 0 && inst_idx < module.instruments.len() {
                        let inst = &module.instruments[inst_idx];
                        if inst.vib_depth > 0 {
                            if voice.auto_vib_sweep > 0 {
                                let mut amp = voice.auto_vib_sweep;
                                if voice.env_sustain_active {
                                    amp += voice.auto_vib_amp;
                                    if (amp >> 8) > inst.vib_depth as i32 {
                                        amp = (inst.vib_depth as i32) << 8;
                                        voice.auto_vib_sweep = 0;
                                    }
                                    voice.auto_vib_amp = amp;
                                }
                            }

                            voice.auto_vib_pos = voice.auto_vib_pos.wrapping_add(inst.vib_rate);

                            let sine_tab = crate::sequencer::period::get_vib_sine();
                            let auto_vib_val: i32 = match inst.vib_type {
                                1 => {
                                    if voice.auto_vib_pos > 127 { 64 } else { -64 }
                                }
                                2 => {
                                    (((voice.auto_vib_pos as i32 >> 1) + 64) & 127) - 64
                                }
                                3 => {
                                    ((-(voice.auto_vib_pos as i32 >> 1) + 64) & 127) - 64
                                }
                                _ => {
                                    sine_tab[voice.auto_vib_pos as usize] as i32
                                }
                            };

                            let val = auto_vib_val * 4;
                            let period_offset = (val * voice.auto_vib_amp) >> 16;
                            let mut tmp_period = voice.auto_vib_period_base as i32 + period_offset;
                            if tmp_period >= 32000 {
                                tmp_period = 0;
                            }
                            if tmp_period < 1 {
                                tmp_period = 1;
                            }

                            let freq = period_to_frequency(tmp_period as u16, linear_flag, 8363);
                            voice.sample_delta = freq / output_sample_rate;
                        }
                    }
                }
            }

            // XM: handle note_off with fade_out check
            if is_xm && voice.note_off {
                let has_vol_env = voice.vol_env.as_ref().map_or(false, |e| {
                    !e.finished
                });
                if !has_vol_env && voice.fade_out_rate == 0 {
                    voice.deactivate();
                    continue;
                }
                if voice.fade_out_amp == 0 {
                    voice.deactivate();
                    continue;
                }
            }
        }
    }

    pub fn handle_nna(
        &mut self, channel: usize, nna: NewNoteAction,
        dct: DuplicateCheckType, dca: DuplicateCheckAction,
        instr_idx: usize, sample_idx: usize, state: &SequencerState,
    ) {
        let mut indices: Vec<usize> = Vec::new();

        if dct != DuplicateCheckType::Disabled {
            for (i, voice) in self.voices.iter().enumerate() {
                if !voice.active || voice.channel != Some(channel) {
                    continue;
                }
                let matches = match dct {
                    DuplicateCheckType::Note => {
                        voice.note == state.channels[channel].last_note
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
                        if let Some(ref mut env) = self.voices[*voice_idx].filter_env { env.released = true; }
                    }
                    DuplicateCheckAction::NoteFade => {
                        self.voices[*voice_idx].fading = true;
                        if let Some(ref mut env) = self.voices[*voice_idx].vol_env { env.released = true; }
                        if let Some(ref mut env) = self.voices[*voice_idx].filter_env { env.released = true; }
                    }
                }
            }
        }

        indices.clear();
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.active && voice.channel == Some(channel) {
                indices.push(i);
            }
        }

        for voice_idx in indices {
            match nna {
                NewNoteAction::NoteCut => {
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
                    if let Some(ref mut env) = self.voices[voice_idx].filter_env {
                        env.released = true;
                    }
                }
                NewNoteAction::NoteFade => {
                    self.voices[voice_idx].fading = true;
                    if let Some(ref mut env) = self.voices[voice_idx].vol_env {
                        env.released = true;
                    }
                    if let Some(ref mut env) = self.voices[voice_idx].filter_env {
                        env.released = true;
                    }
                }
            }
        }
    }

    pub fn cut_channel_voices(&mut self, channel: usize) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.deactivate();
            }
        }
    }

    pub fn fade_channel_voices(&mut self, channel: usize) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.fading = true;
                if let Some(ref mut env) = voice.vol_env {
                    env.released = true;
                }
            }
        }
    }

    pub fn update_voices_from_period(&mut self, channel: usize, freq: f64, sample_delta: f64) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.current_frequency = freq;
                voice.sample_delta = sample_delta;
            }
        }
    }

    pub fn set_envelope_position(&mut self, channel: usize, tick: u16) {
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

    pub fn advance_row_voice_reset(&mut self, is_xm: bool, state: &SequencerState) {
        for voice in &mut self.voices {
            if voice.active {
                voice.cutoff_tick = None;
                voice.delay_tick = None;
                if is_xm {
                    if let Some(ch_idx) = voice.channel {
                        if ch_idx < state.channels.len() {
                            voice.auto_vib_period_base = state.channels[ch_idx].out_period;
                        }
                    }
                }
            }
        }
    }

    pub fn find_channel_voice_mut(&mut self, channel: usize) -> Option<&mut Voice> {
        self.voices.iter_mut().find(|v| v.active && v.channel == Some(channel))
    }
}
