use std::sync::Arc;

use super::compute_samples_per_tick;
use crate::audio::filter::StateVariableFilter;
use crate::audio::voice::EnvelopeState;
use crate::sequencer::effect::{Effect, FormatEffect, XmEffect, S3mEffect, ItEffect, FilterType, NUM_SEND_BUSES};
use crate::sequencer::instrument::{DuplicateCheckAction, DuplicateCheckType, NewNoteAction};
use crate::sequencer::note::Note;
use crate::sequencer::period::{get_note_period, period_to_frequency};
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::{LoopType, Sample};

pub struct XmProcessor;

impl XmProcessor {
    pub fn new() -> Self {
        XmProcessor
    }

    pub fn apply_effect(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize, effect: &Effect, is_row_start: bool) {
        let ch = &mut engine.state.channels[channel];

        match effect {
            Effect::None => {}

            Effect::SetSpeed { speed } => {
                if *speed > 0 {
                    engine.state.speed = *speed;
                }
            }
            Effect::SetTempo { bpm } => {
                if *bpm >= 32 {
                    engine.state.bpm = *bpm as u16;
                    engine.state.samples_per_tick =
                        compute_samples_per_tick(engine.state.bpm, engine.output_sample_rate);
                } else if *bpm > 0 {
                    engine.state.speed = *bpm;
                }
            }

            Effect::SetVolume { volume } | Effect::VolSetVolume { vol: volume } => {
                ch.channel_volume = (*volume).min(64);
                ch.row_volume = (*volume).min(64);
                ch.real_vol = (*volume).min(64);
                let vol = engine.compute_channel_volume(channel);
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = vol;
                        voice.channel_volume = 1.0;
                    }
                }
            }

            Effect::SetPanning { pan } | Effect::SetPanning16 { pan } => {
                ch.channel_panning = (*pan).min(255);
            }

            Effect::SetPanPosition { pan } => {
                ch.channel_panning = (*pan).min(255);
            }

            Effect::SetSampleOffset { offset } => {
                if *offset > 0 {
                    ch.last_sample_offset = *offset;
                }
            }

            Effect::FormatSpecific(fe) => {
                match fe {
                    FormatEffect::Xm(XmEffect::SetSampleOffset(offset))
                    | FormatEffect::S3m(S3mEffect::SetSampleOffset(offset))
                    | FormatEffect::It(ItEffect::SetSampleOffset(offset)) => {
                        if *offset > 0 {
                            ch.last_sample_offset = *offset;
                        }
                    }
                    FormatEffect::Xm(XmEffect::KeyOff { .. }) => {
                        ch.active_effects.key_off = true;
                    }
                    _ => {}
                }
            }

            Effect::PositionJump { order } => {
                engine.state.position_jump_order = Some(*order);
            }

            Effect::PatternBreak { row } => {
                engine.state.position_jump_flag = true;
                engine.state.pattern_break_row = Some(*row);
            }

            Effect::TonePortamento { speed } => {
                if *speed > 0 {
                    ch.porta_speed_period = (*speed as u16) << 2;
                }
                if let Note::On(key) = ch.last_note {
                    let module = engine.module.as_ref().unwrap().clone();
                    let period = get_note_period(
                        key.saturating_add(ch.rel_ton as u8),
                        ch.fine_tune_offset,
                        module.flags.linear_slides,
                    );
                    ch.want_period = period;
                    if period == ch.real_period {
                        ch.porta_dir = 0;
                    } else if period > ch.real_period {
                        ch.porta_dir = 1;
                    } else {
                        ch.porta_dir = 2;
                    }
                }
                ch.active_effects.tone_portamento = true;
            }

            Effect::PortamentoUp { speed } => {
                if *speed > 0 {
                    ch.last_portamento_up_speed = *speed;
                }
                ch.active_effects.portamento_up = true;
                if is_row_start {
                    if *speed > 0 {
                        let spd = (*speed as u16) << 2;
                        ch.real_period = ch.real_period.saturating_sub(spd).max(1);
                        ch.out_period = ch.real_period;
                        let out = ch.out_period;
                        let module = engine.module.as_ref().unwrap().clone();
                        let freq = period_to_frequency(out, module.flags.linear_slides, 8363);
                        let delta = if engine.output_sample_rate > 0.0 { freq / engine.output_sample_rate } else { 0.0 };
                        for voice in &mut engine.voices {
                            if voice.active && voice.channel == Some(channel) {
                                voice.current_frequency = freq;
                                voice.sample_delta = delta;
                            }
                        }
                    }
                }
            }

