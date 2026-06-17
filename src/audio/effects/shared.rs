use crate::sequencer::effect::{Effect, FilterType, NUM_SEND_BUSES};

/// Handle effects that are identical across all format processors.
/// Returns true if the effect was handled (caller should skip further processing).
pub fn dispatch_shared_effect(
    engine: &mut crate::audio::sequencer_engine::SequencerEngine,
    channel: usize,
    effect: &Effect,
) -> bool {
    let ch = &mut engine.state.channels[channel];

    match effect {
        Effect::SetSpeed { speed } => {
            if *speed > 0 {
                engine.state.clock.set_speed(*speed);
            }
            true
        }

        Effect::SetPanning { pan } | Effect::SetPanning16 { pan } => {
            ch.channel_panning = (*pan).min(255);
            true
        }

        Effect::SetPanPosition { pan } => {
            ch.channel_panning = (*pan).min(255);
            true
        }

        Effect::SetSampleOffset { offset } => {
            if *offset > 0 {
                ch.last_sample_offset = *offset;
            }
            true
        }

        Effect::SetFilterCutoff { cutoff } => {
            let cutoff_f = *cutoff as f32;
            engine.state.channels[channel].filter_cutoff = cutoff_f;
            for voice in &mut engine.voice_pool.voices {
                if voice.active && voice.channel == Some(channel) {
                    voice.filter_cutoff = cutoff_f;
                }
            }
            true
        }

        Effect::SetFilterResonance { resonance } => {
            let res_f = *resonance as f32 / 128.0;
            engine.state.channels[channel].filter_resonance = res_f;
            for voice in &mut engine.voice_pool.voices {
                if voice.active && voice.channel == Some(channel) {
                    voice.filter_resonance = res_f;
                }
            }
            true
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
            true
        }

        Effect::FilterCutoffSlide { amount } => {
            engine.state.channels[channel].last_filter_cutoff_slide = *amount;
            engine.state.channels[channel].active_effects.filter_cutoff_slide = true;
            let slide = *amount as f32;
            let new_cutoff = (engine.state.channels[channel].filter_cutoff + slide)
                .clamp(0.0, 0xFFFF as f32);
            engine.state.channels[channel].filter_cutoff = new_cutoff;
            for voice in &mut engine.voice_pool.voices {
                if voice.active && voice.channel == Some(channel) {
                    voice.filter_cutoff = new_cutoff;
                }
            }
            true
        }

        Effect::SetSendLevel { send_index, level } => {
            let idx = *send_index as usize;
            if idx < NUM_SEND_BUSES {
                let level_f = (*level as f32) / 15.0;
                engine.state.channels[channel].send_levels[idx] = level_f;
            }
            true
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
            true
        }

        Effect::PositionJump { order } => {
            engine.state.position_jump_order = Some(*order);
            true
        }

        Effect::PatternBreak { row } => {
            engine.state.position_jump_flag = true;
            engine.state.pattern_break_row = Some(*row);
            true
        }

        Effect::PatternDelay { ticks } => {
            if !engine.state.row_delay_active {
                engine.state.pattern_delay_ticks = *ticks;
                engine.state.row_delay_active = true;
            }
            true
        }

        Effect::GlissandoControl { on } => {
            ch.glissando = *on;
            true
        }

        Effect::None => true,

        _ => false,
    }
}

/// Handle the common process_tick infrastructure that is identical
/// across format processors: filter cutoff slide tick and volume
/// resync (when tremolo is not active).
pub fn shared_process_tick_tail(
    engine: &mut crate::audio::sequencer_engine::SequencerEngine,
    channel: usize,
    _tick: u8,
) {
    let do_filter = engine.state.channels[channel].active_effects.filter_cutoff_slide;
    let do_vol = !engine.state.channels[channel].active_effects.tremolo;

    if do_filter {
        let slide = engine.state.channels[channel].last_filter_cutoff_slide as f32;
        let new_cutoff =
            (engine.state.channels[channel].filter_cutoff + slide).clamp(0.0, 0xFFFF as f32);
        engine.state.channels[channel].filter_cutoff = new_cutoff;
        for voice in &mut engine.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.filter_cutoff = new_cutoff;
            }
        }
    }

    if do_vol {
        let vol = engine.compute_channel_volume(channel);
        for voice in &mut engine.voice_pool.voices {
            if voice.active && voice.channel == Some(channel) {
                voice.base_volume = vol;
            }
        }
    }
}
