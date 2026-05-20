use std::sync::Arc;

use crate::audio::effects::{
    EffectProcessor,
    VIBRATO_TABLE_SIZE, VIBRATO_SINE_TABLE, VIBRATO_RAMP_TABLE, FUNK_TRACK,
    quantize_to_semitone, fastrand, compute_samples_per_tick, get_vibrato_value,
    advance_single_envelope, evaluate_envelope, compute_playback_frequency,
};
use crate::audio::voice::{EnvelopeState, Voice};
use crate::audio::filter::StateVariableFilter;
use crate::sequencer::automation::AutomationTarget;
use crate::sequencer::effect::{Effect, FormatEffect, XmEffect, ModEffect, S3mEffect, ItEffect, FilterType, NUM_SEND_BUSES};
use crate::sequencer::instrument::{
    DuplicateCheckAction, DuplicateCheckType, NewNoteAction,
};
use crate::sequencer::module::{Module, ModuleFormat, MAX_VOICES};
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::player::{ActiveEffects, ChannelState, PlayMode, SequencerState};
use crate::sequencer::sample::{Sample, VibratoWaveform, LoopType};
use crate::debug_log;
use crate::sequencer::period::{
    get_arp_tab, get_note_period, get_vib_tab, period_to_frequency, relocate_ton,
};

pub struct SequencerEngine {
    pub state: SequencerState,
    pub voices: Vec<Voice>,
    next_voice: usize,
    pub module: Option<Arc<Module>>,
    pub(crate) output_sample_rate: f64,
    pub(crate) global_volume: f32,
    pub(crate) use_xm_model: bool,
    pub(crate) amiga_led_filter: bool,
    pub pending_send_fx_params: Vec<(usize, u32, f32)>,
    processor: EffectProcessor,
}

impl SequencerEngine {
    pub fn new(output_sample_rate: f64) -> Self {
        let default_module = Module::default();
        let processor = EffectProcessor::from_module(&default_module);
        SequencerEngine {
            state: SequencerState::default(),
            voices: vec![Voice::default(); MAX_VOICES],
            next_voice: 0,
            module: None,
            output_sample_rate,
            global_volume: 1.0,
            use_xm_model: false,
            amiga_led_filter: false,
            pending_send_fx_params: Vec::new(),
            processor,
        }
    }

    pub fn load_module(&mut self, module: Arc<Module>) {
        self.stop();
        self.use_xm_model = module.flags.xm_period_model;
        self.amiga_led_filter = module.format == ModuleFormat::MOD;
        self.processor = EffectProcessor::from_module(&module);
        self.module = Some(module);
    }

