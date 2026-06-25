use std::sync::Arc;

use crate::audio::effects::{
    VIBRATO_TABLE_SIZE, get_vibrato_value, compute_playback_frequency, quantize_to_semitone,
};
use crate::audio::voice::EnvelopeState;
use crate::audio::filter::StateVariableFilter;
use super::helpers::{
    compute_channel_volume, compute_channel_panning, compute_portamento_target,
};
use crate::audio::sequencer_engine::SequencerEngine;
use crate::sequencer::instrument::{DuplicateCheckAction, DuplicateCheckType, NewNoteAction};
use crate::sequencer::module::Module;
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::{Sample, VibratoWaveform};
use crate::sequencer::period::{
    get_arp_tab, get_note_period, get_vib_tab, period_to_frequency, relocate_ton,
};

impl SequencerEngine {
    // ─── Tone portamento ───────────────────────────────────────

    pub(crate) fn apply_tone_portamento_period(&mut self, channel: usize, linear: bool) {
        let module = match self.module.as_ref() {
            Some(m) => m,
            None => return,
        };
        let ch = &self.state.channels[channel];
        if ch.porta_dir == 0 { return; }
        let speed = ch.porta_speed_period;
        if speed == 0 { return; }
        let want = ch.want_period;

        let ch = &mut self.state.channels[channel];
        if ch.porta_dir == 2 {
            ch.real_period = ch.real_period.saturating_sub(speed);
            if ch.real_period <= want {
                ch.real_period = want;
                ch.porta_dir = 1;
            }
        } else {
            ch.real_period = ch.real_period.saturating_add(speed);
            if ch.real_period >= want {
                ch.real_period = want;
                ch.porta_dir = 1;
            }
        }

        if ch.glissando {
            ch.out_period = relocate_ton(ch.real_period, 0, ch.fine_tune_offset, linear);
        } else {
            ch.out_period = ch.real_period;
        }

        let linear_slides = module.flags.linear_slides;
        self.update_voices_from_period(channel, linear_slides);
    }

    // ─── Vibrato ──────────────────────────────────────────────

    pub(crate) fn apply_vibrato_period(&mut self, channel: usize, _linear: bool) {
        let module = match self.module.as_ref() {
            Some(m) => m,
            None => return,
        };
        let (vib_pos, vib_speed, vib_depth, wave_ctrl) = {
            let ch = &self.state.channels[channel];
            (ch.vib_pos, ch.vib_speed, ch.vib_depth, ch.wave_ctrl & 0x03)
        };
        if vib_depth == 0 { return; }

        let vib_tab = get_vib_tab();
        let tmp_vib = ((vib_pos >> 2) & 0x1F) as usize;

        let vibrato_val: i32 = match wave_ctrl {
            0 => vib_tab[tmp_vib] as i32,
            1 => {
                let val = (tmp_vib as i32) << 3;
                if (vib_pos as i8) < 0 { !val } else { val }
            }
            _ => 255,
        };

        let offset = (vibrato_val * (vib_depth as i32)) >> 5;
        let ch = &mut self.state.channels[channel];

        if (vib_pos as i8) < 0 {
            ch.out_period = ch.real_period.saturating_sub(offset as u16);
        } else {
            ch.out_period = ch.real_period.saturating_add(offset as u16).min(31999);
        }
        if ch.out_period == 0 { ch.out_period = 1; }

        ch.vib_pos = vib_pos.wrapping_add(vib_speed);

        let linear_slides = module.flags.linear_slides;
        self.update_voices_from_period(channel, linear_slides);
    }

    // ─── Tremolo ──────────────────────────────────────────────

