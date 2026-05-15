#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(dead_code)]
pub enum Effect {
    #[default]
    None,

    Arpeggio { note1: u8, note2: u8 },
    PortamentoUp { speed: u8 },
    PortamentoDown { speed: u8 },
    TonePortamento { speed: u8 },
    Vibrato { speed: u8, depth: u8 },
    TonePortamentoVolumeSlide { up: i8 },
    VibratoVolumeSlide { up: i8 },
    Tremolo { speed: u8, depth: u8 },
    SetPanning { pan: u8 },
    SetSampleOffset { offset: u16 },
    VolumeSlide { up: u8, down: u8 },
    PositionJump { order: u8 },
    SetVolume { volume: u8 },
    PatternBreak { row: u8 },
    ExtendedEffect { param: u8 },
    SetSpeed { speed: u8 },
    SetTempo { bpm: u8 },

    SetGlobalVolume { volume: u8 },
    GlobalVolumeSlide { up: i8, down: i8 },
    SetEnvelopePosition { tick: u16 },
    Panbrello { speed: u8, depth: u8 },
    PatternDelay { ticks: u8 },
    SetPanPosition { pan: u8 },
    PanningSlide { speed: i8 },

    GlissandoControl { on: bool },
    VibratoWaveform { waveform: u8 },
    SetFineTune { tune: u8 },
    PatternLoop { count: u8 },
    TremoloWaveform { waveform: u8 },
    SetPanning16 { pan: u8 },
    Retrigger { interval: u8 },
    NoteCutAfter { ticks: u8 },
    NoteDelay { ticks: u8 },

    FinePortamentoUp { speed: u8 },
    FinePortamentoDown { speed: u8 },
    FineVolumeSlideUp { amount: u8 },
    FineVolumeSlideDown { amount: u8 },
    Tremor { ontime: u8, offtime: u8 },

    VolSetVolume { vol: u8 },
    VolFineSlideUp { amount: u8 },
    VolFineSlideDown { amount: u8 },
    VolSlideUp { amount: u8 },
    VolSlideDown { amount: u8 },
    VolPortamento { speed: u8 },
    VolVibrato { speed: u8 },

    SetFilterCutoff { cutoff: u16 },
    SetFilterResonance { resonance: u8 },
    SetFilterType { filter_type: u8 },
    FilterCutoffSlide { amount: i16 },

    FormatSpecific(FormatEffect),
}

/// Format-specific effects that cannot be fully represented in the universal Effect enum
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FormatEffect {
    /// XM format-specific effects
    Xm(XmEffect),
    /// MOD format-specific effects  
    Mod(ModEffect),
    /// S3M format-specific effects
    S3m(S3mEffect),
    /// IT format-specific effects
    It(ItEffect),
}

impl FormatEffect {
    pub fn sample_offset(&self) -> Option<u16> {
        match self {
            FormatEffect::Xm(XmEffect::SetSampleOffset(offset)) => Some(*offset),
            FormatEffect::S3m(S3mEffect::SetSampleOffset(offset)) => Some(*offset),
            FormatEffect::It(ItEffect::SetSampleOffset(offset)) => Some(*offset),
            _ => None,
        }
    }
}

/// XM-specific effects that require special handling
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum XmEffect {
    SetSampleOffset(u16),
    Panbrello(u8),
    VolumeColumn(u8),
    FineTonePortamento(u8),
    GlobalVolumeSlide { fine: bool, up: bool, amount: u8 },
    KeyOff { fade_rate: u8 },
    Raw { effect: u8, param: u8 },
}

/// MOD-specific effects
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModEffect {
    Filter(bool),
    FunkIt { speed: u8 },
    KarplusStrong { param: u8 },
    Raw { effect: u8, param: u8 },
}

/// S3M-specific effects
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum S3mEffect {
    SetSampleOffset(u16),
    Raw { effect: u16, param: u8 },
}

/// IT-specific effects
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ItEffect {
    SetSampleOffset(u16),
    Raw { effect: u8, param: u8 },
}

/// Supported module formats
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FormatType {
    #[default]
    Unknown,
    Mod,
    Xm,
    S3m,
    It,
    Htk,  // Native HTRK format
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FilterType {
    #[default]
    LowPass,
    HighPass,
    BandPass,
    Notch,
}

impl FilterType {
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => FilterType::HighPass,
            2 => FilterType::BandPass,
            3 => FilterType::Notch,
            _ => FilterType::LowPass,
        }
    }

    pub fn to_u8(&self) -> u8 {
        match self {
            FilterType::LowPass => 0,
            FilterType::HighPass => 1,
            FilterType::BandPass => 2,
            FilterType::Notch => 3,
        }
    }
}

