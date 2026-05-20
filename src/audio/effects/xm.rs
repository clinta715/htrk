use super::EffectProcessor;
use crate::sequencer::effect::Effect;
use crate::sequencer::pattern::Cell;
use crate::sequencer::sample::Sample;

pub struct XmProcessor;

impl XmProcessor {
    pub fn new() -> Self {
        XmProcessor
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

    pub fn process_volume_column(&mut self, engine: &mut crate::audio::sequencer_engine::SequencerEngine, channel: usize, vol: u8) {
        use crate::sequencer::player::ChannelState;
        let ch = &mut engine.state.channels[channel];
        ch.vol_kol = vol;
        if vol <= 64 {
            ch.channel_volume = vol;
            ch.row_volume = vol;
        }
    }

    pub fn handle_note_off(&mut self, _engine: &mut crate::audio::sequencer_engine::SequencerEngine, _channel: usize) {
    }
}
