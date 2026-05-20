mod xm;
mod legacy;

use crate::audio::voice::Voice;
use crate::sequencer::effect::Effect;
use crate::sequencer::module::Module;
use crate::sequencer::pattern::Cell;
use crate::sequencer::player::{ChannelState, SequencerState};
use crate::sequencer::sample::Sample;

pub struct EffectContext<'a> {
    pub channels: &'a mut [ChannelState],
    pub voices: &'a mut [Voice],
    pub module: &'a Module,
    pub sample_rate: f64,
    pub global_volume: &'a mut f32,
    pub state: &'a mut SequencerState,
}

pub enum EffectProcessor {
    Xm(xm::XmProcessor),
    Legacy(legacy::LegacyProcessor),
}

impl EffectProcessor {
    pub fn from_module(module: &Module) -> Self {
        if module.flags.xm_period_model {
            EffectProcessor::Xm(xm::XmProcessor::new())
        } else {
            EffectProcessor::Legacy(legacy::LegacyProcessor::new())
        }
    }

    pub fn apply_effect(&mut self, ctx: &mut EffectContext, channel: usize, effect: &Effect, is_row_start: bool) {
        match self {
            EffectProcessor::Xm(p) => p.apply_effect(ctx, channel, effect, is_row_start),
            EffectProcessor::Legacy(p) => p.apply_effect(ctx, channel, effect, is_row_start),
        }
    }

    pub fn process_tick(&mut self, ctx: &mut EffectContext, tick: u8) {
        match self {
            EffectProcessor::Xm(p) => p.process_tick(ctx, tick),
            EffectProcessor::Legacy(p) => p.process_tick(ctx, tick),
        }
    }

    pub fn trigger_note(
        &mut self,
        ctx: &mut EffectContext,
        channel: usize,
        note_key: u8,
        remapped_key: u8,
        sample: Option<&Sample>,
        sample_idx: usize,
        cell: &Cell,
        instrument_idx: usize,
    ) {
        match self {
            EffectProcessor::Xm(p) => p.trigger_note(ctx, channel, note_key, remapped_key, sample, sample_idx, cell, instrument_idx),
            EffectProcessor::Legacy(p) => p.trigger_note(ctx, channel, note_key, remapped_key, sample, sample_idx, cell, instrument_idx),
        }
    }

    pub fn trigger_delayed_note(&mut self, ctx: &mut EffectContext, channel: usize) {
        match self {
            EffectProcessor::Xm(p) => p.trigger_delayed_note(ctx, channel),
            EffectProcessor::Legacy(p) => p.trigger_delayed_note(ctx, channel),
        }
    }

    pub fn process_volume_column(&mut self, ctx: &mut EffectContext, channel: usize, vol: u8) {
        match self {
            EffectProcessor::Xm(p) => p.process_volume_column(ctx, channel, vol),
            EffectProcessor::Legacy(p) => p.process_volume_column(ctx, channel, vol),
        }
    }

    pub fn handle_note_off(&mut self, ctx: &mut EffectContext, channel: usize) {
        match self {
            EffectProcessor::Xm(p) => p.handle_note_off(ctx, channel),
            EffectProcessor::Legacy(p) => p.handle_note_off(ctx, channel),
        }
    }
}