            Effect::PortamentoDown { speed } => {
                if *speed > 0 {
                    ch.last_portamento_down_speed = *speed;
                }
                ch.active_effects.portamento_down = true;
                if is_row_start {
                    if *speed > 0 {
                        let spd = (*speed as u16) << 2;
                        ch.real_period = ch.real_period.saturating_add(spd).min(31999);
                        ch.out_period = ch.real_period;
                        let out = ch.out_period;
                        let module = engine.module.as_ref().unwrap().clone();
                        let freq = period_to_frequency(out, module.flags.linear_slides, 8363);
                        let delta = if engine.output_sample_rate > 0.0 { freq / engine.output_sample_rate } else { 0.0 };
                        for voice in &mut engine.voices {
                            if voice.active && voice.channel == Some(channel) {
                                voice.current_frequency = freq;
                                voice.sample_delta = delta;
                            }
                        }
                    }
                }
            }

            Effect::ExtraFinePortamentoUp { speed } => {
                if is_row_start && *speed > 0 {
                    let spd = ((*speed as u16 + 2) >> 2).max(1);
                    ch.real_period = ch.real_period.saturating_sub(spd).max(1);
                    ch.out_period = ch.real_period;
                    let out = ch.out_period;
                    let module = engine.module.as_ref().unwrap().clone();
                    let freq = period_to_frequency(out, module.flags.linear_slides, 8363);
                    let delta = if engine.output_sample_rate > 0.0 { freq / engine.output_sample_rate } else { 0.0 };
                    for voice in &mut engine.voices {
                        if voice.active && voice.channel == Some(channel) {
                            voice.current_frequency = freq;
                            voice.sample_delta = delta;
                        }
                    }
                }
            }

            Effect::ExtraFinePortamentoDown { speed } => {
                if is_row_start && *speed > 0 {
                    let spd = ((*speed as u16 + 2) >> 2).max(1);
                    ch.real_period = ch.real_period.saturating_add(spd).min(31999);
                    ch.out_period = ch.real_period;
                    let out = ch.out_period;
                    let module = engine.module.as_ref().unwrap().clone();
                    let freq = period_to_frequency(out, module.flags.linear_slides, 8363);
                    let delta = if engine.output_sample_rate > 0.0 { freq / engine.output_sample_rate } else { 0.0 };
                    for voice in &mut engine.voices {
                        if voice.active && voice.channel == Some(channel) {
                            voice.current_frequency = freq;
                            voice.sample_delta = delta;
                        }
                    }
                }
            }

            Effect::FinePortamentoUp { speed } => {
                if is_row_start {
                    if *speed > 0 {
                        let spd = *speed as u16;
                        ch.real_period = ch.real_period.saturating_sub(spd).max(1);
                        ch.out_period = ch.real_period;
                        let out = ch.out_period;
                        let module = engine.module.as_ref().unwrap().clone();
                        let freq = period_to_frequency(out, module.flags.linear_slides, 8363);
                        let delta = if engine.output_sample_rate > 0.0 { freq / engine.output_sample_rate } else { 0.0 };
                        for voice in &mut engine.voices {
                            if voice.active && voice.channel == Some(channel) {
                                voice.current_frequency = freq;
                                voice.sample_delta = delta;
                            }
                        }
                    }
                }
            }

            Effect::FinePortamentoDown { speed } => {
                if is_row_start {
                    if *speed > 0 {
                        let spd = *speed as u16;
                        ch.real_period = ch.real_period.saturating_add(spd).min(31999);
                        ch.out_period = ch.real_period;
                        let out = ch.out_period;
                        let module = engine.module.as_ref().unwrap().clone();
                        let freq = period_to_frequency(out, module.flags.linear_slides, 8363);
                        let delta = if engine.output_sample_rate > 0.0 { freq / engine.output_sample_rate } else { 0.0 };
                        for voice in &mut engine.voices {
                            if voice.active && voice.channel == Some(channel) {
                                voice.current_frequency = freq;
                                voice.sample_delta = delta;
                            }
                        }
                    }
                }
            }