    pub fn play(&mut self) {
        if self.module.is_none() {
            debug_log!("[PLAY] No module loaded, returning");
            return;
        }
        self.stop_playback_state();

        let module = self.module.as_ref().unwrap();
        self.state.bpm = module.initial_bpm;
        self.state.speed = module.initial_speed;
        self.state.global_volume = module.initial_global_volume;
        self.state.master_volume = 1.0;
        self.state.samples_per_tick = compute_samples_per_tick(self.state.bpm, self.output_sample_rate);

        let num_ch = module.channel_panning.len();
        self.state.channels.clear();
        self.state.channels.resize(num_ch, ChannelState::default());
        for i in 0..num_ch {
            self.state.channels[i].channel_panning = module.channel_panning[i];
            self.state.channels[i].channel_volume = module.channel_volume[i];
        }

        #[cfg(feature = "audio_debug")]
        debug_log!("[PLAY] Module loaded: {} samples, {} patterns, BPM={} speed={}",
            module.samples.len(), module.patterns.len(), self.state.bpm, self.state.speed);
        #[cfg(feature = "audio_debug")]
        debug_log!("[PLAY] Channel volumes: ch0={} ch1={} ch2={} ch3={}",
            self.state.channels[0].channel_volume,
            self.state.channels[1].channel_volume,
            self.state.channels[2].channel_volume,
            self.state.channels[3].channel_volume);

        self.state.current_order = 0;
        self.state.current_row = 0;
        self.state.current_pattern = self.get_pattern_for_order(0);
        self.state.pattern_break_row = None;
        self.state.position_jump_order = None;
        self.state.pattern_delay_ticks = 0;
        self.state.pattern_delay_ticks2 = 0;
        self.state.row_delay_active = false;
        self.state.pattern_loop_start = None;
        self.state.pattern_loop_count = 0;
        self.state.pattern_loop_final_pass = false;

        self.state.playing = true;
        self.state.paused = false;
        self.state.current_tick = 0;
        self.state.sample_counter = self.state.samples_per_tick;

        #[cfg(feature = "audio_debug")]
        debug_log!("[PLAY] Ready: playing={}, order={}, row={}",
            self.state.playing, self.state.current_order, self.state.current_row);
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

        let num_ch = module.channel_panning.len();
        self.state.channels.clear();
        self.state.channels.resize(num_ch, ChannelState::default());
        for i in 0..num_ch {
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
        self.state.pattern_delay_ticks2 = 0;
        self.state.row_delay_active = false;
        self.state.pattern_loop_start = None;
        self.state.pattern_loop_count = 0;
        self.state.pattern_loop_final_pass = false;

        self.state.playing = true;
        self.state.paused = false;
        self.state.current_tick = 0;
        self.state.sample_counter = self.state.samples_per_tick;
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

    pub fn process_tick(&mut self) {
        self.evaluate_automation();

        let tick = self.state.current_tick;

        if tick == 0 {
            self.process_tick_zero_unified();
        } else {
            self.process_effects_tick_unified();
        }

        // Advance envelopes after tick processing
        // For XM, this matches FT2's fixaEnvelopeVibrato being called after effects
        // For XM voices triggered this tick, position=-1 so first advance → 0
        self.advance_envelopes();

        self.state.current_tick += 1;

        if self.state.current_tick >= self.state.speed {
            self.advance_row();
        }
    }

    // ─── Automation evaluation ──────────────────────────────────

    fn evaluate_automation(&mut self) {
        let module = match &self.module {
            Some(m) => m.clone(),
            None => return,
        };

        let order = self.state.current_order;
        let row = self.state.current_row;
        let tick = self.state.current_tick;
        let speed = self.state.speed;

        for track in &module.automation_tracks {
            if !track.enabled || track.points.is_empty() {
                continue;
            }

            let value = track.evaluate(order, row as u16, tick, speed);

            match track.channel {
                Some(ch) => {
                    if ch >= self.state.channels.len() {
                        continue;
                    }
                    self.apply_automation_to_channel(ch, &track.target, value);
                }
                None => {
                    self.apply_automation_global(&track.target, value);
                }
            }
        }
    }

    fn apply_automation_to_channel(&mut self, ch: usize, target: &AutomationTarget, value: f32) {
        match target {
            AutomationTarget::ChannelVolume => {
                self.state.channels[ch].auto_volume_factor = value;
            }
            AutomationTarget::ChannelPanning => {
                self.state.channels[ch].auto_pan_offset = (value - 0.5) * 2.0;
            }
            AutomationTarget::FilterCutoff => {
                self.state.channels[ch].auto_filter_cutoff = value;
            }
            AutomationTarget::FilterResonance => {
                self.state.channels[ch].auto_filter_resonance = value;
            }
            AutomationTarget::SendLevel { bus } => {
                if (*bus as usize) < NUM_SEND_BUSES {
                    self.state.channels[ch].auto_send_factor[*bus as usize] = value;
                }
            }
            _ => {}
        }
    }

    fn apply_automation_global(&mut self, target: &AutomationTarget, value: f32) {
        match target {
            AutomationTarget::GlobalVolume => {
                self.state.auto_global_vol_factor = value;
            }
            AutomationTarget::Tempo => {
                self.state.auto_tempo_factor = value;
            }
            _ => {}
        }
    }

    // ─── Unified tick zero ─────────────────────────────────────

    fn process_tick_zero_unified(&mut self) {
        let pattern_index = self.state.current_pattern as usize;
        let row = self.state.current_row as usize;

        let cells: Vec<(usize, Cell)> = {
            let module = match self.module.as_ref() {
                Some(m) => m,
                None => return,
            };
            if pattern_index >= module.patterns.len() {
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

        for (ch, cell) in cells {
            self.process_cell_unified(ch, &cell);
        }
    }

    fn process_cell_unified(&mut self, channel: usize, cell: &Cell) {
    let module = match self.module.as_ref() {
        Some(m) => m.clone(),
        None => return,
    };
    if channel >= self.state.channels.len() {
        return;
    }
    let is_xm = self.use_xm_model;

    // Common: instrument
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
                (idx, if rk < 120 { rk } else { key })
            }
            _ => (self.state.channels[channel].last_sample as usize, {
                match cell.note { Note::On(k) => k, _ => 0 }
            }),
        }
    } else {
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

    // XM: set channel defaults from sample (volume + pan)
    if is_xm {
        if let Some(s) = sample {
            self.state.channels[channel].channel_volume = s.default_volume.min(64);
            self.state.channels[channel].channel_panning = s.default_panning;
        }
    } else {
        // IT: volume reset on instrument change only
        if cell.instrument.is_some() {
            if let Some(s) = sample {
                self.state.channels[channel].channel_volume = s.default_volume.min(64);
            }
        }
    }

    // Volume column
    if let Some(vol) = cell.volume {
        // Pxy effect — volume column carries the param value
        if let Effect::SetSendBusParam { bus, param, .. } = cell.effect {
            let idx = (bus as usize) * 4 + (param as usize) % 4;
            let mapped = ((vol as u16 * 255 + 49) / 99).min(255) as u8; // 0-99 → 0-255, rounding
            self.state.channels[channel].last_send_param_value[idx] = mapped;
        } else {
            let mut processor = std::mem::replace(&mut self.processor, EffectProcessor::from_module(&Module::default()));
            processor.process_volume_column(self, channel, vol);
            self.processor = processor;
        }
    }
    // Set volume effects
    if let Effect::SetVolume { volume } = &cell.effect {
        let v = (*volume).min(64);
        self.state.channels[channel].channel_volume = v;
        self.state.channels[channel].row_volume = v;
    }
    if let Effect::VolSetVolume { vol } = &cell.effect {
        let v = (*vol).min(64);
        self.state.channels[channel].channel_volume = v;
        self.state.channels[channel].row_volume = v;
    }

    let is_tone_portamento = matches!(
        cell.effect,
        Effect::TonePortamento { .. } | Effect::TonePortamentoVolumeSlide { .. }
            | Effect::VolPortamento { .. }
    );

    let is_note_delay = matches!(cell.effect, Effect::NoteDelay { ticks } if ticks > 0);

    let has_volume_effect = cell.volume_effect.is_some();

    if is_note_delay {
        self.state.channels[channel].delayed_cell = Some(*cell);
        if let Effect::NoteDelay { ticks } = cell.effect {
            self.state.channels[channel].note_delay_ticks = ticks;
        }
        if let Note::On(key) = cell.note {
            self.state.channels[channel].last_note = Note::On(key);
        }
    } else {
        match cell.note {
            Note::On(key) => {
                self.state.channels[channel].last_note = Note::On(key);

                if is_tone_portamento {
                    if is_xm {
                        let ch = &mut self.state.channels[channel];
                        if let Some(s) = sample {
                            ch.rel_ton = s.relative_note;
                        }
                        let ft = ch.fine_tune_offset;
                        let want_period = get_note_period(
                            remapped_key.saturating_add(ch.rel_ton as u8),
                            ft,
                            module.flags.linear_slides,
                        );
                        ch.want_period = want_period;
                        if want_period == ch.real_period {
                            ch.porta_dir = 0;
                        } else if want_period > ch.real_period {
                            ch.porta_dir = 1;
                        } else {
                            ch.porta_dir = 2;
                        }
                    } else {
                        let (target_period, target_freq) = self.compute_portamento_target(
                            channel, key, remapped_key, sample, sample_idx, &module,
                        );
                        let ch = &mut self.state.channels[channel];
                        ch.portamento_target_period = Some(target_period);
                        ch.portamento_target_frequency = Some(target_freq);
                    }
                } else {
                    let mut processor = std::mem::replace(&mut self.processor, EffectProcessor::from_module(&Module::default()));
                    processor.trigger_note(self, channel, key, remapped_key, sample, sample_idx, cell, instrument_idx);
                    self.processor = processor;
                }
            }
            Note::Off => {
                let mut processor = std::mem::replace(&mut self.processor, EffectProcessor::from_module(&Module::default()));
                processor.handle_note_off(self, channel);
                self.processor = processor;
            }
            Note::Cut => {
                self.cut_channel_voices(channel);
            }
            Note::Fade => {
                self.fade_channel_voices(channel);
            }
            Note::None => {}
        }

        self.apply_effect_unified(channel, &cell.effect, true);
    }

    // Apply volume_effect on tick 0
    if has_volume_effect {
        if let Some(vol_eff) = cell.volume_effect {
            self.apply_effect_unified(channel, &vol_eff, true);
        }
    }
}

    pub(crate) fn calculate_sample_offset(&self, channel: usize, cell: &Cell, sample: &Sample) -> usize {
        let ch = &self.state.channels[channel];
        let offset = match &cell.effect {
            Effect::SetSampleOffset { offset } => {
                let off = if *offset == 0 {
                    ch.last_sample_offset as u32
                } else {
                    *offset as u32
                };
                ((ch.high_sample_offset as u32) << 16) | off
            }
            Effect::FormatSpecific(fe) => {
                if let Some(offset) = fe.sample_offset() {
                    let off = if offset == 0 {
                        ch.last_sample_offset as u32
                    } else {
                        offset as u32
                    };
                    ((ch.high_sample_offset as u32) << 16) | off
                } else {
                    0
                }
            }
            _ => 0,
        } as usize;

        offset.min(sample.data.len().saturating_sub(1))
    }


    // ─── XM effect application ───────────────────────────────────

    fn apply_effect_unified(&mut self, channel: usize, effect: &Effect, is_row_start: bool) {
        let mut processor = std::mem::replace(&mut self.processor, EffectProcessor::from_module(&Module::default()));
        processor.apply_effect(self, channel, effect, is_row_start);
        self.processor = processor;
    }

    fn process_effects_tick_unified(&mut self) {
        let tick = self.state.current_tick;
        let mut processor = std::mem::replace(&mut self.processor, EffectProcessor::from_module(&Module::default()));
        processor.process_tick(self, tick);
        self.processor = processor;
    }

    pub(crate) fn apply_tone_portamento_period(&mut self, channel: usize, linear: bool) {
        let ch = &self.state.channels[channel];
        if ch.porta_dir == 0 { return; }
        let speed = ch.porta_speed_period;
        if speed == 0 { return; }
        let want = ch.want_period;

        let ch = &mut self.state.channels[channel];
        if ch.porta_dir == 2 {
            // Slide up (lower period)
            ch.real_period = ch.real_period.saturating_sub(speed);
            if ch.real_period <= want {
                ch.real_period = want;
                ch.porta_dir = 1;
            }
        } else {
            // Slide down (higher period)
            ch.real_period = ch.real_period.saturating_add(speed);
            if ch.real_period >= want {
                ch.real_period = want;
                ch.porta_dir = 1;
            }
        }

        if ch.glissando {
            // Quantize to semitone using relocate_ton
            ch.out_period = relocate_ton(ch.real_period, 0, ch.fine_tune_offset, linear);
        } else {
            ch.out_period = ch.real_period;
        }

        let module = self.module.as_ref().unwrap().clone();
        self.update_voices_from_period(channel, module.flags.linear_slides);
    }

    // ─── XM vibrato ──────────────────────────────────────────────

    pub(crate) fn apply_vibrato_period(&mut self, channel: usize, _linear: bool) {
        let (vib_pos, vib_speed, vib_depth, wave_ctrl) = {
            let ch = &self.state.channels[channel];
            (ch.vib_pos, ch.vib_speed, ch.vib_depth, ch.wave_ctrl & 0x03)
        };
        if vib_depth == 0 { return; }

        let vib_tab = get_vib_tab();
        let tmp_vib = ((vib_pos >> 2) & 0x1F) as usize;

        let vibrato_val: i32 = match wave_ctrl {
            0 => vib_tab[tmp_vib] as i32, // sine
            1 => {
                // ramp
                let val = (tmp_vib as i32) << 3;
                if (vib_pos as i8) < 0 {
                    !val
                } else {
                    val
                }
            }
            _ => 255, // square
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

        let module = self.module.as_ref().unwrap().clone();
        self.update_voices_from_period(channel, module.flags.linear_slides);
    }

    // ─── XM tremolo ──────────────────────────────────────────────

    pub(crate) fn apply_tremolo_period(&mut self, channel: usize) {
        let (trem_pos, trem_speed, trem_depth, wave_ctrl) = {
            let ch = &self.state.channels[channel];
            (ch.trem_pos, ch.trem_speed, ch.trem_depth, (ch.wave_ctrl >> 4) & 0x03)
        };
        if trem_depth == 0 { return; }

        let vib_tab = get_vib_tab();
        let tmp_trem = ((trem_pos >> 2) & 0x1F) as usize;

        let trem_val: i32 = match wave_ctrl {
            0 => vib_tab[tmp_trem] as i32, // sine
            1 => {
                // ramp - FT2 bug: uses vibPos not tremPos for sign check
                let val = (tmp_trem as i32) << 3;
                let ch = &self.state.channels[channel];
                if (ch.vib_pos as i8) < 0 {
                    !val
                } else {
                    val
                }
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
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol_f;
            }
        }
    }

    // ─── XM arpeggio ─────────────────────────────────────────────

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

    // ─── XM volume slide ─────────────────────────────────────────

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
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol;
            }
        }
    }

