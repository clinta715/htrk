use crate::sequencer::effect::SendEffectType;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::sequencer::instrument::Instrument;
use crate::sequencer::pattern::Pattern;
use crate::sequencer::sample::Sample;

fn default_send_bus_config() -> [SendEffectType; NUM_SEND_BUSES] {
    [SendEffectType::Delay, SendEffectType::Reverb, SendEffectType::None, SendEffectType::None]
}

fn default_send_return_levels() -> [f32; NUM_SEND_BUSES] {
    [0.5, 0.0, 0.0, 0.0]
}

pub const MAX_CHANNELS: usize = 64;
pub const DEFAULT_CHANNELS: usize = 4;
pub const MAX_VOICES: usize = 256;
#[allow(dead_code)]
pub const MAX_PATTERNS: usize = 256;
#[allow(dead_code)]
pub const MAX_SAMPLES: usize = 999;
#[allow(dead_code)]
pub const MAX_INSTRUMENTS: usize = 256;
#[allow(dead_code)]
pub const MAX_ORDER_LENGTH: usize = 1024;
pub const MAX_ENVELOPE_POINTS: usize = 25;
pub const MAX_PATTERN_ROWS: usize = 1024;

pub const DEFAULT_BPM: u16 = 125;
pub const DEFAULT_SPEED: u8 = 6;
pub const DEFAULT_GLOBAL_VOLUME: u8 = 128;
#[allow(dead_code)]
pub const DEFAULT_ROWS: usize = 64;
#[allow(dead_code)]
pub const DEFAULT_OCTAVE: u8 = 4;

pub const COMMAND_BUFFER_SIZE: usize = 256;

#[allow(dead_code)]
pub const VOLUME_MIN: u8 = 0;
pub const VOLUME_MAX: u8 = 64;
pub const PANNING_CENTER: u8 = 32;

pub const BASE_NOTE_RATE: f64 = 261.6255653005961;
#[allow(dead_code)]
pub const MIDDLE_C: u8 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModuleFormat {
    IT,
    XM,
    S3M,
    MOD,
    HTK,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModVariant {
    #[default]
    ProTracker,
    NoiseTracker,
    SoundTracker,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModuleFlags {
    pub stereo: bool,
    pub use_instruments: bool,
    pub linear_slides: bool,
    pub old_effects: bool,
    pub compatible_gxx: bool,
    pub midi_enabled: bool,
    pub request_embed: bool,
    pub fast_volume_slides: bool,
    #[allow(dead_code)]
    pub xm_envelope_model: bool,
    pub xm_period_model: bool,
    pub mod_variant: ModVariant,
    /// Compatible With Tracker version (IT format). 0 if not IT.
    pub compatible_tracker_version: u16,
    /// Stereo panning separation 0-128. 128 = full separation.
    pub panning_separation: u8,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Module {
    pub name: String,
    pub message: Option<String>,

    pub format: ModuleFormat,
    pub _version: u16,
    pub tracker_name: String,

    pub order_list: Vec<u8>,
    pub patterns: Vec<Pattern>,
    pub instruments: Vec<Instrument>,
    pub samples: Vec<Sample>,

    pub initial_bpm: u16,
    pub initial_speed: u8,
    pub initial_global_volume: u8,
    pub initial_mixing_volume: u8,

    pub channel_panning: Vec<u8>,
    pub channel_volume: Vec<u8>,

    pub flags: ModuleFlags,

    #[serde(default = "default_send_bus_config")]
    pub send_bus_config: [SendEffectType; NUM_SEND_BUSES],
    #[serde(default = "default_send_return_levels")]
    pub send_return_levels: [f32; NUM_SEND_BUSES],
}

impl Default for Module {
    fn default() -> Self {
        Module {
            name: String::new(),
            message: None,
            format: ModuleFormat::IT,
            _version: 0,
            tracker_name: String::new(),
            order_list: Vec::new(),
            patterns: Vec::new(),
            instruments: vec![Instrument::default(); 17],
            samples: vec![Sample::default(); 65],
            initial_bpm: DEFAULT_BPM,
            initial_speed: DEFAULT_SPEED,
            initial_global_volume: DEFAULT_GLOBAL_VOLUME,
            initial_mixing_volume: 128,
            channel_panning: vec![PANNING_CENTER; DEFAULT_CHANNELS],
            channel_volume: vec![VOLUME_MAX; DEFAULT_CHANNELS],
            flags: ModuleFlags::default(),
            send_bus_config: [SendEffectType::Delay, SendEffectType::Reverb, SendEffectType::None, SendEffectType::None],
            send_return_levels: [0.5, 0.0, 0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_default() {
        let m = Module::default();
        assert!(m.name.is_empty());
        assert!(m.message.is_none());
        assert_eq!(m.format, ModuleFormat::IT);
        assert_eq!(m.initial_bpm, DEFAULT_BPM);
        assert_eq!(m.initial_speed, DEFAULT_SPEED);
        assert_eq!(m.initial_global_volume, DEFAULT_GLOBAL_VOLUME);
        assert_eq!(m.channel_panning.len(), DEFAULT_CHANNELS);
        assert_eq!(m.channel_volume.len(), DEFAULT_CHANNELS);
    }

    #[test]
    fn module_with_pattern() {
        let mut m = Module::default();
        m.order_list = vec![0, 1, 0];
        m.patterns.push(Pattern::new(64));
        m.patterns.push(Pattern::new(32));
        assert_eq!(m.order_list.len(), 3);
        assert_eq!(m.patterns.len(), 2);
        assert_eq!(m.patterns[0].num_rows, 64);
        assert_eq!(m.patterns[1].num_rows, 32);
    }

    #[test]
    fn module_flags_default() {
        let f = ModuleFlags::default();
        assert!(!f.stereo);
        assert!(!f.use_instruments);
        assert!(!f.linear_slides);
    }
}