            Effect::Vibrato { speed, depth } => {
                if *speed > 0 { ch.vib_speed = *speed; }
                if *depth > 0 { ch.vib_depth = *depth; }
                ch.active_effects.vibrato = true;
            }

            Effect::Tremolo { speed, depth } => {
                if *speed > 0 { ch.trem_speed = *speed; }
                if *depth > 0 { ch.trem_depth = *depth; }
                ch.active_effects.tremolo = true;
            }

            Effect::VolumeSlide { up, down } => {
                if *up > 0 { ch.last_volume_slide_up = *up; }
                if *down > 0 { ch.last_volume_slide_down = *down; }
                ch.active_effects.volume_slide = true;
                if is_row_start {
                    engine.apply_volume_slide(channel);
                }
            }

            Effect::TonePortamentoVolumeSlide { up } => {
                let param = *up as u8;
                let up_val = param >> 4;
                let down_val = param & 0x0F;
                if up_val > 0 { ch.last_volume_slide_up = up_val; }
                if down_val > 0 { ch.last_volume_slide_down = down_val; }
                ch.active_effects.tone_portamento = true;
                ch.active_effects.volume_slide = true;
            }

            Effect::VibratoVolumeSlide { up } => {
                let param = *up as u8;
                let up_val = param >> 4;
                let down_val = param & 0x0F;
                if up_val > 0 { ch.last_volume_slide_up = up_val; }
                if down_val > 0 { ch.last_volume_slide_down = down_val; }
                ch.active_effects.vibrato = true;
                ch.active_effects.volume_slide = true;
            }

            Effect::Arpeggio { note1, note2 } => {
                if *note1 > 0 || *note2 > 0 {
                    ch.last_arpeggio = (*note1, *note2);
                }
                ch.active_effects.arpeggio = true;
            }

            Effect::PanningSlide { speed } => {
                if *speed != 0 {
                    ch.last_panning_slide = *speed;
                }
                ch.active_effects.panning_slide = true;
                if is_row_start {
                    engine.apply_panning_slide(channel);
                }
            }

            Effect::SetGlobalVolume { volume } => {
                engine.state.global_volume = (*volume).min(64);
                engine.global_volume = engine.state.global_volume as f32 / 64.0;
            }

            Effect::PatternDelay { ticks } => {
                if !engine.state.row_delay_active {
                    engine.state.pattern_delay_ticks = *ticks;
                    engine.state.row_delay_active = true;
                }
            }

            Effect::GlissandoControl { on } => {
                ch.glissando = *on;
            }

            Effect::VibratoWaveform { waveform } => {
                ch.wave_ctrl = (ch.wave_ctrl & 0xF0) | (waveform & 0x0F);
            }

            Effect::TremoloWaveform { waveform } => {
                ch.wave_ctrl = ((waveform & 0x0F) << 4) | (ch.wave_ctrl & 0x0F);
            }

            Effect::SetFineTune { tune } => {
                ch.fine_tune_offset = (((*tune & 0x0F) << 4) as u8).wrapping_sub(128) as i8;
            }

