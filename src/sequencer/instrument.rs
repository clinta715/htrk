#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EnvelopeFlags {
    pub enabled: bool,
    pub sustain: bool,
    pub loop_: bool,
    pub carry: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvelopePoint {
    pub tick: u16,
    pub value: u8,
}

impl Default for EnvelopePoint {
    fn default() -> Self {
        EnvelopePoint { tick: 0, value: 0 }
    }
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct Instrument {
    pub name: String,

    pub sample_map: [u8; 120],
    pub note_map: [u8; 120],

    pub volume_envelope: Option<Envelope>,
    pub panning_envelope: Option<Envelope>,
    pub pitch_envelope: Option<Envelope>,

    pub fade_out: u16,

    pub nna: NewNoteAction,
    pub duplicate_check_type: DuplicateCheckType,
    pub duplicate_check_action: DuplicateCheckAction,

    pub pitch_pan_separation: i8,
    pub pitch_pan_center: u8,

    pub global_volume: u8,

    pub _cutoff: u16,
    pub _resonance: u8,

    pub random_volume: u8,
    pub random_panning: u8,
    pub _random_cutoff: u8,
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
            fade_out: 0,
            nna: NewNoteAction::default(),
            duplicate_check_type: DuplicateCheckType::default(),
            duplicate_check_action: DuplicateCheckAction::default(),
            pitch_pan_separation: 0,
            pitch_pan_center: 60,
            global_volume: 128,
            _cutoff: 0,
            _resonance: 0,
            random_volume: 0,
            random_panning: 0,
            _random_cutoff: 0,
        }
    }
}

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
