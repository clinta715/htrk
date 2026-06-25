use std::sync::Arc;

use crate::audio::effects::EffectProcessor;
use crate::audio::sequencer::clock::SequencerClock;
use crate::audio::voice_pool::VoicePool;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::sequencer::module::Module;
use crate::sequencer::module::ModuleFormat;
use crate::sequencer::player::{ChannelState, SequencerState};
use crate::debug_log;

#[cfg(test)]
use crate::sequencer::pattern::Cell;
#[cfg(test)]
use crate::sequencer::sample::Sample;

// Sub-modules — one per major functional area
mod helpers;
mod cell;
mod period;
mod advance;
#[cfg(test)]
mod tests;

pub struct SequencerEngine {
    pub state: SequencerState,
    pub voice_pool: VoicePool,
    pub module: Option<Arc<Module>>,
    pub(crate) output_sample_rate: f64,
    pub(crate) use_xm_model: bool,
    pub(crate) amiga_led_filter: bool,
    pub pending_send_fx_params: Vec<(usize, u32, f32)>,
    /// Pending plugin-param automation values, populated by
    /// `apply_automation_to_channel` and drained by the audio engine
    /// via `collect_plugin_param_automation`. Each entry is
    /// `(send_bus, param_id, value)`.
    pub pending_plugin_param_changes: Vec<(u8, u32, f32)>,
    /// Pending note events for instrument plugin processors, populated by
    /// `process_cell_unified` and drained by the audio engine via
    /// `collect_plugin_note_events`.
    pub pending_plugin_note_events: Vec<PluginNoteEvent>,
    processor: EffectProcessor,
}

/// A note event queued by the sequencer for delivery to an instrument
/// plugin processor in the audio callback.
#[derive(Clone, Debug)]
pub struct PluginNoteEvent {
    pub instrument_idx: u8,
    pub midi_channel: u8,
    pub key: u8,
    pub velocity: u8,
    pub note_on: bool,
}

impl SequencerEngine {
    pub fn new(output_sample_rate: f64) -> Self {
        let default_module = Module::default();
        let processor = EffectProcessor::from_module(&default_module);
        SequencerEngine {
            state: SequencerState::default(),
            voice_pool: VoicePool::new(),
            module: None,
            output_sample_rate,
            use_xm_model: false,
            amiga_led_filter: false,
            pending_send_fx_params: Vec::new(),
            pending_plugin_param_changes: Vec::new(),
            pending_plugin_note_events: Vec::new(),
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
        self.state.clock = SequencerClock::new(module.initial_bpm, module.initial_speed, self.output_sample_rate);
        self.state.global_volume = module.initial_global_volume;
        self.state.master_volume = 1.0;

        let num_ch = module.channel_panning.len();
        self.state.channels.clear();
        self.state.channels.resize(num_ch, ChannelState::default());
        for i in 0..num_ch {
            self.state.channels[i].channel_panning = module.channel_panning[i];
            self.state.channels[i].channel_volume = module.channel_volume[i];
        }

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
        self.state.clock.current_tick = 0;
        self.state.clock.sample_counter = self.state.clock.samples_per_tick;
    }

    pub fn play_from(&mut self, order: u16, row: u16) {
        if self.module.is_none() {
            return;
        }
        self.stop_playback_state();

        let module = self.module.as_ref().unwrap();
        self.state.clock = SequencerClock::new(module.initial_bpm, module.initial_speed, self.output_sample_rate);
        self.state.global_volume = module.initial_global_volume;
        self.state.master_volume = 1.0;

        let num_ch = module.channel_panning.len();
        self.state.channels.clear();
        self.state.channels.resize(num_ch, ChannelState::default());
        for i in 0..num_ch {
            self.state.channels[i].channel_panning = module.channel_panning[i];
            self.state.channels[i].channel_volume = module.channel_volume[i];
        }

        let max_order = self.get_order_count().saturating_sub(1) as u16;
        self.state.current_order = order.min(max_order);
        self.state.current_row = row;
        self.state.current_pattern = self.get_pattern_for_order(self.state.current_order);
        self.state.clock.current_tick = 0;
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
        self.state.clock.current_tick = 0;
        self.state.clock.sample_counter = self.state.clock.samples_per_tick;
    }

    pub fn stop(&mut self) {
        self.stop_playback_state();
        for voice in &mut self.voice_pool.voices {
            voice.deactivate();
        }
    }

    fn stop_playback_state(&mut self) {
        self.state.playing = false;
        self.state.paused = false;
        self.state.clock.reset();
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
            let samples_per_tick = self.state.clock.samples_per_tick;
            if samples_per_tick <= 0.0 {
                break;
            }
            let samples_until_tick = (samples_per_tick - self.state.clock.sample_counter).ceil() as usize;
            if samples_until_tick == 0 {
                self.process_tick();
                self.state.clock.sample_counter -= samples_per_tick;
                continue;
            }

            if samples_remaining < samples_until_tick {
                self.state.clock.sample_counter += samples_remaining as f64;
                break;
            }

            samples_remaining -= samples_until_tick;
            self.process_tick();
            self.state.clock.sample_counter -= samples_per_tick;
        }
    }