            Effect::SetEnvelopePosition { tick } => {
                let module = engine.module.as_ref().unwrap().clone();
                let inst_idx = ch.last_instrument as usize;
                if inst_idx > 0 && inst_idx < module.instruments.len() {
                    let inst = &module.instruments[inst_idx];
                    let tick_val = *tick as f32;
                    for voice in &mut engine.voices {
                        if !voice.active || voice.channel != Some(channel) {
                            continue;
                        }
                        if let Some(ref vol_env) = inst.volume_envelope {
                            if vol_env.flags.enabled {
                                if let Some(ref mut ve) = voice.vol_env {
                                    ve.position = tick_val - 1.0;
                                    ve.current_point = 0;
                                    for (i, pt) in vol_env.points.iter().enumerate() {
                                        if (pt.tick as f32) < tick_val {
                                            ve.current_point = (i + 1).min(vol_env.points.len() - 1);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Effect::Retrigger { interval } => {
                if *interval > 0 {
                    ch.last_retrigger_interval = *interval;
                    ch.retrig_speed = *interval;
                }
            }

            Effect::NoteCutAfter { ticks } => {
                if *ticks == 0 {
                    engine.cut_channel_voices(channel);
                } else {
                    engine.set_channel_cutoff_tick(channel, *ticks);
                }
            }

            Effect::Tremor { ontime, offtime } => {
                if *ontime > 0 {
                    ch.tremor_ontime = *ontime;
                }
                if *offtime > 0 {
                    ch.tremor_offtime = *offtime;
                }
                ch.tremor_counter = 0;
                ch.active_effects.tremor = true;
            }

            Effect::FineVolumeSlideUp { amount } => {
                ch.channel_volume = (ch.channel_volume + amount).min(64);
                ch.real_vol = ch.channel_volume;
                let vol = engine.compute_channel_volume(channel);
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = vol;
                    }
                }
            }

            Effect::FineVolumeSlideDown { amount } => {
                ch.channel_volume = ch.channel_volume.saturating_sub(*amount);
                ch.real_vol = ch.channel_volume;
                let vol = engine.compute_channel_volume(channel);
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = vol;
                    }
                }
            }

            Effect::NoteDelay { ticks: _ } => {
                // XM: handled in process_cell_unified (stores delayed_cell)
            }

            Effect::ExtendedEffect { .. } => {
                // Legacy-only (MOD EAx etc.), not used in XM
            }

            Effect::PatternLoop { .. } => {
                // XM doesn't support pattern loops, skip
            }

            Effect::Panbrello { speed, depth } => {
                if *speed > 0 { ch.last_panbrello_speed = *speed; }
                if *depth > 0 { ch.last_panbrello_depth = *depth; }
                ch.active_effects.panbrello = true;
            }

            Effect::GlobalVolumeSlide { .. } => {
                // XM doesn't use this legacy global volume slide path
            }

            Effect::VolFineSlideUp { .. } => {}
            Effect::VolFineSlideDown { .. } => {}
            Effect::VolSlideUp { .. } => {}
            Effect::VolSlideDown { .. } => {}
            Effect::VolPortamento { .. } => {}
            Effect::VolVibrato { .. } => {}

            Effect::SetFilterCutoff { cutoff } => {
                let cutoff_f = *cutoff as f32;
                engine.state.channels[channel].filter_cutoff = cutoff_f;
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.filter_cutoff = cutoff_f;
                    }
                }
            }

            Effect::SetFilterResonance { resonance } => {
                let res_f = *resonance as f32 / 128.0;
                engine.state.channels[channel].filter_resonance = res_f;
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.filter_resonance = res_f;
                    }
                }
            }

