use std::sync::Arc;

use crate::audio::effects::compute_samples_per_tick;
use crate::audio::effects::{compute_playback_frequency, fastrand};
use crate::audio::filter::StateVariableFilter;
use crate::audio::voice::EnvelopeState;
use crate::sequencer::effect::{Effect, FilterType, FormatEffect, ItEffect, ModEffect, S3mEffect, XmEffect, NUM_SEND_BUSES};
use crate::sequencer::instrument::{DuplicateCheckAction, DuplicateCheckType, NewNoteAction};
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::{LoopType, Sample, VibratoWaveform};

pub struct LegacyProcessor;

impl LegacyProcessor {
    pub fn new() -> Self {
        LegacyProcessor
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
                }
            }

            Effect::SetVolume { volume } | Effect::VolSetVolume { vol: volume } => {
                ch.channel_volume = (*volume).min(64);
                ch.row_volume = (*volume).min(64);
                let vol = engine.compute_channel_volume(channel);
                for voice in &mut engine.voice_pool.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = vol;

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
                    FormatEffect::S3m(S3mEffect::Raw { effect, param }) => {
                        if *effect == 0x19A {
                            ch.high_sample_offset = *param;
                        }
                    }
                    FormatEffect::Mod(ModEffect::Filter(enabled)) => {
                        ch.filter_enabled = *enabled;
                        if engine.amiga_led_filter {
                            for voice in &mut engine.voice_pool.voices {
                                if voice.active && voice.channel == Some(channel) {
                                    voice.filter_enabled = *enabled;
                                    voice.amiga_led_filter = *enabled;
                                }
                            }
                        }
                    }
                    FormatEffect::Mod(ModEffect::FunkIt { speed }) => {
                        ch.funk_speed = *speed;
                        ch.funk_pos = 0;
                    }
                    FormatEffect::Mod(ModEffect::KarplusStrong { param }) => {
                        ch.karplus_param = *param;
                    }
                    _ => {}
                }
            }

            Effect::PositionJump { order } => {
                engine.state.position_jump_order = Some(*order);
            }

            Effect::PatternBreak { row } => {
                engine.state.pattern_break_row = Some(*row);
            }

            Effect::TonePortamento { speed } => {
                if *speed > 0 {
                    ch.last_tone_portamento_speed = *speed;
                }
                ch.active_effects.tone_portamento = true;
            }

            Effect::PortamentoUp { speed } => {
                if *speed > 0 {
                    ch.last_portamento_up_speed = *speed;
                }
                ch.active_effects.portamento_up = true;
            }

            Effect::PortamentoDown { speed } => {
                if *speed > 0 {
                    ch.last_portamento_down_speed = *speed;
                }
                ch.active_effects.portamento_down = true;
            }

            Effect::ExtraFinePortamentoUp { speed } => {
                if is_row_start && *speed > 0 {
                    let spd = ((*speed as u16 + 2) >> 2).max(1);
                    engine.apply_portamento_up(channel, spd);
                }
            }

            Effect::ExtraFinePortamentoDown { speed } => {
                if is_row_start && *speed > 0 {
                    let spd = ((*speed as u16 + 2) >> 2).max(1);
                    engine.apply_portamento_down(channel, spd);
                }
            }

            Effect::FinePortamentoUp { speed } => {
                if is_row_start && *speed > 0 {
                    engine.apply_portamento_up(channel, *speed as u16);
                }
            }

            Effect::FinePortamentoDown { speed } => {
                if is_row_start && *speed > 0 {
                    engine.apply_portamento_down(channel, *speed as u16);
                }
            }

            Effect::Vibrato { speed, depth } => {
                if *speed > 0 { ch.last_vibrato_speed = *speed; }
                if *depth > 0 { ch.last_vibrato_depth = *depth; }
                ch.active_effects.vibrato = true;
            }

            Effect::Tremolo { speed, depth } => {
                if *speed > 0 { ch.last_tremolo_speed = *speed; }
                if *depth > 0 { ch.last_tremolo_depth = *depth; }
                ch.active_effects.tremolo = true;
            }

            Effect::VolumeSlide { up, down } => {
                if *up > 0 { ch.last_volume_slide_up = *up; }
                if *down > 0 { ch.last_volume_slide_down = *down; }
                ch.last_volume_slide_param = (*up << 4) | *down;
                ch.active_effects.volume_slide = true;
            }

            Effect::TonePortamentoVolumeSlide { up } => {
                let param = *up as u8;
                let up_val = param >> 4;
                let down_val = param & 0x0F;
                if up_val > 0 { ch.last_volume_slide_up = up_val; }
                if down_val > 0 { ch.last_volume_slide_down = down_val; }
                ch.last_volume_slide_param = param;
                ch.active_effects.tone_portamento = true;
                ch.active_effects.volume_slide = true;
            }

            Effect::VibratoVolumeSlide { up } => {
                let param = *up as u8;
                let up_val = param >> 4;
                let down_val = param & 0x0F;
                if up_val > 0 { ch.last_volume_slide_up = up_val; }
                if down_val > 0 { ch.last_volume_slide_down = down_val; }
                ch.last_volume_slide_param = param;
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
            }

            Effect::SetGlobalVolume { volume } => {
                engine.state.global_volume = (*volume).min(128);

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
                let w = match waveform & 0x03 {
                    0 => VibratoWaveform::Sine,
                    1 => VibratoWaveform::Square,
                    2 => VibratoWaveform::Ramp,
                    3 => VibratoWaveform::Random,
                    _ => VibratoWaveform::Sine,
                };
                for voice in &mut engine.voice_pool.voices {
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
                for voice in &mut engine.voice_pool.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.tremolo_waveform = w;
                    }
                }
            }

            Effect::SetFineTune { tune } => {
                ch.fine_tune_offset = *tune as i8;
                for voice in &mut engine.voice_pool.voices {
                    if voice.active && voice.channel == Some(channel) {
                        let detune = (*tune as f64 - 8.0) / 128.0;
                        voice.current_frequency = voice.base_frequency * 2.0_f64.powf(detune / 12.0);
                        voice.sample_delta = voice.current_frequency / engine.output_sample_rate;
                    }
                }
            }

            Effect::SetEnvelopePosition { tick } => {
                engine.set_envelope_position(channel, *tick);
            }

            Effect::Retrigger { interval } => {
                if *interval > 0 {
                    ch.last_retrigger_interval = *interval;
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
                ch.tremor_active = true;
                ch.active_effects.tremor = true;
            }

            Effect::FineVolumeSlideUp { amount } => {
                ch.channel_volume = (ch.channel_volume + amount).min(64);
                let v = ch.channel_volume as f32 / 64.0 * engine.state.global_volume as f32 / 128.0;
                for voice in &mut engine.voice_pool.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = v;

                    }
                }
            }

            Effect::FineVolumeSlideDown { amount } => {
                ch.channel_volume = ch.channel_volume.saturating_sub(*amount);
                let v = ch.channel_volume as f32 / 64.0 * engine.state.global_volume as f32 / 128.0;
                for voice in &mut engine.voice_pool.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = v;

                    }
                }
            }

            Effect::NoteDelay { ticks } => {
                if *ticks == 0 {
                    engine.set_channel_delay_tick(channel, 1);
                } else {
                    engine.set_channel_delay_tick(channel, *ticks);
                }
            }

            Effect::ExtendedEffect { param } => {
                let sub = param >> 4;
                let val = param & 0x0F;
                match sub {
                    0x1 => {
                        if is_row_start {
                            engine.apply_portamento_up(channel, (val as u16) << 4);
                        }
                    }
                    0x2 => {
                        if is_row_start {
                            engine.apply_portamento_down(channel, (val as u16) << 4);
                        }
                    }
                    0x8 => {
                        engine.state.channels[channel].channel_panning = (val << 4).min(255);
                    }
                    0x9 => {
                        if val > 0 {
                            engine.state.channels[channel].last_retrigger_interval = val;
                        }
                    }
                    0xA => {
                        engine.state.channels[channel].channel_volume =
                            (engine.state.channels[channel].channel_volume as u16 + val as u16).min(64) as u8;
                    }
                    0xB => {
                        engine.state.channels[channel].channel_volume =
                            engine.state.channels[channel].channel_volume.saturating_sub(val);
                    }
                    0xC => {
                        engine.set_channel_cutoff_tick(channel, val);
                    }
                    _ => {}
                }
            }

            Effect::PatternLoop { count } => {
                if *count == 0 {
                    if engine.state.pattern_loop_count == 0 {
                        engine.state.pattern_loop_start = Some((engine.state.current_order, engine.state.current_row));
                    }
                } else if !engine.state.pattern_loop_final_pass {
                    if engine.state.pattern_loop_count == 0 {
                        engine.state.pattern_loop_count = *count;
                    }

                    let loop_row = engine.state.pattern_loop_start
                        .map(|(_, row)| row)
                        .unwrap_or(0);
                    engine.state.pattern_loop_jump_target = Some(loop_row);
                }
            }

            Effect::Panbrello { speed, depth } => {
                if *speed > 0 { ch.last_panbrello_speed = *speed; }
                if *depth > 0 { ch.last_panbrello_depth = *depth; }
                ch.active_effects.panbrello = true;
            }

            Effect::GlobalVolumeSlide { up, down } => {
                if *up > 0 {
                    engine.state.last_global_volume_up = *up as u8;
                }
                if *down > 0 {
                    engine.state.last_global_volume_down = (*down).unsigned_abs() as u8;
                }
                let up_val: i16 = if *up > 0 { *up as i16 } else { engine.state.last_global_volume_up as i16 };
                let down_val: i16 = if *down > 0 { (*down).unsigned_abs() as i16 } else { engine.state.last_global_volume_down as i16 };
                if up_val > 0 || down_val > 0 {
                    let new_vol = engine.state.global_volume as i16 + up_val - down_val;
                    engine.state.global_volume = new_vol.clamp(0, 128) as u8;
    
                }
                ch.active_effects.global_volume_slide = true;
            }

            Effect::VolFineSlideUp { amount } => {
                engine.state.channels[channel].channel_volume =
                    (engine.state.channels[channel].channel_volume as u16 + *amount as u16).min(64) as u8;
            }
            Effect::VolFineSlideDown { amount } => {
                engine.state.channels[channel].channel_volume =
                    engine.state.channels[channel].channel_volume.saturating_sub(*amount);
            }
            Effect::VolSlideUp { amount } => {
                engine.state.channels[channel].channel_volume =
                    (engine.state.channels[channel].channel_volume as u16 + *amount as u16).min(64) as u8;
            }
            Effect::VolSlideDown { amount } => {
                engine.state.channels[channel].channel_volume =
                    engine.state.channels[channel].channel_volume.saturating_sub(*amount);
            }
            Effect::VolPortamento { speed } => {
                if *speed > 0 {
                    engine.state.channels[channel].last_tone_portamento_speed = *speed;
                }
                engine.state.channels[channel].active_effects.tone_portamento = true;
                if engine.state.channels[channel].portamento_target_period.is_none() {
                    if let Note::On(key) = engine.state.channels[channel].last_note {
                        let module = match engine.module.as_ref() {
                            Some(m) => m.clone(),
                            None => return,
                        };
                        let inst_idx = engine.state.channels[channel].last_instrument as usize;
                        let has_inst = !module.instruments.is_empty();
                        if has_inst && inst_idx > 0 && inst_idx < module.instruments.len() && (key as usize) < 120 {
                            let sample_idx = module.instruments[inst_idx].sample_map[key as usize] as usize;
                            let rk = module.instruments[inst_idx].note_map[key as usize];
                            let sample = if sample_idx > 0 && sample_idx < module.samples.len() {
                                Some(&module.samples[sample_idx])
                            } else {
                                None
                            };
                            let (tp, tf) = engine.compute_portamento_target(channel, key, rk, sample, sample_idx, &module);
                            let ch = &mut engine.state.channels[channel];
                            ch.portamento_target_period = Some(tp);
                            ch.portamento_target_frequency = Some(tf);
                        }
                    }
                }
            }
            Effect::VolVibrato { speed } => {
                if *speed > 0 {
                    engine.state.channels[channel].last_vibrato_speed = *speed;
                }
                engine.state.channels[channel].active_effects.vibrato = true;
            }

            Effect::SetFilterCutoff { cutoff } => {
                let cutoff_f = *cutoff as f32;
                engine.state.channels[channel].filter_cutoff = cutoff_f;
                for voice in &mut engine.voice_pool.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.filter_cutoff = cutoff_f;
                    }
                }
            }

            Effect::SetFilterResonance { resonance } => {
                let res_f = *resonance as f32 / 128.0;
                engine.state.channels[channel].filter_resonance = res_f;
                for voice in &mut engine.voice_pool.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.filter_resonance = res_f;
                    }
                }
            }

            Effect::SetFilterType { filter_type } => {
                let ft = FilterType::from_u8(*filter_type);
                engine.state.channels[channel].filter_type = ft;
                for voice in &mut engine.voice_pool.voices {
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
                for voice in &mut engine.voice_pool.voices {
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
        let module_format = engine.module.as_ref().unwrap().format;

        for ch in 0..engine.state.channels.len() {
            let ae = engine.state.channels[ch].active_effects;

            if ae.arpeggio
                && (engine.state.channels[ch].last_arpeggio.0 > 0
                    || engine.state.channels[ch].last_arpeggio.1 > 0)
            {
                let (arp1, arp2) = engine.state.channels[ch].last_arpeggio;
                engine.apply_arpeggio(ch, tick, arp1, arp2);
            }
            if ae.portamento_up {
                let spd = engine.state.channels[ch].last_portamento_up_speed;
                if spd > 0 {
                    let actual_spd = if module_format == crate::sequencer::module::ModuleFormat::S3M { spd as u16 * 4 } else { spd as u16 };
                    engine.apply_portamento_up(ch, actual_spd);
                }
            }
            if ae.portamento_down {
                let spd = engine.state.channels[ch].last_portamento_down_speed;
                if spd > 0 {
                    let actual_spd = if module_format == crate::sequencer::module::ModuleFormat::S3M { spd as u16 * 4 } else { spd as u16 };
                    engine.apply_portamento_down(ch, actual_spd);
                }
            }
            if ae.tone_portamento {
                let tp_speed = engine.state.channels[ch].last_tone_portamento_speed;
                if tp_speed > 0 && engine.state.channels[ch].portamento_target_period.is_some() {
                    let actual_spd = if module_format == crate::sequencer::module::ModuleFormat::S3M { tp_speed as u16 * 4 } else { tp_speed as u16 };
                    engine.apply_tone_portamento(ch, actual_spd);
                }
            }
            if ae.vibrato {
                let vib_speed = engine.state.channels[ch].last_vibrato_speed;
                let vib_depth = engine.state.channels[ch].last_vibrato_depth;
                if vib_speed > 0 || vib_depth > 0 {
                    engine.apply_vibrato(ch, vib_speed, vib_depth);
                }
            }
            if ae.tremolo {
                let trem_speed = engine.state.channels[ch].last_tremolo_speed;
                let trem_depth = engine.state.channels[ch].last_tremolo_depth;
                if trem_speed > 0 || trem_depth > 0 {
                    engine.apply_tremolo(ch, trem_speed, trem_depth);
                }
            }
            if ae.volume_slide {
                let vol_up = engine.state.channels[ch].last_volume_slide_up;
                let vol_down = engine.state.channels[ch].last_volume_slide_down;
                if vol_up > 0 || vol_down > 0 {
                    engine.apply_volume_slide(ch);
                }
            }
            if ae.tremor {
                let ontime = engine.state.channels[ch].tremor_ontime;
                let offtime = engine.state.channels[ch].tremor_offtime;
                if ontime > 0 || offtime > 0 {
                    engine.apply_tremor(ch, tick, ontime, offtime);
                }
            }
            if ae.global_volume_slide {
                let up_val = engine.state.last_global_volume_up as i16;
                let down_val = engine.state.last_global_volume_down as i16;
                if up_val > 0 || down_val > 0 {
                    let new_vol = engine.state.global_volume as i16 + up_val - down_val;
                    engine.state.global_volume = new_vol.clamp(0, 128) as u8;
    
                }
            }

            if ae.panbrello {
                let pb_speed = engine.state.channels[ch].last_panbrello_speed;
                let pb_depth = engine.state.channels[ch].last_panbrello_depth;
                if pb_speed > 0 || pb_depth > 0 {
                    engine.apply_panbrello(ch, pb_speed, pb_depth);
                }
            }

            if ae.filter_cutoff_slide {
                let slide = engine.state.channels[ch].last_filter_cutoff_slide as f32;
                let new_cutoff = (engine.state.channels[ch].filter_cutoff + slide).clamp(0.0, 0xFFFF as f32);
                engine.state.channels[ch].filter_cutoff = new_cutoff;
                for voice in &mut engine.voice_pool.voices {
                    if voice.active && voice.channel == Some(ch) {
                        voice.filter_cutoff = new_cutoff;
                    }
                }
            }

            let fs = engine.state.channels[ch].funk_speed;
            if fs > 0 {
                let fp = &mut engine.state.channels[ch].funk_pos;
                *fp = fp.wrapping_add(fs);
                if *fp >= 128 {
                    *fp = 0;
                    for voice in &mut engine.voice_pool.voices {
                        if voice.active && voice.channel == Some(ch) && !voice.karplus_strong {
                            let offset = (crate::audio::effects::fastrand() * 4.0) as u32;
                            voice.position = voice.position + offset as f64;
                        }
                    }
                }
            }

            let retrigger_interval = engine.state.channels[ch].last_retrigger_interval;
            if retrigger_interval > 0 && tick > 0 && tick % retrigger_interval == 0 {
                engine.retrigger_channel_note(ch);
            }

            if let Some(delay) = engine.get_channel_delay_tick(ch) {
                if tick == delay as u8 {
                    engine.trigger_delayed_note(ch);
                }
            }

            if let Some(cutoff) = engine.get_channel_cutoff_tick(ch) {
                if tick == cutoff as u8 {
                    engine.cut_channel_voices(ch);
                }
            }

            if !ae.tremolo {
                let vol = engine.compute_channel_volume(ch);
                for voice in &mut engine.voice_pool.voices {
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
        remapped_key: u8,
        sample: Option<&Sample>,
        sample_idx: usize,
        cell: &Cell,
        instrument_idx: usize,
    ) {
        if sample.is_none() || sample_idx == 0 {
            return;
        }
        let sample = sample.unwrap();

        let nna;
        let dct;
        let dca;
        let fade_out_rate;
        let linear_slides;
        let instruments_len;
        {
            let module = &**engine.module.as_ref().unwrap();
            nna = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
                module.instruments[instrument_idx].nna
            } else {
                NewNoteAction::NoteCut
            };
            dct = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
                module.instruments[instrument_idx].duplicate_check_type
            } else {
                DuplicateCheckType::Disabled
            };
            dca = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
                module.instruments[instrument_idx].duplicate_check_action
            } else {
                DuplicateCheckAction::NoteCut
            };
            fade_out_rate = if instrument_idx > 0 && instrument_idx < module.instruments.len() {
                module.instruments[instrument_idx].fade_out
            } else {
                0
            };
            linear_slides = module.flags.linear_slides;
            instruments_len = module.instruments.len();
        }
        engine.handle_nna(channel, nna, dct, dca, instrument_idx, sample_idx);

        let freq = match Note::On(remapped_key).frequency() {
            Some(f) => f,
            None => return,
        };

        let fine_tune_offset = if channel < engine.state.channels.len() {
            engine.state.channels[channel].fine_tune_offset
        } else {
            0
        };
        let playback_freq = compute_playback_frequency(
            freq,
            sample.sample_rate,
            sample.relative_note,
            sample.fine_tune.saturating_add(fine_tune_offset),
        );

        if !linear_slides {
            let period = (8363.0 * 428.0 / playback_freq) as u16;
            engine.state.channels[channel].real_period = period;
            engine.state.channels[channel].out_period = period;
        }

        let mut vol = engine.compute_channel_volume(channel);
        let mut pan = engine.compute_channel_panning(channel);

        if instrument_idx > 0 && instrument_idx < instruments_len {
            let inst = &engine.module.as_ref().unwrap().instruments[instrument_idx];
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
                let sep = inst.pitch_pan_separation as f32 / 96.0;
                pan += (note_key as i16 - center) as f32 * sep;
                pan = pan.clamp(0.0, 1.0);
            }
        }

        let sample_offset = engine.calculate_sample_offset(channel, cell, sample);

        let voice_idx = engine.allocate_voice(channel);
        engine.voice_pool.voices[voice_idx].trigger(
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
            fade_out_rate,
        );
        engine.voice_pool.voices[voice_idx].channel = Some(channel);
        if engine.amiga_led_filter {
            engine.voice_pool.voices[voice_idx].amiga_led_filter = true;
        }
        if sample.loop_type == LoopType::Backward {
            engine.voice_pool.voices[voice_idx].direction = -1.0;
            if sample_offset == 0 {
                engine.voice_pool.voices[voice_idx].position = (sample.data.len().max(1) - 1) as f64;
            }
        }

        if let Effect::SetSampleOffset { offset } = cell.effect {
            if offset > 0 {
                engine.state.channels[channel].last_sample_offset = offset;
            }
        } else if let Effect::FormatSpecific(fe) = cell.effect {
            if let Some(offset) = fe.sample_offset() {
                if offset > 0 {
                    engine.state.channels[channel].last_sample_offset = offset;
                }
            }
        }

        if instrument_idx > 0 && instrument_idx < instruments_len {
            let inst = &engine.module.as_ref().unwrap().instruments[instrument_idx];
            let carry_vol = inst.volume_envelope.as_ref().map_or(false, |e| e.flags.carry);
            let carry_pan = inst.panning_envelope.as_ref().map_or(false, |e| e.flags.carry);
            let carry_pitch = inst.pitch_envelope.as_ref().map_or(false, |e| e.flags.carry);

            let mut prev_vol_pos = None;
            let mut prev_pan_pos = None;
            if carry_vol || carry_pan || carry_pitch {
                for v in &engine.voice_pool.voices {
                    if v.active && v.channel == Some(channel) {
                        if carry_vol { prev_vol_pos = v.vol_env.as_ref().map(|e| e.position); }
                        if carry_pan { prev_pan_pos = v.pan_env.as_ref().map(|e| e.position); }
                        break;
                    }
                }
            }

            if let Some(ref vol_env) = inst.volume_envelope {
                if vol_env.flags.enabled {
                    let pos = if carry_vol { prev_vol_pos.unwrap_or(0.0) } else { 0.0 };
                    engine.voice_pool.voices[voice_idx].vol_env = Some(EnvelopeState {
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
                    engine.voice_pool.voices[voice_idx].pan_env = Some(EnvelopeState {
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
                    engine.voice_pool.voices[voice_idx].pitch_env = Some(EnvelopeState {
                        envelope: Arc::new(pitch_env.clone()),
                        current_point: 0,
                        position: 0.0,
                        released: false,
                        finished: false,
                    });
                }
            }
            if let Some(ref filter_env) = inst.filter_envelope {
                if filter_env.flags.enabled {
                    engine.voice_pool.voices[voice_idx].filter_env = Some(EnvelopeState {
                        envelope: Arc::new(filter_env.clone()),
                        current_point: 0,
                        position: 0.0,
                        released: false,
                        finished: false,
                    });
                }
            }

            engine.voice_pool.voices[voice_idx].filter_cutoff = inst.filter_cutoff as f32;
            engine.voice_pool.voices[voice_idx].filter_resonance = inst.filter_resonance as f32 / 128.0;
            engine.voice_pool.voices[voice_idx].filter_type = inst.filter_type;
            engine.voice_pool.voices[voice_idx].svf = StateVariableFilter { low: 0.0, band: 0.0, high: 0.0, filter_type: inst.filter_type };
            engine.voice_pool.voices[voice_idx].envelope_filter_cutoff = 0.0;

            engine.voice_pool.voices[voice_idx].fade_out_rate = inst.fade_out;
        }

        let kp = engine.state.channels[channel].karplus_param;
        if kp > 0 {
            let delay_len = (engine.output_sample_rate / playback_freq) as usize;
            if delay_len > 0 {
                let mut ks_delay = Vec::with_capacity(delay_len);
                for _ in 0..delay_len {
                    ks_delay.push(fastrand() * 2.0 - 1.0);
                }
                engine.voice_pool.voices[voice_idx].ks_delay_line = ks_delay;
                engine.voice_pool.voices[voice_idx].ks_pos = 0;
                engine.voice_pool.voices[voice_idx].karplus_strong = true;
                engine.voice_pool.voices[voice_idx].ks_feedback = if kp > 0 { (kp as f32) / 16.0 } else { 0.5 };
            }
        }
    }

    pub fn trigger_delayed_note(&mut self, _engine: &mut crate::audio::sequencer_engine::SequencerEngine, _channel: usize) {
    }

    pub fn process_volume_column(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize, vol: u8) {
        let ch_state = &mut engine.state.channels[channel];

        if vol <= 64 {
            ch_state.row_volume = vol;
            ch_state.channel_volume = vol;
            return;
        }

        match vol {
            65..=74 => {
                let amount = vol - 65;
                ch_state.channel_volume = (ch_state.channel_volume as u16 + amount as u16).min(64) as u8;
                ch_state.row_volume = ch_state.channel_volume;
            }
            75..=84 => {
                let amount = vol - 75;
                ch_state.channel_volume = ch_state.channel_volume.saturating_sub(amount);
                ch_state.row_volume = ch_state.channel_volume;
            }
            85..=94 => {
                ch_state.last_tone_portamento_speed = vol - 85;
            }
            95..=104 => {
                ch_state.last_vibrato_speed = vol - 95;
            }
            105..=114 => {
                ch_state.last_vibrato_depth = vol - 105;
            }
            115..=124 => {
                ch_state.last_portamento_up_speed = vol - 115;
            }
            125..=127 => {}
            128..=192 => {
                let pan = vol - 128;
                ch_state.channel_panning = (pan as u16 * 255 / 64).min(255) as u8;
            }
            193..=207 => {
                let speed = vol - 193;
                ch_state.last_portamento_up_speed = speed;
                engine.apply_portamento_up(channel, speed as u16);
            }
            208..=222 => {
                let speed = vol - 208;
                ch_state.last_portamento_down_speed = speed;
                engine.apply_portamento_down(channel, speed as u16);
            }
            _ => {}
        }
    }

    pub fn setup_portamento(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize, note_key: u8, remapped_key: u8, sample: Option<&Sample>, sample_idx: usize) {
        let module = &**engine.module.as_ref().unwrap();
        let (target_period, target_freq) = engine.compute_portamento_target(
            channel, note_key, remapped_key, sample, sample_idx, module,
        );
        let ch = &mut engine.state.channels[channel];
        ch.portamento_target_period = Some(target_period);
        ch.portamento_target_frequency = Some(target_freq);
    }

    pub fn init_sample_defaults(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize, cell: &Cell, sample: Option<&Sample>) {
        // IT: volume reset on instrument change only
        if cell.instrument.is_some() {
            if let Some(s) = sample {
                engine.state.channels[channel].channel_volume = s.default_volume.min(64);
                engine.state.channels[channel].channel_panning = s.default_panning;
            }
        }
    }

    pub fn handle_note_off(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize) {
        for voice in &mut engine.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.note_off = true;
                if let Some(ref mut env) = voice.vol_env { env.released = true; }
                if let Some(ref mut env) = voice.pan_env { env.released = true; }
                if let Some(ref mut env) = voice.pitch_env { env.released = true; }
                if let Some(ref mut env) = voice.filter_env { env.released = true; }
            }
        }
    }
}