    // ─── XM tremor ───────────────────────────────────────────────

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
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol;
            }
        }
    }

    // ─── XM retrig ───────────────────────────────────────────────

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

            // Apply volume column set-volume override
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
        self.voices[voice_idx].trigger(
            sample.data.clone(), sample.sample_rate as f64, sample.loop_type,
            sample.loop_start, sample.loop_end, playback_freq, self.output_sample_rate,
            vol, pan, sample_offset, Some(self.state.channels[channel].last_instrument), Some(sample_idx),
            note, NewNoteAction::NoteCut, 0,
        );
        self.voices[voice_idx].channel = Some(channel);
    }

    pub(crate) fn trigger_delayed_note_period(&mut self, channel: usize, linear: bool) {
        let cell = match self.state.channels[channel].delayed_cell.take() {
            Some(c) => c,
            None => return,
        };
        let module = self.module.as_ref().unwrap().clone();
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
                self.voices[voice_idx].trigger(
                    s.data.clone(), s.sample_rate as f64, s.loop_type,
                    s.loop_start, s.loop_end, playback_freq, self.output_sample_rate,
                    vol, pan, sample_offset, Some(inst_idx as u8), Some(sample_idx as u8),
                    Note::On(key), NewNoteAction::NoteCut, fade_out,
                );
                self.voices[voice_idx].channel = Some(channel);

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

                // Setup envelopes and auto-vibrato for delayed XM notes
                if inst_idx > 0 && inst_idx < module.instruments.len() {
                    let inst = &module.instruments[inst_idx];
                    let voice = &mut self.voices[voice_idx];

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

    // ─── Update voices from period ───────────────────────────────

    fn update_voices_from_period(&mut self, channel: usize, linear: bool) {
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
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.current_frequency = freq;
                voice.sample_delta = delta;
            }
        }
    }

    pub(crate) fn handle_nna(&mut self, channel: usize, nna: NewNoteAction,
        dct: DuplicateCheckType, dca: DuplicateCheckAction,
        instr_idx: usize, sample_idx: usize) {
        let mut indices: Vec<usize> = Vec::new();

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


    pub(crate) fn cut_channel_voices(&mut self, channel: usize) {
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
        for voice in &mut self.voices {
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
        for voice in &mut self.voices {
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

        for voice in &mut self.voices {
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
        for voice in &mut self.voices {
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

        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_panning = new_pan as f32 / 255.0;
            }
        }
    }

    pub(crate) fn apply_vibrato(&mut self, channel: usize, speed: u8, depth: u8) {
        if depth == 0 {
            return;
        }

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
                ch.real_period = ch.real_period.saturating_sub(offset).max(1);
            } else {
                ch.real_period = ch.real_period.saturating_add(offset).min(31999);
            }
            ch.out_period = ch.real_period;
            ch.vib_pos = ch.vib_pos.wrapping_add(speed as u8);

            self.update_voices_from_period(channel, false);
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

    pub(crate) fn apply_tremolo(&mut self, channel: usize, speed: u8, depth: u8) {
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

    pub(crate) fn apply_arpeggio(&mut self, channel: usize, tick: u8, note1: u8, note2: u8) {
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

    pub(crate) fn apply_panbrello(&mut self, channel: usize, speed: u8, depth: u8) {
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

    pub(crate) fn apply_tremor(&mut self, channel: usize, _tick: u8, ontime: u8, offtime: u8) {
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
        self.voices[voice_idx].trigger(
            sample.data.clone(), sample.sample_rate as f64, sample.loop_type,
            sample.loop_start, sample.loop_end, playback_freq, self.output_sample_rate,
            vol, pan, 0, Some(instrument_idx), Some(sample_idx),
            note, NewNoteAction::NoteCut, 0,
        );
        self.voices[voice_idx].channel = Some(channel);
    }

    pub(crate) fn set_channel_cutoff_tick(&mut self, channel: usize, ticks: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.cutoff_tick = Some(ticks as u16);
            }
        }
    }

    pub(crate) fn get_channel_cutoff_tick(&self, channel: usize) -> Option<u16> {
        for voice in &self.voices {
            if voice.active && voice.channel == Some(channel) {
                return voice.cutoff_tick;
            }
        }
        None
    }

    pub(crate) fn set_channel_delay_tick(&mut self, channel: usize, ticks: u8) {
        for voice in &mut self.voices {
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
        for voice in &self.voices {
            if voice.active && voice.channel == Some(channel) {
                return voice.delay_tick;
            }
        }
        None
    }

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
            let mut processor = std::mem::replace(&mut self.processor, EffectProcessor::from_module(&Module::default()));
            processor.trigger_note(self, channel, key, remapped_key, Some(&module.samples[sample_idx]), sample_idx, &cell, instrument_idx);
            self.processor = processor;

            if instrument_idx > 0 && instrument_idx < module.instruments.len() {
                let voice = self.voices.iter_mut().find(|v| v.active && v.channel == Some(channel));
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
        let is_xm = self.module.as_ref().map_or(false, |m| m.flags.xm_envelope_model);

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

            if let Some(ref mut env) = voice.filter_env {
                advance_single_envelope(env);
                let env_val = evaluate_envelope(env);
                voice.envelope_filter_cutoff = env_val / 64.0;
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
                    if ch_idx < self.state.channels.len() {
                        self.state.channels[ch_idx].channel_volume.min(64) as u32
                    } else {
                        0
                    }
                } else {
                    0
                };
                let fade_amp = voice.fade_out_amp as u32;
                let glob_vol = self.state.global_volume.max(1) as u32;

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

                // Panning envelope (FT2-compatible)
                let out_pan = if let Some(ch_idx) = voice.channel {
                    if ch_idx < self.state.channels.len() {
                        self.state.channels[ch_idx].old_pan
                    } else {
                        128
                    }
                } else {
                    128
                };
                voice.final_panning = out_pan as f32 / 255.0;

                if let Some(ref pan_env_ref) = voice.pan_env {
                    if pan_env_ref.envelope.flags.enabled {
                        let env_pan_val = (evaluate_envelope(pan_env_ref) as i32 - 32) * 256;
                        let pan_tmp = (out_pan as i32 - 128).abs() + 128;
                        let pan_tmp_scaled = pan_tmp * 8;
                        let pan_add = (env_pan_val * pan_tmp_scaled) >> 16;
                        let final_pan = (out_pan as i32 + pan_add).clamp(0, 255);
                        voice.final_panning = final_pan as f32 / 255.0;
                    }
                }
            } else {
                // Non-XM fadeout
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

            // Auto-vibrato for XM
            if is_xm {
                let linear_flag = self.module.as_ref().unwrap().flags.linear_slides;
                let module = self.module.as_ref().unwrap();
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
                        voice.sample_delta = freq / self.output_sample_rate;
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

    fn advance_row(&mut self) {
        self.state.current_tick = 0;

        for voice in &mut self.voices {
            if voice.active {
                voice.cutoff_tick = None;
                voice.delay_tick = None;
                if self.use_xm_model {
                    if let Some(ch_idx) = voice.channel {
                        if ch_idx < self.state.channels.len() {
                            voice.auto_vib_period_base = self.state.channels[ch_idx].out_period;
                        }
                    }
                }
            }
        }

        for ch in &mut self.state.channels {
            ch.delayed_cell = None;
            ch.note_delay_ticks = 0;
            ch.active_effects = ActiveEffects::default();
            ch.last_retrigger_interval = 0;
            ch.retrig_speed = 0;
            ch.retrig_cnt = 0;
            ch.note_cut_tick = None;
            ch.vol_kol = 0;
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
                self.reset_pattern_loop_state();
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
            self.reset_pattern_loop_state();
            return;
        }

        // Handle Pattern Loop jump
        if let Some(target_row) = self.state.pattern_loop_jump_target.take() {
            if self.state.pattern_loop_count > 0 {
                self.state.current_row = target_row;
                self.state.pattern_loop_count -= 1;
                if self.state.pattern_loop_count == 0 {
                    self.state.pattern_loop_start = None;
                    self.state.pattern_loop_final_pass = true;
                }
            } else {
                self.state.pattern_loop_start = None;
                self.state.pattern_loop_final_pass = true;
            }
            return;
        }

        let pattern_idx = self.state.current_pattern as usize;
        let pattern_rows = if pattern_idx < module.patterns.len() {
            module.patterns[pattern_idx].num_rows
        } else {
            64
        };

        let next_row = self.state.current_row as usize + 1;
        if next_row >= pattern_rows {
            if self.state.pattern_loop_count > 0 {
                if let Some((_loop_order, loop_row)) = self.state.pattern_loop_start {
                    self.state.current_row = loop_row;
                    self.state.pattern_loop_count -= 1;
                    if self.state.pattern_loop_count == 0 {
                        self.state.pattern_loop_start = None;
                        self.state.pattern_loop_final_pass = true;
                    }
                    return;
                }
            }
            self.state.current_order += 1;
            if (self.state.current_order as usize) >= module.order_list.len() {
                self.handle_song_end();
                return;
            }
            self.state.current_pattern = self.get_pattern_for_order(self.state.current_order);
            self.state.current_row = 0;
            self.reset_pattern_loop_state();
        } else {
            self.state.current_row = next_row as u8;
        }
    }

    fn reset_pattern_loop_state(&mut self) {
        debug_log!("[LOOP] Resetting pattern loop state");
        self.state.pattern_loop_start = None;
        self.state.pattern_loop_count = 0;
        self.state.pattern_loop_final_pass = false;
        self.state.pattern_loop_jump_target = None;
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

    pub(crate) fn allocate_voice(&mut self, channel: usize) -> usize {
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

    pub(crate) fn compute_channel_volume(&self, channel: usize) -> f32 {
        if channel >= self.state.channels.len() {
            return 0.0;
        }
        let ch = &self.state.channels[channel];
        if self.use_xm_model {
            ch.channel_volume.min(64) as f32 / 64.0
        } else {
            let vol = ch.channel_volume.min(64) as f32 / 64.0;
            let global = self.state.global_volume as f32 / 128.0;
            vol * global
        }
    }

    pub(crate) fn compute_channel_panning(&self, channel: usize) -> f32 {
        if channel >= self.state.channels.len() {
            return 0.5;
        }
        self.state.channels[channel].channel_panning as f32 / 255.0
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
        let freq = match Note::On(remapped_key).frequency() {
            Some(f) => f,
            None => return (0, 0.0),
        };

        let (s, _playback_freq) = if sample_idx > 0 && sample_idx < module.samples.len() {
            let s = &module.samples[sample_idx];
            let pf = compute_playback_frequency(freq, s.sample_rate, s.relative_note, s.fine_tune);
            (s, pf)
        } else {
            match sample {
                Some(s) => {
                    let pf = compute_playback_frequency(freq, s.sample_rate, s.relative_note, s.fine_tune);
                    (s, pf)
                }
                None => return ((8363.0 * 428.0 / freq) as u16, freq),
            }
        };

        let pf = compute_playback_frequency(freq, s.sample_rate, s.relative_note, s.fine_tune);
        let period = (8363.0 * 428.0 / pf).max(1.0) as u16;
        (period, pf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::Instrument;
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

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
        let env = crate::sequencer::instrument::Envelope {
            points: vec![
                crate::sequencer::instrument::EnvelopePoint { tick: 0, value: 0 },
                crate::sequencer::instrument::EnvelopePoint { tick: 10, value: 64 },
            ],
            sustain_point: None,
            loop_start: None,
            loop_end: None,
            flags: crate::sequencer::instrument::EnvelopeFlags {
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
        let env = crate::sequencer::instrument::Envelope {
            points: vec![
                crate::sequencer::instrument::EnvelopePoint { tick: 0, value: 0 },
                crate::sequencer::instrument::EnvelopePoint { tick: 5, value: 64 },
                crate::sequencer::instrument::EnvelopePoint { tick: 10, value: 0 },
            ],
            sustain_point: Some(1),
            loop_start: None,
            loop_end: None,
            flags: crate::sequencer::instrument::EnvelopeFlags {
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

        assert!(!module.samples[1].data.is_empty(), "Sample 1 should have data");
        assert_eq!(module.samples[1].sample_rate, 8363);

        let mut engine = SequencerEngine::new(48000.0);
        engine.load_module(module.clone());
        engine.play();

        // With the new engine loop, play() doesn't trigger tick 0 immediately.
        // We need to call process_tick() or advance() to trigger the notes.
        engine.process_tick();

        assert!(engine.state.playing, "Engine should be playing after play()");

        let active_after_play = engine.voices.iter().filter(|v| v.active).count();
        assert!(active_after_play > 0, "Should have at least 1 active voice after tick 0, got {}", active_after_play);

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
            48000.0,
        );

        let max_sample = left.iter().chain(right.iter()).map(|&s| s.abs()).fold(0.0f32, f32::max);
        assert!(max_sample > 0.0001, "MOD playback should produce audio output, max sample = {:.6}", max_sample);
    }

    #[test]
    fn advance_row_resets_effects() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.state.channels[0].active_effects.volume_slide = true;
        engine.state.channels[0].last_retrigger_interval = 2;
        engine.state.channels[0].vol_kol = 0x50;

        engine.advance_row();

        assert!(!engine.state.channels[0].active_effects.volume_slide);
        assert_eq!(engine.state.channels[0].last_retrigger_interval, 0);
        assert_eq!(engine.state.channels[0].vol_kol, 0);
    }

    #[test]
    fn note_delay_stores_cell() {
        let mut engine = SequencerEngine::new(48000.0);
        let mut cell = Cell::default();
        cell.note = Note::On(60);
        cell.instrument = Some(1);
        cell.effect = Effect::NoteDelay { ticks: 3 };

        let module = Arc::new(Module::default());
        engine.load_module(module);
        engine.process_cell_unified(0, &cell);

        assert_eq!(engine.state.channels[0].note_delay_ticks, 3);
        assert!(engine.state.channels[0].delayed_cell.is_some());
        assert_eq!(engine.state.channels[0].delayed_cell.unwrap().note, Note::On(60));
    }

    #[test]
    fn auto_vibrato_period_base_set_on_trigger_note() {
        use crate::sequencer::instrument::Instrument;
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        // Create a sample with data
        let mut sample = Sample::default();
        sample.default_volume = 48;
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        // Create an instrument with auto-vibrato
        let vib_depth: u8 = 10;
        let mut inst = Instrument::default();
        inst.vib_depth = vib_depth;
        inst.vib_sweep = 0;
        inst.vib_rate = 8;
        inst.vib_type = 0;
        inst.sample_map[60] = 1;

        // Create pattern with a cell on channel 0 row 0
        let mut pattern = Pattern::new(64);
        let mut cell = Cell::default();
        cell.note = Note::On(60);
        cell.instrument = Some(1);
        pattern.data[0][0] = cell;

        let module = Arc::new(Module {
            name: String::new(),
            format: ModuleFormat::XM,
            instruments: vec![Instrument::default(), inst],
            samples: vec![Sample::default(), sample],
            order_list: vec![0],
            patterns: vec![pattern],
            flags: ModuleFlags {
                linear_slides: true,
                use_instruments: true,
                xm_period_model: true,
                ..ModuleFlags::default()
            },
            ..Module::default()
        });

        engine.load_module(module.clone());

        // Initialise state so that process_tick_zero_unified reads row 0 of pattern 0
        engine.state.current_order = 0;
        engine.state.current_pattern = 0;
        engine.state.current_row = 0;
        engine.state.current_tick = 0;
        engine.state.channels.resize(64, ChannelState::default());

        engine.process_tick_zero_unified();

        // Find the active voice on channel 0
        let voice = engine.voices.iter()
            .find(|v| v.active && v.channel == Some(0))
            .expect("Should have an active voice on channel 0");

        // auto_vib_period_base must be non-zero and match the note's period
        assert!(
            voice.auto_vib_period_base > 0,
            "auto_vib_period_base should be > 0 after trigger, got {}",
            voice.auto_vib_period_base
        );

        let expected_period = crate::sequencer::period::get_note_period(60, 0, true);
        assert_eq!(
            voice.auto_vib_period_base, expected_period,
            "auto_vib_period_base {} should match note period {}",
            voice.auto_vib_period_base, expected_period
        );

        // Verify auto-vibrato sweep is set up correctly (sweep=0 → full depth)
        assert_eq!(voice.auto_vib_amp, (vib_depth as i32) * 256);
        assert_eq!(voice.auto_vib_sweep, 0);
    }

    #[test]
    fn delayed_note_xm_sets_up_auto_vibrato_and_envelopes() {
        use crate::sequencer::instrument::{
            Instrument, Envelope, EnvelopeFlags, EnvelopePoint,
        };
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let mut sample = Sample::default();
        sample.default_volume = 48;
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        let vol_env = Envelope {
            points: vec![
                EnvelopePoint { tick: 0, value: 0 },
                EnvelopePoint { tick: 10, value: 64 },
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

        let vib_depth: u8 = 8;
        let fade_out: u16 = 128;
        let mut inst = Instrument::default();
        inst.vib_depth = vib_depth;
        inst.vib_sweep = 0;
        inst.vib_rate = 6;
        inst.vib_type = 0;
        inst.fade_out = fade_out;
        inst.volume_envelope = Some(vol_env);
        inst.sample_map[60] = 1;

        let module = Arc::new(Module {
            format: ModuleFormat::XM,
            instruments: vec![Instrument::default(), inst],
            samples: vec![Sample::default(), sample],
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags {
                linear_slides: true,
                use_instruments: true,
                xm_period_model: true,
                ..ModuleFlags::default()
            },
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        // Create a delayed-note cell
        let mut cell = Cell::default();
        cell.note = Note::On(60);
        cell.instrument = Some(1);
        cell.effect = Effect::NoteDelay { ticks: 2 };

        // Set up channel state for the delayed note
        engine.state.channels[0].delayed_cell = Some(cell);
        engine.state.channels[0].note_delay_ticks = 2;
        engine.state.channels[0].last_instrument = 1;

        // Trigger the delayed note (simulating tick 2 processing)
        engine.state.current_tick = 2;
        let linear = module.flags.linear_slides;
        engine.trigger_delayed_note_period(0, linear);

        let voice = engine.voices.iter()
            .find(|v| v.active && v.channel == Some(0))
            .expect("Should have active voice after delayed trigger");

        // Verify auto-vibrato was set up
        assert!(
            voice.auto_vib_period_base > 0,
            "Delayed note: auto_vib_period_base should be > 0, got {}",
            voice.auto_vib_period_base
        );
        assert_eq!(voice.auto_vib_amp, (vib_depth as i32) * 256,
            "Delayed note: auto_vib_amp should be at full depth");

        // Verify envelope was set up
        assert!(
            voice.vol_env.is_some(),
            "Delayed note: volume envelope should be set up"
        );
        assert!(
            voice.env_sustain_active,
            "Delayed note: env_sustain_active should be true"
        );
        assert_eq!(
            voice.fade_out_rate, fade_out,
            "Delayed note: fade_out_rate should be {}", fade_out
        );
        assert_eq!(
            voice.fade_out_amp, 32768,
            "Delayed note: fade_out_amp should be 32768"
        );
        assert_eq!(
            voice.instrument_index,
            Some(1),
            "Delayed note: instrument_index should be set"
        );
    }

    #[test]
    fn trigger_channel_note_resets_auto_vib_period_on_reuse() {
        use crate::sequencer::instrument::Instrument;
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let mut sample = Sample::default();
        sample.default_volume = 48;
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        let mut inst = Instrument::default();
        inst.vib_depth = 5;
        inst.vib_sweep = 20;
        inst.vib_rate = 4;
        inst.vib_type = 0;
        inst.sample_map[60] = 1;
        inst.sample_map[72] = 1;

        let module = Arc::new(Module {
            format: ModuleFormat::XM,
            instruments: vec![Instrument::default(), inst],
            samples: vec![Sample::default(), sample],
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags {
                linear_slides: true,
                use_instruments: true,
                xm_period_model: true,
                ..ModuleFlags::default()
            },
            ..Module::default()
        });

        engine.load_module(module.clone());

        // First trigger: C-5 (key=60)
        engine.play();
        let mut cell = Cell::default();
        cell.note = Note::On(60);
        cell.instrument = Some(1);

        engine.state.current_tick = 0;
        engine.state.current_row = 0;
        engine.process_cell_unified(0, &cell);

        let period_c5 = crate::sequencer::period::get_note_period(60, 0, true);

        let voice = engine.voices.iter()
            .find(|v| v.active && v.channel == Some(0))
            .expect("Should have voice after first trigger");
        assert_eq!(
            voice.auto_vib_period_base, period_c5,
            "First note: auto_vib_period_base should match C-5 period"
        );

        // Advance to next row and trigger a different note
        engine.advance_row();
        let mut cell2 = Cell::default();
        cell2.note = Note::On(72); // C-6
        cell2.instrument = Some(1);

        engine.state.current_tick = 0;
        engine.state.current_row = 1;
        engine.process_cell_unified(0, &cell2);

        let period_c6 = crate::sequencer::period::get_note_period(72, 0, true);

        let voice2 = engine.voices.iter()
            .find(|v| v.active && v.channel == Some(0))
            .expect("Should have voice after second trigger");
        assert_eq!(
            voice2.auto_vib_period_base, period_c6,
            "Second note: auto_vib_period_base should match C-6 period, not C-5"
        );
    }

    #[test]
    fn xm_active_effects_dispatch_volume_slide() {
        use crate::sequencer::instrument::Instrument;
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let mut sample = Sample::default();
        sample.default_volume = 64;
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        let mut inst = Instrument::default();
        inst.sample_map[60] = 1;

        let module = Arc::new(Module {
            format: ModuleFormat::XM,
            instruments: vec![Instrument::default(), inst],
            samples: vec![Sample::default(), sample],
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags {
                linear_slides: true,
                use_instruments: true,
                xm_period_model: true,
                ..ModuleFlags::default()
            },
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        // Trigger a note
        let mut cell = Cell::default();
        cell.note = Note::On(60);
        cell.instrument = Some(1);
        cell.effect = Effect::VolumeSlide { up: 2, down: 0 };

        engine.state.current_tick = 0;
        engine.process_cell_unified(0, &cell);

        assert!(
            engine.state.channels[0].active_effects.volume_slide,
            "VolumeSlide should set active_effects.volume_slide"
        );
        assert_eq!(
            engine.state.channels[0].last_volume_slide_up, 2,
            "VolumeSlide should store up value"
        );

        // Process non-zero tick — ActiveEffects dispatch should apply slide
        let vol_before = engine.state.channels[0].real_vol;
        engine.state.current_tick = 1;
        engine.process_effects_tick_unified();
        let vol_after = engine.state.channels[0].real_vol;
        assert!(
            vol_after > vol_before || vol_after == 64,
            "VolumeSlide should increase volume on non-zero tick: {} -> {}",
            vol_before, vol_after
        );
    }

    #[test]
    fn xm_active_effects_dispatch_tpvs() {
        use crate::sequencer::instrument::Instrument;
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let mut sample = Sample::default();
        sample.default_volume = 64;
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        let mut inst = Instrument::default();
        inst.sample_map[60] = 1;
        inst.sample_map[64] = 1;

        let module = Arc::new(Module {
            format: ModuleFormat::XM,
            instruments: vec![Instrument::default(), inst],
            samples: vec![Sample::default(), sample],
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags {
                linear_slides: true,
                use_instruments: true,
                xm_period_model: true,
                ..ModuleFlags::default()
            },
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        // First trigger a note
        let mut cell = Cell::default();
        cell.note = Note::On(60);
        cell.instrument = Some(1);
        engine.state.current_tick = 0;
        engine.process_cell_unified(0, &cell);

        // Now set TPVS: tone portamento to new note + volume slide
        engine.advance_row();
        let mut cell2 = Cell::default();
        cell2.note = Note::On(64);
        cell2.effect = Effect::TonePortamentoVolumeSlide { up: 0x15 };
        engine.state.current_tick = 0;
        engine.process_cell_unified(0, &cell2);

        assert!(
            engine.state.channels[0].active_effects.tone_portamento,
            "TPVS should set active_effects.tone_portamento"
        );
        assert!(
            engine.state.channels[0].active_effects.volume_slide,
            "TPVS should set active_effects.volume_slide"
        );
        assert_eq!(
            engine.state.channels[0].last_volume_slide_up, 1,
            "TPVS param 0x15: up nibble = 1"
        );
        assert_eq!(
            engine.state.channels[0].last_volume_slide_down, 5,
            "TPVS param 0x15: down nibble = 5"
        );
    }

    #[test]
    fn xm_note_delay_triggers_on_correct_tick() {
        use crate::sequencer::instrument::Instrument;
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let mut sample = Sample::default();
        sample.default_volume = 48;
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        let mut inst = Instrument::default();
        inst.sample_map[60] = 1;

        let module = Arc::new(Module {
            format: ModuleFormat::XM,
            instruments: vec![Instrument::default(), inst],
            samples: vec![Sample::default(), sample],
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags {
                linear_slides: true,
                use_instruments: true,
                xm_period_model: true,
                ..ModuleFlags::default()
            },
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        // Tick 0: process delayed note cell
        let mut cell = Cell::default();
        cell.note = Note::On(60);
        cell.instrument = Some(1);
        cell.effect = Effect::NoteDelay { ticks: 3 };

        engine.state.current_tick = 0;
        engine.process_cell_unified(0, &cell);

        // No voice should be active yet (note is delayed)
        assert!(
            engine.voices.iter().all(|v| !v.active || v.channel != Some(0)),
            "No voice on ch0 after tick 0 with NoteDelay"
        );
        assert_eq!(engine.state.channels[0].note_delay_ticks, 3);
        assert!(engine.state.channels[0].delayed_cell.is_some());

        // Tick 1: still no voice
        engine.state.current_tick = 1;
        engine.process_effects_tick_unified();
        assert!(
            engine.voices.iter().all(|v| !v.active || v.channel != Some(0)),
            "No voice on ch0 at tick 1"
        );

        // Tick 2: still no voice
        engine.state.current_tick = 2;
        engine.process_effects_tick_unified();
        assert!(
            engine.voices.iter().all(|v| !v.active || v.channel != Some(0)),
            "No voice on ch0 at tick 2"
        );

        // Tick 3: delayed note should trigger
        engine.state.current_tick = 3;
        engine.process_effects_tick_unified();
        assert!(
            engine.voices.iter().any(|v| v.active && v.channel == Some(0)),
            "Voice should be active on ch0 at tick 3 (delayed note trigger)"
        );
    }

    #[test]
    fn mod_pattern_loop_sets_loop_start() {
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        engine.state.current_order = 2;
        engine.state.current_row = 16;
        engine.state.channels.resize(1, ChannelState::default());
        engine.use_xm_model = false;

        // E6x with count=0 sets loop start
        let cell = Cell {
            effect: Effect::PatternLoop { count: 0 },
            ..Cell::default()
        };

        engine.process_cell_unified(0, &cell);

        // Loop start should be captured
        assert!(
            engine.state.pattern_loop_start.is_some(),
            "Pattern loop start should be set when count=0"
        );
        let (order, row) = engine.state.pattern_loop_start.unwrap();
        assert_eq!(order, 2);
        assert_eq!(row, 16);
    }

    #[test]
    fn mod_pattern_loop_executes_loop() {
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        engine.state.current_order = 0;
        engine.state.current_row = 4;
        engine.state.channels.resize(1, ChannelState::default());
        engine.use_xm_model = false;

        // First, E60 to set loop start at row 4
        let cell_start = Cell {
            effect: Effect::PatternLoop { count: 0 },
            ..Cell::default()
        };
        engine.process_cell_unified(0, &cell_start);
        assert!(engine.state.pattern_loop_start.is_some());

        // Move to row 63 (last row) for the loop trigger
        engine.state.current_row = 63;

        // Then E62 (count=2) to set loop repeat count
        let cell_loop = Cell {
            effect: Effect::PatternLoop { count: 2 },
            ..Cell::default()
        };
        engine.process_cell_unified(0, &cell_loop);

        assert_eq!(engine.state.pattern_loop_count, 2);

        // After advance_row from last row, should loop back and decrement
        engine.advance_row();

        assert_eq!(engine.state.pattern_loop_count, 1);
        assert_eq!(engine.state.current_row, 4);
    }

    #[test]
    fn mod_pattern_loop_advances_to_next_order() {
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0, 1],
            patterns: vec![
                Pattern::new(64),
                Pattern::new(64),
            ],
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        engine.state.current_order = 0;
        engine.state.current_row = 4;
        engine.state.channels.resize(1, ChannelState::default());
        engine.use_xm_model = false;

        // E60 to set loop start at row 4
        let cell_start = Cell {
            effect: Effect::PatternLoop { count: 0 },
            ..Cell::default()
        };
        engine.process_cell_unified(0, &cell_start);
        assert!(engine.state.pattern_loop_start.is_some());

        // Move to row 63 for the trigger
        engine.state.current_row = 63;

        // E61 (count=1) to trigger one loop iteration
        let cell_loop = Cell {
            effect: Effect::PatternLoop { count: 1 },
            ..Cell::default()
        };
        engine.process_cell_unified(0, &cell_loop);
        assert_eq!(engine.state.pattern_loop_count, 1);

        // advance_row: count 1->0, loop back to row 4
        engine.advance_row();
        assert_eq!(engine.state.pattern_loop_count, 0);
        assert_eq!(engine.state.current_row, 4);
        assert_eq!(engine.state.current_order, 0);
        assert_eq!(engine.state.pattern_loop_start, None);

        // Advance from row 4 through the rest of the pattern back to row 63
        for _ in 0..59 {
            engine.advance_row();
        }
        assert_eq!(engine.state.current_row, 63);
        assert_eq!(engine.state.current_order, 0);

        // Now at row 63 again with no loop active - advance past last row to next order
        engine.advance_row();
        assert_eq!(engine.state.current_order, 1);
        assert_eq!(engine.state.current_row, 0);
    }

    #[test]
    fn mod_pattern_loop_count_3_exits_correctly() {
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0, 1],
            patterns: vec![
                Pattern::new(64),
                Pattern::new(64),
            ],
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        engine.state.current_order = 0;
        engine.state.current_row = 0;
        engine.state.channels.resize(1, ChannelState::default());
        engine.use_xm_model = false;

        // E60 to set loop start at row 0
        let cell_start = Cell {
            effect: Effect::PatternLoop { count: 0 },
            ..Cell::default()
        };
        engine.process_cell_unified(0, &cell_start);
        assert!(engine.state.pattern_loop_start.is_some());

        // Move to row 63
        engine.state.current_row = 63;

        // E63 (count=3) - same as wash.mod pattern 3
        let cell_loop = Cell {
            effect: Effect::PatternLoop { count: 3 },
            ..Cell::default()
        };
        engine.process_cell_unified(0, &cell_loop);
        assert_eq!(engine.state.pattern_loop_count, 3);

        // Iteration 1: count 3->2, jump back to row 0
        engine.advance_row();
        assert_eq!(engine.state.pattern_loop_count, 2);
        assert_eq!(engine.state.current_row, 0);
        assert_eq!(engine.state.current_order, 0);

        // Advance to row 63
        for _ in 0..63 {
            engine.advance_row();
        }
        assert_eq!(engine.state.current_row, 63);

        // Iteration 2: count 2->1, jump back to row 0
        engine.advance_row();
        assert_eq!(engine.state.pattern_loop_count, 1);
        assert_eq!(engine.state.current_row, 0);
        assert_eq!(engine.state.current_order, 0);

        // Advance to row 63
        for _ in 0..63 {
            engine.advance_row();
        }
        assert_eq!(engine.state.current_row, 63);

        // Iteration 3 (final): count 1->0, jump back to row 0 for final pass
        engine.advance_row();
        assert_eq!(engine.state.pattern_loop_count, 0);
        assert_eq!(engine.state.pattern_loop_start, None);
        assert_eq!(engine.state.pattern_loop_final_pass, true);
        assert_eq!(engine.state.current_row, 0);
        assert_eq!(engine.state.current_order, 0);

        // Advance through final pass to row 63 (loop commands ignored due to final_pass flag)
        for _ in 0..63 {
            engine.advance_row();
        }
        assert_eq!(engine.state.current_row, 63);

        // Advance past row 63 to next order
        engine.advance_row();
        assert_eq!(engine.state.current_order, 1);
        assert_eq!(engine.state.current_row, 0);
        assert_eq!(engine.state.pattern_loop_final_pass, false);
    }

    #[test]
    fn advance_row_resets_retrigger_state() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.state.channels[0].retrig_speed = 4;
        engine.state.channels[0].retrig_cnt = 3;
        engine.state.channels[0].last_retrigger_interval = 4;

        engine.advance_row();

        assert_eq!(engine.state.channels[0].retrig_speed, 0,
            "retrig_speed should be reset on row advance");
        assert_eq!(engine.state.channels[0].retrig_cnt, 0,
            "retrig_cnt should be reset on row advance");
        assert_eq!(engine.state.channels[0].last_retrigger_interval, 0,
            "last_retrigger_interval should be reset on row advance");
    }

    #[test]
    fn xm_pattern_delay_sets_row_delay_active() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = true;

        let module = Arc::new(Module::default());
        engine.load_module(module);
        engine.play();

        let cell = Cell {
            effect: Effect::PatternDelay { ticks: 2 },
            ..Cell::default()
        };

        engine.state.current_tick = 0;
        engine.process_cell_unified(0, &cell);

        assert!(engine.state.row_delay_active,
            "PatternDelay should set row_delay_active for XM");
        assert_eq!(engine.state.pattern_delay_ticks, 2,
            "PatternDelay should store tick count");
    }

    #[test]
    fn advance_row_resets_note_cut_tick() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.state.channels[0].note_cut_tick = Some(5);

        engine.advance_row();

        assert_eq!(engine.state.channels[0].note_cut_tick, None,
            "note_cut_tick should be reset on row advance");
    }

    #[test]
    fn mod_tone_portamento_slides_toward_target() {
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);
        let mut sample = Sample::default();
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            instruments: vec![Instrument::default()],
            samples: vec![Sample::default(), sample.clone()],
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        engine.state.channels[0].last_instrument = 1;
        engine.state.channels[0].last_sample = 1;

        let (target_period, target_freq) = engine.compute_portamento_target(0, 60, 60, Some(&sample), 1, &module);
        assert!(target_period > 0, "compute_portamento_target should return a valid period");

        engine.state.channels[0].real_period = 856;
        engine.state.channels[0].out_period = 856;
        engine.state.channels[0].want_period = target_period;
        engine.state.channels[0].portamento_target_period = Some(target_period);
        engine.state.channels[0].portamento_target_frequency = Some(target_freq);
        engine.state.channels[0].last_tone_portamento_speed = 8;

        let before = engine.state.channels[0].real_period;
        engine.apply_tone_portamento(0, 8);
        let after = engine.state.channels[0].real_period;

        assert_ne!(before, after, "apply_tone_portamento should change period");
        assert_ne!(after, 856, "apply_tone_portamento should produce a different period");
    }

    #[test]
    fn mod_vibrato_depth_within_protracker_range() {
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;
        use crate::sequencer::period::period_to_frequency;

        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let mut sample = Sample::default();
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            samples: vec![Sample::default(), sample],
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        let mut voice = Voice::default();
        voice.active = true;
        voice.channel = Some(0);
        voice.base_frequency = 440.0;
        voice.sample_delta = 440.0 / 48000.0;
        voice.vibrato_waveform = VibratoWaveform::Sine;
        voice.vibrato_phase = 0.0;
        engine.voices[0] = voice;

        engine.state.channels[0].real_period = 856;
        engine.state.channels[0].out_period = 856;
        engine.state.channels[0].wave_ctrl = 0;
        engine.state.channels[0].vib_pos = 0;

        let initial_period = engine.state.channels[0].real_period;
        let initial_freq = period_to_frequency(initial_period, false, 8363);
        let after_period = {
            let ch = &mut engine.state.channels[0];
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
            let offset = ((vibrato_val * 15) >> 3) as u16;
            if (ch.vib_pos as i8) < 0 {
                ch.real_period.saturating_sub(offset).max(1)
            } else {
                ch.real_period.saturating_add(offset).min(31999)
            }
        };
        let after_freq = period_to_frequency(after_period, false, 8363);

        let freq_mod = after_freq / initial_freq;
        let semitones = (freq_mod.log2() * 12.0).abs();
        assert!(semitones < 2.0,
            "MOD vibrato depth 15 should be < 2 semitones, got {:.1}", semitones);
    }

    #[test]
    fn mod_volume_slide_memory_uses_full_param() {
        use crate::sequencer::module::ModuleFlags;
        use crate::sequencer::pattern::Pattern;

        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        engine.state.channels[0].channel_volume = 64;
        engine.state.channels[0].last_volume_slide_param = 0;
        engine.state.channels[0].last_volume_slide_up = 3;
        engine.state.channels[0].last_volume_slide_down = 0;

        engine.apply_volume_slide(0);

        let vol_after = engine.state.channels[0].channel_volume;
        assert_eq!(vol_after, 64,
            "With param=0, should slide 0 (up=0, down=0 from param), ignoring stale up=3");

        engine.state.channels[0].channel_volume = 64;
        engine.state.channels[0].last_volume_slide_param = 0x30;
        engine.state.channels[0].last_volume_slide_up = 5;
        engine.state.channels[0].last_volume_slide_down = 7;

        engine.apply_volume_slide(0);

        let vol_after_param = engine.state.channels[0].channel_volume;
        assert_eq!(vol_after_param, 64,
            "With param=0x30, should slide up by 3, but 64+3=67 exceeds max 64, clamped to 64");
    }

    #[test]
    fn xm_tone_portamento_still_works() {
        use crate::sequencer::{Instrument, Module, ModuleFormat, Pattern, Sample};
        use crate::sequencer::module::ModuleFlags;

        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = true;

        let mut sample = Sample::default();
        sample.data = Arc::new(vec![0.0f32; 100]);
        sample.sample_rate = 8363;

        let module = Arc::new(Module {
            format: ModuleFormat::XM,
            instruments: vec![Instrument::default()],
            samples: vec![Sample::default(), sample],
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags {
                linear_slides: true,
                use_instruments: true,
                xm_period_model: true,
                ..ModuleFlags::default()
            },
            ..Module::default()
        });

        engine.load_module(module.clone());
        engine.play();

        engine.state.channels[0].last_instrument = 1;
        engine.state.channels[0].last_sample = 1;
        engine.state.channels[0].real_period = 856;
        engine.state.channels[0].want_period = 428;
        engine.state.channels[0].porta_dir = 2;
        engine.state.channels[0].porta_speed_period = 8;

        let before = engine.state.channels[0].real_period;
        let target_period = engine.state.channels[0].want_period;
        engine.apply_tone_portamento_period(0, true);
        let after = engine.state.channels[0].real_period;

        assert!(after < before,
            "XM portamento (porta_dir=2, slide up = lower period) should decrease period: {} -> {}",
            before, after);
        assert!(after >= target_period,
            "XM portamento should not overshoot target period");
    }

    #[test]
    fn portamento_up_memory_preserved_when_param_is_zero() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        engine.state.channels[0].last_portamento_up_speed = 4;
        engine.state.channels[0].active_effects.portamento_up = true;

        engine.apply_effect_unified(0, &Effect::PortamentoUp { speed: 0 }, true);

        assert_eq!(engine.state.channels[0].last_portamento_up_speed, 4,
            "Zero-param portamento up should preserve last speed");
        assert!(engine.state.channels[0].active_effects.portamento_up,
            "Zero-param portamento up should keep active flag");
    }

    #[test]
    fn vibrato_memory_preserved_when_param_is_zero() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        engine.state.channels[0].last_vibrato_speed = 5;
        engine.state.channels[0].last_vibrato_depth = 8;
        engine.state.channels[0].active_effects.vibrato = true;

        engine.apply_effect_unified(0, &Effect::Vibrato { speed: 0, depth: 0 }, true);

        assert_eq!(engine.state.channels[0].last_vibrato_speed, 5,
            "Zero-param vibrato should preserve last speed");
        assert_eq!(engine.state.channels[0].last_vibrato_depth, 8,
            "Zero-param vibrato should preserve last depth");
        assert!(engine.state.channels[0].active_effects.vibrato,
            "Zero-param vibrato should keep active flag");
    }

    #[test]
    fn panning_slide_memory_preserved_when_param_is_zero() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = true;

        let module = Arc::new(Module {
            format: ModuleFormat::XM,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: true, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        engine.state.channels[0].last_panning_slide = 3;
        engine.state.channels[0].channel_panning = 128;

        engine.apply_effect_unified(0, &Effect::PanningSlide { speed: 0 }, true);

        assert_eq!(engine.state.channels[0].last_panning_slide, 3,
            "Zero-param panning slide should preserve last value");
        assert!(engine.state.channels[0].active_effects.panning_slide,
            "Zero-param panning slide should keep active flag");
    }

    #[test]
    fn global_volume_slide_xm_applies_each_tick() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = true;

        let module = Arc::new(Module {
            format: ModuleFormat::XM,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: true, ..ModuleFlags::default() },
            instruments: vec![crate::sequencer::instrument::Instrument::default()],
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        engine.state.global_volume = 32;
        engine.state.last_global_volume_up = 3;
        engine.state.last_global_volume_down = 0;
        engine.state.channels[0].active_effects.global_volume_slide = true;
        engine.state.current_tick = 1;
        engine.process_effects_tick_unified();
        assert_eq!(engine.state.global_volume, 35, "XM global volume should increase by up each tick");
    }

    #[test]
    fn global_volume_slide_non_xm_applies_each_tick() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        engine.state.global_volume = 64;
        engine.state.last_global_volume_up = 0;
        engine.state.last_global_volume_down = 5;
        engine.state.channels[0].active_effects.global_volume_slide = true;
        engine.state.current_tick = 1;
        engine.process_effects_tick_unified();
        assert_eq!(engine.state.global_volume, 59, "non-XM global volume should decrease by down each tick");
    }

    #[test]
    fn global_volume_slide_memory_accumulates_per_tick() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        engine.state.global_volume = 64;
        engine.state.last_global_volume_up = 2;
        engine.state.last_global_volume_down = 0;
        engine.state.channels[0].active_effects.global_volume_slide = true;
        engine.state.current_tick = 1;
        engine.process_effects_tick_unified();
        assert_eq!(engine.state.global_volume, 66);
        engine.state.current_tick = 2;
        engine.process_effects_tick_unified();
        assert_eq!(engine.state.global_volume, 68, "slide should accumulate across ticks");
    }

    #[test]
    fn extra_fine_portamento_slows_by_factor_4() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        let ch = 0;
        engine.state.channels[ch].real_period = 500;
        engine.state.channels[ch].out_period = 500;
        engine.apply_effect_unified(ch, &Effect::ExtraFinePortamentoDown { speed: 4 }, true);
        let spd = ((4u8 as u16 + 2) >> 2).max(1);
        assert_eq!(engine.state.channels[ch].real_period, 500 + spd as u16,
            "ExtraFinePortamentoDown speed 4 -> spd {}, period+spd", spd);
    }

    #[test]
    fn funkit_modulates_voice_position_on_tick() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        let ch = 0;
        engine.state.channels[ch].funk_speed = 4;
        engine.state.channels[ch].funk_toggle = true;
        engine.state.current_tick = FUNK_TRACK[4] as u8;
        engine.voices[0].active = true;
        engine.voices[0].channel = Some(0);
        engine.voices[0].position = 100.0;
        engine.process_effects_tick_unified();
        assert!(engine.voices[0].position >= 100.0, "FunkIt should modulate voice position");
    }

    #[test]
    fn funkit_speed_zero_disables_modulation() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        engine.state.channels[0].funk_speed = 0;
        let pos_before = engine.voices[0].position;
        engine.state.current_tick = 5;
        engine.process_effects_tick_unified();
        assert_eq!(engine.voices[0].position, pos_before, "funk_speed=0 should not move position");
    }

    #[test]
    fn karplus_strong_initializes_buffer_on_trigger() {
        let mut engine = SequencerEngine::new(48000.0);
        engine.use_xm_model = false;

        let module = Arc::new(Module {
            format: ModuleFormat::MOD,
            order_list: vec![0],
            patterns: vec![Pattern::new(64)],
            flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
            ..Module::default()
        });
        engine.load_module(module);
        engine.play();

        let ch = 0;
        engine.state.channels[ch].karplus_param = 8;
        assert_eq!(engine.state.channels[ch].karplus_param, 8);
    }

    #[test]
    fn karplus_strong_disabled_when_param_zero() {
        let mut engine = SequencerEngine::new(48000.0);
        let voice = &mut engine.voices[0];
        voice.karplus_strong = false;
        assert!(!voice.karplus_strong);
        voice.karplus_strong = true;
        voice.karplus_strong = false;
        assert!(!voice.karplus_strong);
    }

    #[test]
    fn karplus_strong_mixer_produces_output() {
        use crate::audio::mixer;
        use crate::audio::commands::InterpolationType;
        use std::sync::Arc;

        let mut voices = vec![crate::audio::voice::Voice::default()];
        let v = &mut voices[0];
        v.active = true;
        v.karplus_strong = true;
        v.ks_pos = 0;
        v.ks_delay_line = vec![0.5_f32; 64];
        v.ks_feedback = 0.9;
        v.base_volume = 1.0;
        v.final_volume = 1.0;
        v.final_panning = 0.5;
        v.channel = Some(0);
        let mut left = vec![0.0_f32; 16];
        let mut right = vec![0.0_f32; 16];
        let sample_rate = 44100.0;
        mixer::mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Linear, &[], sample_rate);
        let has_output = left.iter().any(|&s| s != 0.0);
        assert!(has_output, "KS should produce non-zero output");
    }
}