            Effect::SetFilterType { filter_type } => {
                let ft = FilterType::from_u8(*filter_type);
                engine.state.channels[channel].filter_type = ft;
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.filter_type = ft;
                        voice.svf.filter_type = ft;
                    }
                }
            }

            Effect::FilterCutoffSlide { amount } => {
                engine.state.channels[channel].last_filter_cutoff_slide = *amount;
                engine.state.channels[channel].active_effects.filter_cutoff_slide = true;
                let slide = *amount as f32;
                let new_cutoff = (engine.state.channels[channel].filter_cutoff + slide).clamp(0.0, 0xFFFF as f32);
                engine.state.channels[channel].filter_cutoff = new_cutoff;
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.filter_cutoff = new_cutoff;
                    }
                }
            }

            Effect::SetSendLevel { send_index, level } => {
                let idx = *send_index as usize;
                if idx < NUM_SEND_BUSES {
                    let level_f = (*level as f32) / 15.0;
                    engine.state.channels[channel].send_levels[idx] = level_f;
                }
            }

            Effect::SetSendBusParam { bus, param, .. } => {
                let bus = *bus as usize;
                let param_idx = (*param as u32) % 4;
                let mem_idx = bus * 4 + (*param as usize) % 4;
                let value = engine.state.channels[channel].last_send_param_value[mem_idx];
                let actual_value = (value as f32) / 255.0;
                if bus < NUM_SEND_BUSES {
                    engine.pending_send_fx_params.push((bus, param_idx, actual_value));
                }
            }
        }
    }

    pub fn process_tick(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, tick: u8) {
        let linear = engine.module.as_ref().map(|m| m.flags.linear_slides).unwrap_or(false);

        for ch in 0..engine.state.channels.len() {
            let ch_state = &mut engine.state.channels[ch];
            let vol_kol = ch_state.vol_kol;
            if vol_kol > 0 {
                let vfx = vol_kol >> 4;
                match vfx {
                    0x6 => {
                        let amt = vol_kol & 0x0F;
                        let new_vol = ch_state.real_vol.saturating_sub(amt);
                        ch_state.real_vol = new_vol;
                        ch_state.channel_volume = new_vol;
                    }
                    0x7 => {
                        let amt = vol_kol & 0x0F;
                        let new_vol = (ch_state.real_vol + amt).min(64);
                        ch_state.real_vol = new_vol;
                        ch_state.channel_volume = new_vol;
                    }
                    0xD => {
                        let amt = vol_kol & 0x0F;
                        let new_pan = ch_state.channel_panning.saturating_sub(amt);
                        ch_state.channel_panning = new_pan;
                    }
                    0xE => {
                        let amt = vol_kol & 0x0F;
                        let new_pan = (ch_state.channel_panning + amt).min(255);
                        ch_state.channel_panning = new_pan;
                    }
                    _ => {}
                }
            }

            let ae = engine.state.channels[ch].active_effects;

            if ae.arpeggio
                && (engine.state.channels[ch].last_arpeggio.0 > 0
                    || engine.state.channels[ch].last_arpeggio.1 > 0)
            {
                engine.apply_arpeggio_period(ch, tick, linear);
            }
            if ae.portamento_up {
                let spd = engine.state.channels[ch].last_portamento_up_speed;
                if spd > 0 {
                    let spd_period = (spd as u16) << 2;
                    {
                        let ch = &mut engine.state.channels[ch];
                        ch.real_period = ch.real_period.saturating_sub(spd_period).max(1);
                        ch.out_period = ch.real_period;
                    }
                    let module = engine.module.as_ref().unwrap().clone();
                    let freq = crate::sequencer::period::period_to_frequency(engine.state.channels[ch].out_period, module.flags.linear_slides, 8363);
                    let delta = if engine.output_sample_rate > 0.0 { freq / engine.output_sample_rate } else { 0.0 };
                    for voice in &mut engine.voices {
                        if voice.active && voice.channel == Some(ch) {
                            voice.current_frequency = freq;
                            voice.sample_delta = delta;
                        }
                    }
                }
            }
            if ae.portamento_down {
                let spd = engine.state.channels[ch].last_portamento_down_speed;
                if spd > 0 {
                    let spd_period = (spd as u16) << 2;
                    {
                        let ch = &mut engine.state.channels[ch];
                        ch.real_period = ch.real_period.saturating_add(spd_period).min(31999);
                        ch.out_period = ch.real_period;
                    }
                    let module = engine.module.as_ref().unwrap().clone();
                    let freq = crate::sequencer::period::period_to_frequency(engine.state.channels[ch].out_period, module.flags.linear_slides, 8363);
                    let delta = if engine.output_sample_rate > 0.0 { freq / engine.output_sample_rate } else { 0.0 };
                    for voice in &mut engine.voices {
                        if voice.active && voice.channel == Some(ch) {
                            voice.current_frequency = freq;
                            voice.sample_delta = delta;
                        }
                    }
                }
            }
            if ae.tone_portamento {
                engine.apply_tone_portamento_period(ch, linear);
            }
            if ae.vibrato {
                engine.apply_vibrato_period(ch, linear);
            }
            if ae.tremolo {
                engine.apply_tremolo_period(ch);
            }
            if ae.volume_slide {
                engine.apply_volume_slide_period(ch);
            }
            if ae.tremor {
                engine.apply_tremor_period(ch);
            }

            if ae.panning_slide {
                engine.apply_panning_slide(ch);
            }

            if ae.filter_cutoff_slide {
                let slide = engine.state.channels[ch].last_filter_cutoff_slide as f32;
                let new_cutoff = (engine.state.channels[ch].filter_cutoff + slide).clamp(0.0, 0xFFFF as f32);
                engine.state.channels[ch].filter_cutoff = new_cutoff;
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(ch) {
                        voice.filter_cutoff = new_cutoff;
                    }
                }
            }

            let retrig_speed = engine.state.channels[ch].retrig_speed;
            let retrig_interval = engine.state.channels[ch].last_retrigger_interval;
            if retrig_speed > 0 && tick > 0 && tick % retrig_speed == 0 {
                engine.do_multi_retrig_period(ch, linear);
            } else if retrig_interval > 0 && tick > 0 && tick % retrig_interval == 0 {
                engine.retrig_channel_note_period(ch, linear);
            }

            let delay_ticks = engine.state.channels[ch].note_delay_ticks;
            if delay_ticks > 0 && tick == delay_ticks {
                engine.trigger_delayed_note_period(ch, linear);
            }

            let note_cut = engine.state.channels[ch].note_cut_tick;
            if let Some(cutoff) = note_cut {
                if tick == cutoff {
                    engine.cut_channel_voices(ch);
                    engine.state.channels[ch].note_cut_tick = None;
                }
            }

            if engine.state.channels[ch].active_effects.key_off {
                engine.state.channels[ch].active_effects.key_off = false;
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(ch) {
                        if let Some(ref mut env) = voice.vol_env { env.released = true; }
                        if let Some(ref mut env) = voice.pan_env { env.released = true; }
                        if let Some(ref mut env) = voice.pitch_env { env.released = true; }
                        if let Some(ref mut env) = voice.filter_env { env.released = true; }
                    }
                }
            }

            if !ae.tremolo {
                let vol = engine.compute_channel_volume(ch);
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(ch) {
                        voice.base_volume = vol;
                    }
                }
            }
        }
    }

    pub fn trigger_note(
        &mut self,
        engine: &mut crate::audio::sequencer_engine::SequencerEngine,
        channel: usize,
        note_key: u8,
        _remapped_key: u8,
        sample: Option<&Sample>,
        sample_idx: usize,
        cell: &Cell,
        instrument_idx: usize,
    ) {
        if sample.is_none() || sample_idx == 0 {
            return;
        }
        let sample = sample.unwrap();

        let (period, linear) = {
            let ch_state = &mut engine.state.channels[channel];
            ch_state.rel_ton = sample.relative_note;

            let fine_tune = if let Effect::SetFineTune { tune } = &cell.effect {
                ch_state.fine_tune_offset = (((*tune & 0x0F) << 4) as u8).wrapping_sub(128) as i8;
                ch_state.fine_tune_offset
            } else {
                ch_state.fine_tune_offset = sample.fine_tune;
                sample.fine_tune
            };

            let note_with_rel = note_key.saturating_add(sample.relative_note as u8);
            if note_with_rel >= 120 {
                return;
            }
            let module = engine.module.as_ref().unwrap().clone();
            let period = get_note_period(note_with_rel, fine_tune, module.flags.linear_slides);
            ch_state.real_period = period;
            ch_state.out_period = period;

            ch_state.real_vol = sample.default_volume.min(64);
            ch_state.old_vol = sample.default_volume.min(64);
            ch_state.old_pan = sample.default_panning;

            (period, module.flags.linear_slides)
        };

        let module = engine.module.as_ref().unwrap().clone();
        let playback_freq = period_to_frequency(period, linear, 8363);

        let nna = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            module.instruments[instrument_idx].nna
        } else {
            NewNoteAction::NoteCut
        };
        let fade_out = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            module.instruments[instrument_idx].fade_out
        } else {
            0
        };

        let sample_offset = engine.calculate_sample_offset(channel, cell, sample);
        engine.handle_nna(channel, NewNoteAction::NoteCut,
            DuplicateCheckType::Disabled, DuplicateCheckAction::NoteCut,
            instrument_idx, sample_idx);

        let voice_idx = engine.allocate_voice(channel);
        let vol = engine.compute_channel_volume(channel);
        let pan = engine.compute_channel_panning(channel);

        let voice = &mut engine.voices[voice_idx];
        voice.trigger(
            sample.data.clone(),
            sample.sample_rate as f64,
            sample.loop_type,
            sample.loop_start,
            sample.loop_end,
            playback_freq,
            engine.output_sample_rate,
            vol,
            pan,
            sample_offset,
            Some(instrument_idx as u8),
            Some(sample_idx as u8),
            Note::On(note_key),
            nna,
            fade_out,
        );
        voice.channel = Some(channel);
        if sample.loop_type == LoopType::Backward {
            voice.direction = -1.0;
            let max_pos = sample.data.len().max(1) - 1;
            if sample_offset == 0 {
                voice.position = max_pos as f64;
            }
        }

        if instrument_idx > 0 && instrument_idx < module.instruments.len() {
            let inst = &module.instruments[instrument_idx];
            let voice = &mut engine.voices[voice_idx];

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

            voice.instrument_index = Some(instrument_idx as u8);
        }

        if let Effect::SetSampleOffset { offset } = &cell.effect {
            if *offset > 0 {
                engine.state.channels[channel].last_sample_offset = *offset;
            }
        } else if let Effect::FormatSpecific(fe) = &cell.effect {
            if let Some(offset) = fe.sample_offset() {
                if offset > 0 {
                    engine.state.channels[channel].last_sample_offset = offset;
                }
            }
        }
    }

    pub fn trigger_delayed_note(&mut self, _engine: &mut crate::audio::sequencer_engine::SequencerEngine, _channel: usize) {
    }

    pub fn process_volume_column(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize, vol: u8) {
        let ch = &mut engine.state.channels[channel];
        ch.vol_kol = vol;
        if vol <= 64 {
            ch.channel_volume = vol;
            ch.row_volume = vol;
        }
    }

    pub fn setup_portamento(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize, note_key: u8, remapped_key: u8, sample: Option<&Sample>, _sample_idx: usize) {
        if let Some(s) = sample {
            let ch = &mut engine.state.channels[channel];
            ch.rel_ton = s.relative_note;
        }
        let module = engine.module.as_ref().unwrap().clone();
        let ch = &mut engine.state.channels[channel];
        let ft = ch.fine_tune_offset;
        let want_period = crate::sequencer::period::get_note_period(
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
    }

    pub fn init_sample_defaults(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize, _cell: &Cell, sample: Option<&Sample>) {
        if let Some(s) = sample {
            engine.state.channels[channel].channel_volume = s.default_volume.min(64);
            engine.state.channels[channel].channel_panning = s.default_panning;
        }
    }

    pub fn handle_note_off(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize) {
        let module = match engine.module.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };

        for voice in &mut engine.voices {
            if !voice.active || voice.channel != Some(channel) {
                continue;
            }

            let inst_idx = voice.instrument_index.unwrap_or(0) as usize;
            if inst_idx > 0 && inst_idx < module.instruments.len() {
                let inst = &module.instruments[inst_idx];

                if let Some(ref pan_env) = inst.panning_envelope {
                    if !pan_env.flags.enabled {
                        if let Some(ref mut pe) = voice.pan_env {
                            if pe.current_point < pan_env.points.len() {
                                if pe.position >= pan_env.points[pe.current_point].tick as f32 {
                                    pe.position = pan_env.points[pe.current_point].tick as f32 - 1.0;
                                }
                            }
                        }
                    }
                }

                if let Some(ref vol_env) = inst.volume_envelope {
                    if vol_env.flags.enabled {
                        if let Some(ref mut ve) = voice.vol_env {
                            if ve.current_point < vol_env.points.len() {
                                if ve.position >= vol_env.points[ve.current_point].tick as f32 {
                                    ve.position = vol_env.points[ve.current_point].tick as f32 - 1.0;
                                }
                            }
                        }
                    }
                } else {
                    let ch = &mut engine.state.channels[channel];
                    ch.real_vol = 0;
                    ch.channel_volume = 0;
                    voice.base_volume = 0.0;
                }
            }

            voice.note_off = true;
            voice.env_sustain_active = false;
            if let Some(ref mut env) = voice.vol_env { env.released = true; }
            if let Some(ref mut env) = voice.pan_env { env.released = true; }
            if let Some(ref mut env) = voice.filter_env { env.released = true; }
        }
    }
}
