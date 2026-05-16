use crate::sequencer::effect::Effect;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::sequencer::effect::FilterType;

#[derive(Clone, Copy, Debug, Default)]
pub struct ActiveEffects {
    pub volume_slide: bool,
    pub portamento_up: bool,
    pub portamento_down: bool,
    pub tone_portamento: bool,
    pub vibrato: bool,
    pub tremolo: bool,
    pub arpeggio: bool,
    pub panbrello: bool,
    pub tremor: bool,
    pub key_off: bool,
    pub filter_cutoff_slide: bool,
    pub panning_slide: bool,
}

#[derive(Clone, Debug)]
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
    pub last_volume_slide_param: u8,
    pub last_sample_offset: u16,
    pub high_sample_offset: u8,
    pub last_arpeggio: (u8, u8),
    pub last_retrigger_interval: u8,
    pub last_panbrello_speed: u8,
    pub last_panbrello_depth: u8,
    pub last_panning_slide: i8,
    pub tremor_ontime: u8,
    pub tremor_offtime: u8,
    pub tremor_counter: u8,
    pub tremor_active: bool,
    pub glissando: bool,
    pub fine_tune_offset: i8,

    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub filter_type: FilterType,
    pub last_filter_cutoff_slide: i16,
    pub filter_enabled: bool,
    pub sample_loop_point: u32,

    pub portamento_target_period: Option<u16>,
    pub portamento_target_frequency: Option<f64>,

    pub muted: bool,
    pub solo: bool,

    pub delayed_cell: Option<Cell>,
    pub note_delay_ticks: u8,
    pub active_effects: ActiveEffects,

    // XM-specific fields — will be removed in Phase 4 (unified sequencer)
    pub real_period: u16,
    pub want_period: u16,
    pub out_period: u16,
    pub porta_speed_period: u16,
    pub porta_dir: u8,
    pub vib_pos: u8,
    pub trem_pos: u8,
    pub vib_speed: u8,
    pub vib_depth: u8,
    pub trem_speed: u8,
    pub trem_depth: u8,
    pub wave_ctrl: u8,
    pub retrig_cnt: u8,
    pub retrig_speed: u8,
    pub retrig_vol: u8,

    pub vol_kol: u8,
    pub rel_ton: i8,
    pub real_vol: u8,
    pub old_vol: u8,
    pub old_pan: u8,
    pub tremor_pos_byte: u8,
    pub note_cut_tick: Option<u8>,
    pub funk_speed: u8,
    pub funk_pos: u8,
    pub karplus_param: u8,

    pub send_levels: [f32; 4],
    pub last_send_param_value: [u8; NUM_SEND_BUSES * 4],
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
            last_volume_slide_param: 0,
            last_sample_offset: 0,
            high_sample_offset: 0,
            last_arpeggio: (0, 0),
            last_retrigger_interval: 0,
            last_panbrello_speed: 0,
            last_panbrello_depth: 0,
            last_panning_slide: 0,
            tremor_ontime: 0,
            tremor_offtime: 0,
            tremor_counter: 0,
            tremor_active: true,
            glissando: false,
            fine_tune_offset: 0,
            filter_cutoff: 0xFFFF as f32,
            filter_resonance: 0.0,
            filter_type: FilterType::LowPass,
            last_filter_cutoff_slide: 0,
            filter_enabled: true,
            sample_loop_point: 0,
            portamento_target_period: None,
            portamento_target_frequency: None,
            muted: false,
            solo: false,
            delayed_cell: None,
            note_delay_ticks: 0,
            active_effects: ActiveEffects::default(),
            real_period: 0,
            want_period: 0,
            out_period: 0,
            porta_speed_period: 0,
            porta_dir: 0,
            vib_pos: 0,
            trem_pos: 0,
            vib_speed: 0,
            vib_depth: 0,
            trem_speed: 0,
            trem_depth: 0,
            wave_ctrl: 0,
            retrig_cnt: 0,
            retrig_speed: 0,
            retrig_vol: 0,
            vol_kol: 0,
            rel_ton: 0,
            real_vol: 64,
            old_vol: 64,
            old_pan: 32,
            tremor_pos_byte: 0,
            note_cut_tick: None,
            funk_speed: 0,
            funk_pos: 0,
            karplus_param: 0,
            send_levels: [0.0; NUM_SEND_BUSES],
            last_send_param_value: [0; NUM_SEND_BUSES * 4],
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
    pub last_global_volume_up: u8,
    pub last_global_volume_down: u8,
    pub master_volume: f32,
    pub playing: bool,
    pub paused: bool,

    pub pattern_break_row: Option<u8>,
    pub position_jump_order: Option<u8>,
    pub position_jump_flag: bool,
    pub pattern_delay_ticks: u8,
    pub pattern_delay_ticks2: u8,
    pub row_delay_active: bool,
    pub pattern_loop_start: Option<(u16, u8)>,
    pub pattern_loop_count: u8,
    pub pattern_loop_final_pass: bool,
    pub pattern_loop_jump_target: Option<u8>,

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
            last_global_volume_up: 0,
            last_global_volume_down: 0,
            master_volume: 1.0,
            playing: false,
            paused: false,
            pattern_break_row: None,
            position_jump_order: None,
            position_jump_flag: false,
            pattern_delay_ticks: 0,
            pattern_delay_ticks2: 0,
            row_delay_active: false,
            pattern_loop_start: None,
            pattern_loop_count: 0,
            pattern_loop_final_pass: false,
            pattern_loop_jump_target: None,
            channels: vec![ChannelState::default(); crate::sequencer::module::DEFAULT_CHANNELS],
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
        assert_eq!(ss.channels.len(), crate::sequencer::module::DEFAULT_CHANNELS);
        assert_eq!(ss.play_mode, PlayMode::Once);
    }

    #[test]
    fn play_mode_variants() {
        assert_ne!(PlayMode::Once, PlayMode::Loop);
        assert_ne!(PlayMode::Loop, PlayMode::Pattern);
        assert_ne!(PlayMode::Pattern, PlayMode::Order);
    }
}