pub fn effect_param_value(effect: &Effect) -> Option<u8> {
    match effect {
        Effect::Arpeggio { note1, note2 } => Some((*note1 << 4) | note2),
        Effect::PortamentoUp { speed } => Some(*speed),
        Effect::PortamentoDown { speed } => Some(*speed),
        Effect::TonePortamento { speed } => Some(*speed),
        Effect::Vibrato { speed, depth } => Some((*speed << 4) | depth),
        Effect::VolumeSlide { up, down } => Some((*up << 4) | down),
        Effect::PositionJump { order } => Some(*order),
        Effect::SetVolume { volume } => Some(*volume),
        Effect::PatternBreak { row } => Some(*row),
        Effect::ExtendedEffect { param } => Some(*param),
        Effect::SetSpeed { speed } => Some(*speed),
        Effect::SetTempo { bpm } => Some(*bpm),
        Effect::SetGlobalVolume { volume } => Some(*volume),
        Effect::SetPanning { pan } => Some(*pan),
        Effect::SetPanPosition { pan } => Some(*pan),
        Effect::SetSampleOffset { offset } => Some((*offset >> 8) as u8),
        Effect::Tremolo { speed, depth } => Some((*speed << 4) | depth),
        Effect::PatternDelay { ticks } => Some(*ticks),
        Effect::Panbrello { speed, depth } => Some((*speed << 4) | depth),
        Effect::Tremor { ontime, offtime } => Some((*ontime << 4) | offtime),
        Effect::Retrigger { interval } => Some(*interval),
        Effect::NoteCutAfter { ticks } => Some(*ticks),
        Effect::NoteDelay { ticks } => Some(*ticks),
        Effect::FinePortamentoUp { speed } => Some(*speed),
        Effect::FinePortamentoDown { speed } => Some(*speed),
        Effect::FineVolumeSlideUp { amount } => Some(*amount),
        Effect::FineVolumeSlideDown { amount } => Some(*amount),
        Effect::SetFilterCutoff { cutoff } => Some((*cutoff >> 8) as u8),
        Effect::SetFilterResonance { resonance } => Some(*resonance),
        Effect::SetFilterType { filter_type } => Some(*filter_type),
        Effect::PanningSlide { speed } => Some(speed.unsigned_abs()),
        _ => None,
    }
}

use crate::sequencer::pattern::Cell;

pub fn set_effect_param_value(mut cell: Cell, val: u8) -> Cell {
    match &mut cell.effect {
        Effect::PortamentoUp { speed } => *speed = val,
        Effect::PortamentoDown { speed } => *speed = val,
        Effect::TonePortamento { speed } => *speed = val,
        Effect::SetSpeed { speed } => *speed = val,
        Effect::SetTempo { bpm } => *bpm = val,
        Effect::SetVolume { volume } => *volume = val,
        Effect::PositionJump { order } => *order = val,
        Effect::SetGlobalVolume { volume } => *volume = val,
        Effect::SetPanning { pan } => *pan = val,
        Effect::ExtendedEffect { param } => *param = val,
        Effect::PatternBreak { row } => *row = val,
        Effect::PanningSlide { speed } => *speed = val as i8,
        _ => {}
    }
    cell
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_default_is_none() {
        assert_eq!(Effect::default(), Effect::None);
    }

    #[test]
    fn effect_equality() {
        assert_eq!(Effect::PortamentoUp { speed: 5 }, Effect::PortamentoUp { speed: 5 });
        assert_ne!(Effect::PortamentoUp { speed: 5 }, Effect::PortamentoUp { speed: 6 });
    }

    #[test]
    fn effect_clone_copy() {
        let e = Effect::Vibrato { speed: 4, depth: 8 };
        let e2 = e;
        assert_eq!(e, e2);
    }

    #[test]
    fn all_volume_column_effects() {
        let effects = [
            Effect::VolSetVolume { vol: 64 },
            Effect::VolFineSlideUp { amount: 5 },
            Effect::VolFineSlideDown { amount: 5 },
            Effect::VolSlideUp { amount: 5 },
            Effect::VolSlideDown { amount: 5 },
            Effect::VolPortamento { speed: 5 },
            Effect::VolVibrato { speed: 5 },
        ];
        for e in &effects {
            let cloned = e.clone();
            assert_eq!(*e, cloned);
        }
    }
}
