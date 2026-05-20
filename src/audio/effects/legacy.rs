use super::EffectProcessor;
use crate::sequencer::effect::Effect;
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::Sample;

pub struct LegacyProcessor;

impl LegacyProcessor {
    pub fn new() -> Self {
        LegacyProcessor
    }

    pub fn apply_effect(&mut self, _engine: &mut crate::audio::sequencer_engine::SequencerEngine, _channel: usize, _effect: &Effect, _is_row_start: bool) {
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