    pub(crate) fn apply_tremolo_period(&mut self, channel: usize) {
        let (trem_pos, trem_speed, trem_depth, wave_ctrl) = {
            let ch = &self.state.channels[channel];
            (ch.trem_pos, ch.trem_speed, ch.trem_depth, (ch.wave_ctrl >> 4) & 0x03)
        };
        if trem_depth == 0 { return; }

        let vib_tab = get_vib_tab();
        let tmp_trem = ((trem_pos >> 2) & 0x1F) as usize;

        let trem_val: i32 = match wave_ctrl {
            0 => vib_tab[tmp_trem] as i32,
            1 => {
                let val = (tmp_trem as i32) << 3;
                let ch = &self.state.channels[channel];
                if (ch.vib_pos as i8) < 0 { !val } else { val }
            }
            _ => 255,
        };

        let offset = (trem_val * (trem_depth as i32)) >> 6;
        let mut vol = self.state.channels[channel].real_vol as i32;

        let ch = &mut self.state.channels[channel];
        if (trem_pos as i8) < 0 {
            vol -= offset;
            if vol < 0 { vol = 0; }
        } else {
            vol += offset;
            if vol > 64 { vol = 64; }
        }

        ch.channel_volume = vol as u8;
        ch.trem_pos = trem_pos.wrapping_add(trem_speed);

        let vol_f = self.compute_channel_volume(channel);
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol_f;
            }
        }
    }

    // ─── Arpeggio ─────────────────────────────────────────────

    pub(crate) fn apply_arpeggio_period(&mut self, channel: usize, tick: u8, linear: bool) {
        let (arp1, arp2) = self.state.channels[channel].last_arpeggio;
        let arp_tab = get_arp_tab();
        let arp_tick = arp_tab[tick as usize % 256];

        if arp_tick == 0 {
            self.state.channels[channel].out_period = self.state.channels[channel].real_period;
        } else {
            let note = if arp_tick == 1 { arp1 } else { arp2 };
            let real_period = self.state.channels[channel].real_period;
            let fine_tune = self.state.channels[channel].fine_tune_offset;
            self.state.channels[channel].out_period = relocate_ton(real_period, note, fine_tune, linear);
        }
        self.update_voices_from_period(channel, linear);
    }

    // ─── Volume slide ─────────────────────────────────────────

    pub(crate) fn apply_volume_slide_period(&mut self, channel: usize) {
        let ch = &mut self.state.channels[channel];
        let up = ch.last_volume_slide_up;
        let down = ch.last_volume_slide_down;

        if up > 0 {
            ch.real_vol = (ch.real_vol + up).min(64);
        }
        if down > 0 {
            ch.real_vol = ch.real_vol.saturating_sub(down);
        }
        ch.channel_volume = ch.real_vol;
        let vol = self.compute_channel_volume(channel);
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol;
            }
        }
    }

    // ─── Tremor ───────────────────────────────────────────────

    pub(crate) fn apply_tremor_period(&mut self, channel: usize) {
        let ch = &mut self.state.channels[channel];
        let tremor_sign = ch.tremor_pos_byte & 0x80;
        let mut tremor_data = ch.tremor_pos_byte & 0x7F;

        tremor_data = tremor_data.wrapping_sub(1);
        if tremor_data == 0xFF {
            if tremor_sign == 0x80 {
                ch.tremor_pos_byte = ch.tremor_offtime & 0x0F;
            } else {
                ch.tremor_pos_byte = 0x80 | (ch.tremor_ontime & 0x0F);
            }
        } else {
            ch.tremor_pos_byte = tremor_sign | tremor_data;
        }

        if ch.tremor_pos_byte & 0x80 != 0 {
            ch.channel_volume = 0;
        } else {
            ch.channel_volume = ch.real_vol;
        }
        let vol = self.compute_channel_volume(channel);
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol;
            }
        }
    }

    // ─── Retrigger ────────────────────────────────────────────

    pub(crate) fn do_multi_retrig_period(&mut self, channel: usize, linear: bool) {
        let (cnt, speed, vol_kol, _retrig_vol) = {
            let ch = &mut self.state.channels[channel];
            ch.retrig_cnt += 1;
            if ch.retrig_cnt < ch.retrig_speed {
                return;
            }
            ch.retrig_cnt = 0;

            let mut vol = ch.real_vol as i32;
            match ch.retrig_vol {
                1 => vol -= 1,
                2 => vol -= 2,
                3 => vol -= 4,
                4 => vol -= 8,
                5 => vol -= 16,
                6 => vol = (vol >> 1) + (vol >> 3) + (vol >> 4),
                7 => vol >>= 1,
                9 => vol += 1,
                10 => vol += 2,
                11 => vol += 4,
                12 => vol += 8,
                13 => vol += 16,
                14 => vol = (vol >> 1) + vol,
                15 => vol <<= 1,
                _ => {}
            }
            vol = vol.clamp(0, 64);

            ch.real_vol = vol as u8;
            ch.channel_volume = vol as u8;

            let vk = ch.vol_kol;
            if vk >= 0x10 && vk <= 0x50 {
                ch.real_vol = vk - 0x10;
                ch.channel_volume = ch.real_vol;
            }
            if vk >= 0xC0 && vk <= 0xCF {
                ch.channel_panning = (vk & 0x0F) << 4;
            }

            (ch.retrig_cnt, ch.retrig_speed, ch.vol_kol, ch.retrig_vol)
        };
        let _ = (cnt, speed, vol_kol);

        self.retrig_channel_note_period(channel, linear);
    }

    pub(crate) fn retrig_channel_note_period(&mut self, channel: usize, linear: bool) {
        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };

        let (note, sample_idx, _instrument_idx) = {
            let ch = &self.state.channels[channel];
            (ch.last_note, ch.last_sample, ch.last_instrument)
        };

        if sample_idx == 0 || (sample_idx as usize) >= module.samples.len() {
            return;
        }
        let note_key = match note {
            Note::On(key) => key,
            _ => return,
        };
        let sample = &module.samples[sample_idx as usize];

        let period = get_note_period(
            note_key.saturating_add(sample.relative_note as u8),
            self.state.channels[channel].fine_tune_offset,
            linear,
        );
        self.state.channels[channel].real_period = period;
        self.state.channels[channel].out_period = period;

        let playback_freq = period_to_frequency(period, linear, 8363);
        let vol = self.compute_channel_volume(channel);
        let pan = self.compute_channel_panning(channel);

        let voice_idx = self.allocate_voice(channel);
        let sample_offset = self.calculate_sample_offset(channel, &Cell::default(), sample);
        self.voice_pool.voices[voice_idx].trigger(
            sample.data.clone(), sample.sample_rate as f64, sample.loop_type,
            sample.loop_start, sample.loop_end, playback_freq, self.output_sample_rate,
            vol, pan, sample_offset, Some(self.state.channels[channel].last_instrument), Some(sample_idx),
            note, NewNoteAction::NoteCut, 0,
        );
        self.voice_pool.voices[voice_idx].channel = Some(channel);
    }

    // ─── Delayed note (period-based) ──────────────────────────

    pub(crate) fn trigger_delayed_note_period(&mut self, channel: usize, linear: bool) {
        let cell = match self.state.channels[channel].delayed_cell.take() {
            Some(c) => c,
            None => return,
        };
        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };
        let ch = &mut self.state.channels[channel];

        if cell.instrument.is_some() {
            ch.last_instrument = cell.instrument.unwrap();
        }
        if let Note::On(key) = cell.note {
            ch.last_note = Note::On(key);

            let inst_idx = ch.last_instrument as usize;
            let sample_idx = if inst_idx > 0 && inst_idx < module.instruments.len() && (key as usize) < 120 {
                let inst = &module.instruments[inst_idx];
                inst.sample_map[key as usize] as usize
            } else {
                inst_idx
            };
            let sample = if sample_idx > 0 && sample_idx < module.samples.len() {
                Some(&module.samples[sample_idx])
            } else {
                None
            };

            if let Some(s) = sample {
                let (period, linear, vol_kol_val) = {
                    let period = get_note_period(key.saturating_add(s.relative_note as u8), ch.fine_tune_offset, linear);
                    ch.real_period = period;
                    ch.out_period = period;
                    (period, linear, ch.vol_kol)
                };

                let playback_freq = period_to_frequency(period, linear, 8363);
                let vol = self.compute_channel_volume(channel);
                let pan = self.compute_channel_panning(channel);

                let fade_out = if inst_idx > 0 && inst_idx < module.instruments.len() {
                    module.instruments[inst_idx].fade_out
                } else {
                    0
                };

                let voice_idx = self.allocate_voice(channel);
                let sample_offset = self.calculate_sample_offset(channel, &cell, s);
                self.voice_pool.voices[voice_idx].trigger(
                    s.data.clone(), s.sample_rate as f64, s.loop_type,
                    s.loop_start, s.loop_end, playback_freq, self.output_sample_rate,
                    vol, pan, sample_offset, Some(inst_idx as u8), Some(sample_idx as u8),
                    Note::On(key), NewNoteAction::NoteCut, fade_out,
                );
                self.voice_pool.voices[voice_idx].channel = Some(channel);

                {
                    let ch = &mut self.state.channels[channel];
                    if vol_kol_val >= 0x10 && vol_kol_val <= 0x50 {
                        ch.real_vol = vol_kol_val - 0x10;
                        ch.channel_volume = ch.real_vol;
                    }
                    if vol_kol_val >= 0xC0 && vol_kol_val <= 0xCF {
                        ch.channel_panning = (vol_kol_val & 0xF) << 4;
                    }
                }

                if inst_idx > 0 && inst_idx < module.instruments.len() {
                    let inst = &module.instruments[inst_idx];
                    let voice = &mut self.voice_pool.voices[voice_idx];

                    voice.fade_out_rate = fade_out;
                    voice.fade_out_amp = 32768i32;
                    voice.fade_out_speed_i32 = fade_out as i32;
                    voice.env_sustain_active = true;

                    if let Some(ref vol_env) = inst.volume_envelope {
                        if vol_env.flags.enabled {
                            voice.vol_env = Some(EnvelopeState {
                                envelope: Arc::new(vol_env.clone()),
                                current_point: 0,
                                position: -1.0,
                                released: false,
                                finished: false,
                            });
                        }
                    }
                    if let Some(ref pan_env) = inst.panning_envelope {
                        if pan_env.flags.enabled {
                            voice.pan_env = Some(EnvelopeState {
                                envelope: Arc::new(pan_env.clone()),
                                current_point: 0,
                                position: -1.0,
                                released: false,
                                finished: false,
                            });
                        }
                    }
                    if let Some(ref filter_env) = inst.filter_envelope {
                        if filter_env.flags.enabled {
                            voice.filter_env = Some(EnvelopeState {
                                envelope: Arc::new(filter_env.clone()),
                                current_point: 0,
                                position: -1.0,
                                released: false,
                                finished: false,
                            });
                        }
                    }

                    voice.filter_cutoff = inst.filter_cutoff as f32;
                    voice.filter_resonance = inst.filter_resonance as f32 / 128.0;
                    voice.filter_type = inst.filter_type;
                    voice.svf = StateVariableFilter { low: 0.0, band: 0.0, high: 0.0, filter_type: inst.filter_type };
                    voice.envelope_filter_cutoff = 1.0;

                    if inst.vib_depth > 0 {
                        voice.auto_vib_pos = 0;
                        voice.auto_vib_period_base = period;
                        if inst.vib_sweep > 0 {
                            voice.auto_vib_amp = 0;
                            voice.auto_vib_sweep = (inst.vib_depth as i32) * 256 / (inst.vib_sweep as i32).max(1);
                        } else {
                            voice.auto_vib_amp = (inst.vib_depth as i32) * 256;
                            voice.auto_vib_sweep = 0;
                        }
                    }

                    voice.instrument_index = Some(inst_idx as u8);
                }
            }
        }
    }

    // ─── Voice helpers ───────────────────────────────────────

    pub(crate) fn update_voices_from_period(&mut self, channel: usize, linear: bool) {
        let period = self.state.channels[channel].out_period;
        let freq = if linear {
            period_to_frequency(period, true, 8363)
        } else {
            8363.0 * 428.0 / period as f64
        };
        let delta = if self.output_sample_rate > 0.0 {
            freq / self.output_sample_rate
        } else {
            0.0
        };
        self.voice_pool.update_voices_from_period(channel, freq, delta);
    }

    pub(crate) fn handle_nna(&mut self, channel: usize, nna: NewNoteAction,
        dct: DuplicateCheckType, dca: DuplicateCheckAction,
        instr_idx: usize, sample_idx: usize) {
        self.voice_pool.handle_nna(channel, nna, dct, dca, instr_idx, sample_idx, &self.state);
    }

    pub(crate) fn cut_channel_voices(&mut self, channel: usize) {
        self.voice_pool.cut_channel_voices(channel);
    }

    pub(crate) fn fade_channel_voices(&mut self, channel: usize) {
        self.voice_pool.fade_channel_voices(channel);
    }

    // ─── Portamento up / down / tone ──────────────────────────

    pub(crate) fn apply_portamento_up(&mut self, channel: usize, speed: u16) {
        let module = match self.module.as_ref() {
            Some(m) => m,
            None => return,
        };
        if !module.flags.linear_slides {
            let ch = &mut self.state.channels[channel];
            ch.real_period = ch.real_period.saturating_sub(speed).max(1);
            ch.out_period = ch.real_period;
            self.update_voices_from_period(channel, false);
            return;
        }

        let slide = speed as f64;
        let factor = 2.0_f64.powf(slide / (12.0 * 64.0));
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.current_frequency *= factor;
                voice.sample_delta = voice.current_frequency / self.output_sample_rate;
            }
        }
    }

    pub(crate) fn apply_portamento_down(&mut self, channel: usize, speed: u16) {
        let module = match self.module.as_ref() {
            Some(m) => m,
            None => return,
        };
        if !module.flags.linear_slides {
            let ch = &mut self.state.channels[channel];
            ch.real_period = ch.real_period.saturating_add(speed).min(31999);
            ch.out_period = ch.real_period;
            self.update_voices_from_period(channel, false);
            return;
        }

        let slide = speed as f64;
        let factor = 2.0_f64.powf(slide / (12.0 * 64.0));
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.current_frequency /= factor;
                voice.sample_delta = voice.current_frequency / self.output_sample_rate;
            }
        }
    }

    pub(crate) fn apply_tone_portamento(&mut self, channel: usize, speed: u16) {
        let module = match self.module.as_ref() {
            Some(m) => m,
            None => return,
        };
        let target_period = match self.state.channels[channel].portamento_target_period {
            Some(t) => t,
            None => return,
        };
        let target_freq = self.state.channels[channel].portamento_target_frequency;

        if !module.flags.linear_slides {
            let ch = &mut self.state.channels[channel];
            let want = target_period;

            if ch.real_period < want {
                ch.real_period = ch.real_period.saturating_add(speed).min(want);
            } else if ch.real_period > want {
                ch.real_period = ch.real_period.saturating_sub(speed).max(want);
            }

            if ch.glissando {
                ch.out_period = relocate_ton(ch.real_period, 0, ch.fine_tune_offset, false);
            } else {
                ch.out_period = ch.real_period;
            }
            self.update_voices_from_period(channel, false);
            return;
        }

        let tf = match target_freq {
            Some(f) => f,
            None => return,
        };

        let slide = speed as f64 / (12.0 * 64.0);
        let glissando = self.state.channels[channel].glissando;

        for voice in &mut self.voice_pool.voices {
            if !voice.active || voice.channel != Some(channel) {
                continue;
            }
            let mut current = voice.current_frequency;
            if (current - tf).abs() < 0.5 {
                voice.current_frequency = tf;
            } else if current < tf {
                current = current * 2.0_f64.powf(slide);
                if current > tf { current = tf; }
            } else {
                current = current / 2.0_f64.powf(slide);
                if current < tf { current = tf; }
            }
            if glissando {
                current = quantize_to_semitone(current);
            }
            voice.current_frequency = current;
            voice.sample_delta = voice.current_frequency / self.output_sample_rate;
        }
    }

    // ─── Volume / panning slides ──────────────────────────────

    pub(crate) fn apply_volume_slide(&mut self, channel: usize) {
        let (up, down) = {
            let ch = &self.state.channels[channel];
            if !self.use_xm_model && ch.last_volume_slide_param > 0 {
                ((ch.last_volume_slide_param >> 4) as u8, (ch.last_volume_slide_param & 0x0F) as u8)
            } else {
                (ch.last_volume_slide_up, ch.last_volume_slide_down)
            }
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

        let vol_f = self.compute_channel_volume(channel);
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol_f;
            }
        }
    }

    pub(crate) fn apply_panning_slide(&mut self, channel: usize) {
        let ch = &mut self.state.channels[channel];
        if ch.last_panning_slide == 0 {
            return;
        }
        let delta = ch.last_panning_slide as i16;
        let new_pan = (ch.channel_panning as i16 + delta).clamp(0, 255) as u8;
        ch.channel_panning = new_pan;

        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_panning = new_pan as f32 / 255.0;
            }
        }
    }

    // ─── Vibrato / Tremolo / Arpeggio (frequency-based) ───────

    pub(crate) fn apply_vibrato(&mut self, channel: usize, speed: u8, depth: u8) {
        if depth == 0 { return; }

        if !self.use_xm_model {
            let ch = &mut self.state.channels[channel];
            let vib_tab = get_vib_tab();
            let waveform = ch.wave_ctrl & 0x03;
            let tmp_vib = ((ch.vib_pos >> 2) & 0x1F) as usize;

            let vibrato_val: i32 = match waveform {
                0 => vib_tab[tmp_vib] as i32,
                1 => {
                    let val = (tmp_vib as i32) << 3;
                    if (ch.vib_pos as i8) < 0 { !val } else { val }
                }
                _ => 255,
            };

            let offset = ((vibrato_val * (depth as i32)) >> 3) as u16;
            if (ch.vib_pos as i8) < 0 {
                ch.out_period = ch.real_period.saturating_sub(offset).max(1);
            } else {
                ch.out_period = ch.real_period.saturating_add(offset).min(31999);
            }
            ch.vib_pos = ch.vib_pos.wrapping_add(speed as u8);

            self.update_voices_from_period(channel, false);
            return;
        }

        let depth_f = depth as f32 / 64.0;

        for voice in &mut self.voice_pool.voices {
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

    pub(crate) fn apply_tremolo(&mut self, channel: usize, speed: u8, depth: u8) {
        if depth == 0 { return; }
        let depth_f = depth as f32 / 64.0;

        for voice in &mut self.voice_pool.voices {
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

    pub(crate) fn apply_arpeggio(&mut self, channel: usize, tick: u8, note1: u8, note2: u8) {
        let arp_tick = tick % 3;
        let semitone_offset = match arp_tick {
            0 => 0,
            1 => note1,
            2 => note2,
            _ => 0,
        };
        if semitone_offset == 0 { return; }
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                let freq_mod = 2.0_f64.powf(semitone_offset as f64 / 12.0);
                voice.sample_delta = (voice.base_frequency * freq_mod) / self.output_sample_rate;
            }
        }
    }

    // ─── Panbrello / Tremor / Retrigger ───────────────────────

    pub(crate) fn apply_panbrello(&mut self, channel: usize, speed: u8, depth: u8) {
        if depth == 0 { return; }
        let depth_f = depth as f32 / 64.0;

        for voice in &mut self.voice_pool.voices {
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

    pub(crate) fn apply_tremor(&mut self, channel: usize, _tick: u8, ontime: u8, offtime: u8) {
        if ontime == 0 && offtime == 0 { return; }
        let cycle = ontime as u16 + offtime as u16;
        if cycle == 0 { return; }
        let counter = self.state.channels[channel].tremor_counter as u16;
        let phase = counter % cycle;
        let mute = phase >= ontime as u16;
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.tremor_mute = mute;
            }
        }
        self.state.channels[channel].tremor_counter = self.state.channels[channel].tremor_counter.wrapping_add(1);
    }

    pub(crate) fn retrigger_channel_note(&mut self, channel: usize) {
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

        self.handle_nna(channel, NewNoteAction::NoteCut,
            DuplicateCheckType::Disabled, DuplicateCheckAction::NoteCut,
            instrument_idx as usize, sample_idx as usize);

        let voice_idx = self.allocate_voice(channel);
        self.voice_pool.voices[voice_idx].trigger(
            sample.data.clone(), sample.sample_rate as f64, sample.loop_type,
            sample.loop_start, sample.loop_end, playback_freq, self.output_sample_rate,
            vol, pan, 0, Some(instrument_idx), Some(sample_idx),
            note, NewNoteAction::NoteCut, 0,
        );
        self.voice_pool.voices[voice_idx].channel = Some(channel);
    }

    // ─── Cutoff / delay ticks ─────────────────────────────────

    pub(crate) fn set_channel_cutoff_tick(&mut self, channel: usize, ticks: u8) {
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.cutoff_tick = Some(ticks as u16);
            }
        }
    }

    pub(crate) fn get_channel_cutoff_tick(&self, channel: usize) -> Option<u16> {
        for voice in &self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                return voice.cutoff_tick;
            }
        }
        None
    }

    pub(crate) fn set_channel_delay_tick(&mut self, channel: usize, ticks: u8) {
        for voice in &mut self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.delay_tick = Some(ticks as u16);
            }
        }
    }

    pub(crate) fn get_channel_delay_tick(&self, channel: usize) -> Option<u16> {
        let ch = &self.state.channels[channel];
        if ch.note_delay_ticks > 0 {
            return Some(ch.note_delay_ticks as u16);
        }
        for voice in &self.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                return voice.delay_tick;
            }
        }
        None
    }

    // ─── Delayed note (frequency-based) ───────────────────────

    pub(crate) fn trigger_delayed_note(&mut self, channel: usize) {
        let cell = match self.state.channels[channel].delayed_cell.take() {
            Some(c) => c,
            None => return,
        };
        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };

        if cell.instrument.is_some() {
            self.state.channels[channel].last_instrument = cell.instrument.unwrap();
        }

        let (note, instrument_idx) = (cell.note, self.state.channels[channel].last_instrument as usize);

        let (sample_idx, remapped_key) = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            let inst = &module.instruments[instrument_idx];
            match note {
                Note::On(key) if (key as usize) < 120 => {
                    let idx = inst.sample_map[key as usize] as usize;
                    let rk = inst.note_map[key as usize];
                    (idx, if rk < 120 { rk } else { key })
                }
                _ => (self.state.channels[channel].last_sample as usize, {
                    match note { Note::On(k) => k, _ => 0 }
                }),
            }
        } else {
            (instrument_idx, match note { Note::On(k) => k, _ => 0 })
        };

        if sample_idx == 0 || sample_idx >= module.samples.len() {
            return;
        }

        if cell.instrument.is_some() {
            self.state.channels[channel].channel_volume = module.samples[sample_idx].default_volume.min(64);
        }

        self.state.channels[channel].note_delay_ticks = 0;

        if let Note::On(key) = note {
            let module_ref = &module;
            self.with_processor_mut(|processor, engine| processor.trigger_note(engine, channel, key, remapped_key, Some(&module_ref.samples[sample_idx]), sample_idx, &cell, instrument_idx));

            if instrument_idx > 0 && instrument_idx < module.instruments.len() {
                let voice = self.voice_pool.voices.iter_mut().find(|v| v.active && v.channel == Some(channel));
                if let Some(voice) = voice {
                    let fade_out = module.instruments[instrument_idx].fade_out;
                    voice.fade_out_rate = fade_out;
                    voice.fade_out_amp = 32768i32;
                    voice.fade_out_speed_i32 = fade_out as i32;
                    voice.env_sustain_active = true;
                }
            }
        }
    }

    pub(crate) fn set_envelope_position(&mut self, channel: usize, tick: u16) {
        self.voice_pool.set_envelope_position(channel, tick);
    }

    // ─── Public helper method wrappers ────────────────────────

    pub(crate) fn allocate_voice(&mut self, channel: usize) -> usize {
        self.voice_pool.allocate_voice(channel)
    }

    pub(crate) fn compute_channel_volume(&self, channel: usize) -> f32 {
        compute_channel_volume(&self.state, channel, self.use_xm_model)
    }

    pub(crate) fn compute_channel_panning(&self, channel: usize) -> f32 {
        compute_channel_panning(&self.state, channel)
    }

    pub(crate) fn compute_portamento_target(
        &self,
        _channel: usize,
        _note_key: u8,
        remapped_key: u8,
        sample: Option<&Sample>,
        sample_idx: usize,
        module: &Module,
    ) -> (u16, f64) {
        compute_portamento_target(_channel, _note_key, remapped_key, sample, sample_idx, module)
    }
}