    pub fn process_tick(&mut self) {
        self.evaluate_automation();

        let tick = self.state.clock.current_tick;

        if tick == 0 {
            self.process_tick_zero_unified();
        } else {
            self.process_effects_tick_unified();
        }

        self.advance_envelopes();

        if self.state.clock.on_tick_processed() {
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
        let tick = self.state.clock.current_tick;
        let speed = self.state.clock.speed;

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

    /// Collect pending plugin-param automation values from the most
    /// recent `process_automation` pass. The audio engine calls this
    /// after `process_tick` to route values to the appropriate
    /// `HostedPluginProcessor`'s param ring. The Vec is cleared on
    /// each call.
    pub fn collect_plugin_param_automation(
        &mut self,
    ) -> Vec<(u8, u32, f32)> {
        std::mem::take(&mut self.pending_plugin_param_changes)
    }

    /// Drain all pending plugin note events. Called by the audio engine
    /// on each tick after process_tick().
    pub fn collect_plugin_note_events(&mut self) -> Vec<PluginNoteEvent> {
        std::mem::take(&mut self.pending_plugin_note_events)
    }

    fn apply_automation_to_channel(&mut self, ch: usize, target: &crate::sequencer::automation::AutomationTarget, value: f32) {
        use crate::sequencer::automation::AutomationTarget;
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
            AutomationTarget::PluginParam { send_bus, param_id, .. } => {
                self.pending_plugin_param_changes.push((*send_bus, *param_id, value));
            }
            _ => {}
        }
    }

    fn apply_automation_global(&mut self, target: &crate::sequencer::automation::AutomationTarget, value: f32) {
        use crate::sequencer::automation::AutomationTarget;
        match target {
            AutomationTarget::GlobalVolume => {
                self.state.auto_global_vol_factor = value;
            }
            AutomationTarget::Tempo => {
                self.state.clock.auto_tempo_factor = value;
            }
            _ => {}
        }
    }

    // ─── Effect dispatch ───────────────────────────────────────

    fn apply_effect_unified(&mut self, channel: usize, effect: &crate::sequencer::effect::Effect, is_row_start: bool) {
        self.with_processor_mut(|processor, engine| processor.apply_effect(engine, channel, effect, is_row_start));
    }

    fn with_processor_mut<R>(&mut self, f: impl FnOnce(&mut EffectProcessor, &mut Self) -> R) -> R {
        let mut saved = std::mem::replace(&mut self.processor, EffectProcessor::placeholder());
        let result = f(&mut saved, self);
        self.processor = saved;
        result
    }

    pub(crate) fn process_effects_tick_unified(&mut self) {
        let tick = self.state.clock.current_tick;
        self.with_processor_mut(|processor, engine| processor.process_tick(engine, tick));
    }
}
