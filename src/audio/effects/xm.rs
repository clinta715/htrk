use super::EffectContext;
use crate::sequencer::effect::Effect;
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::Sample;

pub struct XmProcessor;

impl XmProcessor {
    pub fn new() -> Self {
        XmProcessor
    }

    pub fn apply_effect(&mut self, _ctx: &mut EffectContext, _channel: usize, _effect: &Effect, _is_row_start: bool) {
    }

    pub fn process_tick(&mut self, _ctx: &mut EffectContext, _tick: u8) {
    }

    pub fn trigger_note(
        &mut self,
        _ctx: &mut EffectContext,
        _channel: usize,
        _note_key: u8,
        _remapped_key: u8,
        _sample: Option<&Sample>,
        _sample_idx: usize,
        _cell: &Cell,
        _instrument_idx: usize,
    ) {
    }

    pub fn trigger_delayed_note(&mut self, _ctx: &mut EffectContext, _channel: usize) {
    }

    pub fn process_volume_column(&mut self, ctx: &mut EffectContext, channel: usize, vol: u8) {
        let ch = &mut ctx.channels[channel];
        ch.vol_kol = vol;
        if vol <= 64 {
            ch.channel_volume = vol;
            ch.row_volume = vol;
        }
    }

    pub fn handle_note_off(&mut self, _ctx: &mut EffectContext, _channel: usize) {
    }
}
