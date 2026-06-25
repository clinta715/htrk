#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NewNoteAction {
    NoteCut,
    Continue,
    NoteOff,
    NoteFade,
}

impl Default for NewNoteAction {
    fn default() -> Self {
        NewNoteAction::NoteCut
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DuplicateCheckType {
    Disabled,
    Note,
    Sample,
    Instrument,
}

impl Default for DuplicateCheckType {
    fn default() -> Self {
        DuplicateCheckType::Disabled
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DuplicateCheckAction {
    NoteCut,
    NoteOff,
    NoteFade,
}

impl Default for DuplicateCheckAction {
    fn default() -> Self {
        DuplicateCheckAction::NoteCut
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnvelopeFlags {
    pub enabled: bool,
    pub sustain: bool,
    pub loop_: bool,
    pub carry: bool,
}

mod array_u8_120_serde {
    use serde::de::{self, Deserializer};
    use serde::ser::Serializer;
    use serde::{Deserialize, Serialize};

    pub fn serialize<S>(value: &[u8; 120], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 120], D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: Vec<u8> = Vec::deserialize(deserializer)?;
        if v.len() != 120 {
            return Err(de::Error::custom(format!(
                "expected 120 elements, got {}",
                v.len()
            )));
        }
        let mut arr = [0u8; 120];
        arr.copy_from_slice(&v);
        Ok(arr)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnvelopePoint {
    pub tick: u16,
    pub value: u8,
}

impl Default for EnvelopePoint {
    fn default() -> Self {
        EnvelopePoint { tick: 0, value: 0 }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Envelope {
    pub points: Vec<EnvelopePoint>,
    pub sustain_point: Option<usize>,
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
    pub flags: EnvelopeFlags,
}

impl Default for Envelope {
    fn default() -> Self {
        Envelope {
            points: Vec::new(),
            sustain_point: None,
            loop_start: None,
            loop_end: None,
            flags: EnvelopeFlags::default(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Instrument {
    pub name: String,

    #[serde(with = "array_u8_120_serde")]
    pub sample_map: [u8; 120],
    #[serde(with = "array_u8_120_serde")]
    pub note_map: [u8; 120],

    pub volume_envelope: Option<Envelope>,
    pub panning_envelope: Option<Envelope>,
    pub pitch_envelope: Option<Envelope>,
    #[serde(default)]
    pub filter_envelope: Option<Envelope>,

    pub fade_out: u16,

    pub nna: NewNoteAction,
    pub duplicate_check_type: DuplicateCheckType,
    pub duplicate_check_action: DuplicateCheckAction,

    pub pitch_pan_separation: i8,
    pub pitch_pan_center: u8,

    pub global_volume: u8,

    #[serde(default)]
    pub filter_cutoff: u16,
    #[serde(default)]
    pub filter_resonance: u8,
    #[serde(default)]
    pub filter_type: super::effect::FilterType,

    pub random_volume: u8,
    pub random_panning: u8,
    #[serde(default)]
    pub filter_random_cutoff: u8,

    pub vib_type: u8,
    pub vib_sweep: u8,
    pub vib_depth: u8,
    pub vib_rate: u8,

    /// Optional plugin backing. `None` = traditional sample instrument.
    #[serde(default)]
    pub plugin: Option<crate::sequencer::plugin::PluginSlot>,

    /// Base MIDI channel for multi-timbral routing (0–15).
    /// When multiple sequencer channels use the same plugin instrument,
    /// they are distinguished by `midi_base_channel + channel_index`.
    #[serde(default = "default_midi_channel")]
    pub midi_base_channel: u8,

    /// Parameter macros: tracker column values that drive instrument
    /// plugin parameters. When the sequencer processes a cell that
    /// uses this instrument and has a value for one of the macro
    /// sources, the value is remapped to the macro's range and
    /// written to the corresponding plugin parameter.
    ///
    /// Currently only the cell's volume column (0–64) is supported
    /// as a macro source; the column is normalized to 0.0–1.0 and
    /// linearly remapped to `[range_min, range_max]`. This lets
    /// tracker composers use the volume column as a "modulation
    /// wheel" for synth parameters (e.g. filter cutoff, resonance,
    /// vibrato amount) without writing automation lanes.
    #[serde(default)]
    pub macros: Vec<ParameterMacro>,
}

/// Source of a value used by a `ParameterMacro`. Currently only
/// the cell's volume column is supported, but the enum leaves
/// room for additional sources (panning, filter cutoff, etc.)
/// in future phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MacroSource {
    /// The cell's volume column value, 0–64.
    Volume,
}

/// A single tracker-column → plugin-parameter mapping.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ParameterMacro {
    pub source: MacroSource,
    /// The plugin's stable `ClapId` for the parameter to drive.
    pub param_id: u32,
    /// Minimum value of the remapped range (typically the plugin
    /// param's `min`).
    pub range_min: f32,
    /// Maximum value of the remapped range (typically the plugin
    /// param's `max`).
    pub range_max: f32,
}

impl Default for ParameterMacro {
    fn default() -> Self {
        Self {
            source: MacroSource::Volume,
            param_id: 0,
            range_min: 0.0,
            range_max: 1.0,
        }
    }
}

impl Default for Instrument {
    fn default() -> Self {
        Instrument {
            name: String::new(),
            sample_map: [0u8; 120],
            note_map: {
                let mut m = [0u8; 120];
                for i in 0..120 { m[i] = i as u8; }
                m
            },
            volume_envelope: None,
            panning_envelope: None,
            pitch_envelope: None,
            filter_envelope: None,
            fade_out: 0,
            nna: NewNoteAction::default(),
            duplicate_check_type: DuplicateCheckType::default(),
            duplicate_check_action: DuplicateCheckAction::default(),
            pitch_pan_separation: 0,
            pitch_pan_center: 60,
            global_volume: 128,
            filter_cutoff: 0xFFFF,
            filter_resonance: 0,
            filter_type: super::effect::FilterType::default(),
            random_volume: 0,
            random_panning: 0,
            filter_random_cutoff: 0,
            vib_type: 0,
            vib_sweep: 0,
            vib_depth: 0,
            vib_rate: 0,
            plugin: None,
            midi_base_channel: 0,
            macros: Vec::new(),
        }
    }
}

fn default_midi_channel() -> u8 { 0 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_default() {
        let inst = Instrument::default();
        assert!(inst.name.is_empty());
        assert_eq!(inst.sample_map, [0u8; 120]);
        assert!(inst.volume_envelope.is_none());
        assert_eq!(inst.fade_out, 0);
        assert_eq!(inst.nna, NewNoteAction::NoteCut);
        assert_eq!(inst.global_volume, 128);
    }

    #[test]
    fn envelope_with_points() {
        let env = Envelope {
            points: vec![
                EnvelopePoint { tick: 0, value: 0 },
                EnvelopePoint { tick: 10, value: 64 },
            ],
            sustain_point: Some(1),
            loop_start: None,
            loop_end: None,
            flags: EnvelopeFlags {
                enabled: true,
                sustain: true,
                loop_: false,
                carry: false,
            },
        };
        assert_eq!(env.points.len(), 2);
        assert_eq!(env.sustain_point, Some(1));
        assert!(env.flags.enabled);
    }

    #[test]
    fn nna_variants() {
        assert_ne!(NewNoteAction::NoteCut, NewNoteAction::Continue);
        assert_ne!(NewNoteAction::Continue, NewNoteAction::NoteOff);
        assert_ne!(NewNoteAction::NoteOff, NewNoteAction::NoteFade);
    }
}
