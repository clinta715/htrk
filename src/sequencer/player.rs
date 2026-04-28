use crate::sequencer::effect::Effect;
use crate::sequencer::note::Note;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelState {
    pub last_note: Note,
    pub last_instrument: u8,
    pub last_sample: u8,

    pub channel_volume: u8,
    pub row_volume: u8,

    pub channel_panning: u8,

    pub last_effect: Effect,
    pub last_portamento_up_speed: u8,
    pub last_portamento_down_speed: u8,
    pub last_tone_portamento_speed: u8,
    pub last_vibrato_speed: u8,
    pub last_vibrato_depth: u8,
    pub last_tremolo_speed: u8,
    pub last_tremolo_depth: u8,
    pub last_volume_slide_up: u8,
    pub last_volume_slide_down: u8,
    pub last_sample_offset: u16,
    pub last_arpeggio: (u8, u8),
    pub last_retrigger_interval: u8,
    pub last_panbrello_speed: u8,
    pub last_panbrello_depth: u8,
    pub tremor_ontime: u8,
    pub tremor_offtime: u8,
    pub tremor_counter: u8,
    pub tremor_active: bool,
    pub glissando: bool,
    pub fine_tune_offset: i8,

    pub portamento_target_period: Option<f64>,

    pub muted: bool,
    pub solo: bool,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState {
            last_note: Note::None,
            last_instrument: 0,
            last_sample: 0,
            channel_volume: 64,
            row_volume: 64,
            channel_panning: 32,
            last_effect: Effect::None,
            last_portamento_up_speed: 0,
            last_portamento_down_speed: 0,
            last_tone_portamento_speed: 0,
            last_vibrato_speed: 0,
            last_vibrato_depth: 0,
            last_tremolo_speed: 0,
            last_tremolo_depth: 0,
            last_volume_slide_up: 0,
            last_volume_slide_down: 0,
            last_sample_offset: 0,
            last_arpeggio: (0, 0),
            last_retrigger_interval: 0,
            last_panbrello_speed: 0,
            last_panbrello_depth: 0,
            tremor_ontime: 0,
            tremor_offtime: 0,
            tremor_counter: 0,
            tremor_active: true,
            glissando: false,
            fine_tune_offset: 0,
            portamento_target_period: None,
            muted: false,
            solo: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PlayMode {
    Once,
    Loop,
    Pattern,
    Order,
}

impl Default for PlayMode {
    fn default() -> Self {
        PlayMode::Once
    }
}

#[derive(Clone, Debug)]
pub struct SequencerState {
    pub current_order: u16,
    pub current_row: u8,
    pub current_pattern: u8,
    pub current_tick: u8,

    pub bpm: u16,
    pub speed: u8,
    pub samples_per_tick: f64,
    pub sample_counter: f64,

    pub global_volume: u8,
    pub master_volume: f32,
    pub playing: bool,
    pub paused: bool,

    pub pattern_break_row: Option<u8>,
    pub position_jump_order: Option<u8>,
    pub pattern_delay_ticks: u8,
    pub row_delay_active: bool,
    pub pattern_loop_start: Option<(u16, u8)>,
    pub pattern_loop_count: u8,

    pub channels: Vec<ChannelState>,

    pub play_mode: PlayMode,
}

impl Default for SequencerState {
    fn default() -> Self {
        SequencerState {
            current_order: 0,
            current_row: 0,
            current_pattern: 0,
            current_tick: 0,
            bpm: 125,
            speed: 6,
            samples_per_tick: 0.0,
            sample_counter: 0.0,
            global_volume: 128,
            master_volume: 1.0,
            playing: false,
            paused: false,
            pattern_break_row: None,
            position_jump_order: None,
            pattern_delay_ticks: 0,
            row_delay_active: false,
            pattern_loop_start: None,
            pattern_loop_count: 0,
            channels: vec![ChannelState::default(); 64],
            play_mode: PlayMode::Once,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_state_default() {
        let cs = ChannelState::default();
        assert_eq!(cs.channel_volume, 64);
        assert_eq!(cs.channel_panning, 32);
        assert!(!cs.muted);
        assert!(!cs.solo);
        assert_eq!(cs.last_note, Note::None);
    }

    #[test]
    fn sequencer_state_default() {
        let ss = SequencerState::default();
        assert_eq!(ss.bpm, 125);
        assert_eq!(ss.speed, 6);
        assert!(!ss.playing);
        assert!(!ss.paused);
        assert_eq!(ss.channels.len(), 64);
        assert_eq!(ss.play_mode, PlayMode::Once);
    }

    #[test]
    fn play_mode_variants() {
        assert_ne!(PlayMode::Once, PlayMode::Loop);
        assert_ne!(PlayMode::Loop, PlayMode::Pattern);
        assert_ne!(PlayMode::Pattern, PlayMode::Order);
    }
}
