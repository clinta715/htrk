use crate::audio::effects::compute_samples_per_tick;
use crate::sequencer::effect::{Effect, FilterType, FormatEffect, ItEffect, ModEffect, S3mEffect, XmEffect, NUM_SEND_BUSES};
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::{Sample, VibratoWaveform};

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
                    FormatEffect::S3m(S3mEffect::Raw { effect, param }) => {
                        if *effect == 0x19A {
                            ch.high_sample_offset = *param;
                        }
                    }
                    FormatEffect::Mod(ModEffect::Filter(enabled)) => {
                        ch.filter_enabled = *enabled;
                        if engine.amiga_led_filter {
                            for voice in &mut engine.voices {
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
                engine.global_volume = engine.state.global_volume as f32 / 128.0;
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
                for voice in &mut engine.voices {
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
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.tremolo_waveform = w;
                    }
                }
            }

            Effect::SetFineTune { tune } => {
                ch.fine_tune_offset = *tune as i8;
                for voice in &mut engine.voices {
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
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = v;
                        voice.channel_volume = 1.0;
                    }
                }
            }

            Effect::FineVolumeSlideDown { amount } => {
                ch.channel_volume = ch.channel_volume.saturating_sub(*amount);
                let v = ch.channel_volume as f32 / 64.0 * engine.state.global_volume as f32 / 128.0;
                for voice in &mut engine.voices {
                    if voice.active && voice.channel == Some(channel) {
                        voice.base_volume = v;
                        voice.channel_volume = 1.0;
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
                    engine.global_volume = engine.state.global_volume as f32 / 128.0;
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

    pub fn process_tick(&mut self, _engine: &mut crate::audio::sequencer_engine::SequencerEngine, _tick: u8) {
    }

    pub fn trigger_note(
        &mut self,
        _engine: &mut crate::audio::sequencer_engine::SequencerEngine,
        _channel: usize,
        _note_key: u8,
        _remapped_key: u8,
        _sample: Option<&Sample>,
        _sample_idx: usize,
        _cell: &Cell,
        _instrument_idx: usize,
    ) {
    }

    pub fn trigger_delayed_note(&mut self, _engine: &mut crate::audio::sequencer_engine::SequencerEngine, _channel: usize) {
    }

    pub fn process_volume_column(&mut self, _engine: &mut crate::audio::sequencer_engine::SequencerEngine, _channel: usize, _vol: u8) {
    }

    pub fn handle_note_off(&mut self, _engine: &mut crate::audio::sequencer_engine::SequencerEngine, _channel: usize) {
    }
}
