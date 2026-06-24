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
    PositionJump { order: u16 },
    SetVolume { volume: u8 },
    PatternBreak { row: u16 },
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

    ExtraFinePortamentoUp { speed: u8 },
    ExtraFinePortamentoDown { speed: u8 },
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

    SetSendLevel { send_index: u8, level: u8 },
    SetSendBusParam { bus: u8, param: u8, value: u8 },

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
    /// 669 format-specific effects
    C669(C669Effect),
    /// MMD format-specific effects
    Mmd(MmdEffect),
    /// ULT format-specific effects
    Ult(UltEffect),
    /// STM format-specific effects
    Stm(StmEffect),
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

/// 669-specific effects (Composer 669 / UNIS 669)
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum C669Effect {
    PortamentoUp { speed: u8 },
    PortamentoDown { speed: u8 },
    TonePortamento { speed: u8 },
    Finetune { tune: u8 },
    Vibrato { speed: u8, depth: u8 },
    SetSpeed { speed: u8 },
    Raw { effect: u8, param: u8 },
}

/// MMD (OctaMED) format-specific effects
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MmdEffect {
    SetTempo(u8),
    Retrigger(u8),
    VolumeSlide { up: bool, amount: u8 },
    Finetune { tune: u8 },
    Raw { effect: u8, param: u8 },
}

/// Ultra Tracker format-specific effects
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UltEffect {
    TonePortamento { speed: u8 },
    SampleOffset(u16),
    Panning { pan: u8 },
    SpeedBPM { value: u8 },
    Raw { effect: u8, param: u8 },
}

/// Scream Tracker 2 format-specific effects
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StmEffect {
    Raw { effect: u8, param: u8 },
}

pub const NUM_SEND_BUSES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum SendEffectType {
    #[default]
    None,
    Delay,
    Reverb,
    Chorus,
    Flanger,
    Phaser,
}

impl SendEffectType {
    pub fn name(&self) -> &'static str {
        match self {
            SendEffectType::None => "None",
            SendEffectType::Delay => "Stereo Delay",
            SendEffectType::Reverb => "Reverb",
            SendEffectType::Chorus => "Chorus",
            SendEffectType::Flanger => "Flanger",
            SendEffectType::Phaser => "Phaser",
        }
    }
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
        Effect::TonePortamentoVolumeSlide { up } => Some(up.unsigned_abs()),
        Effect::VibratoVolumeSlide { up } => Some(up.unsigned_abs()),
        Effect::Tremolo { speed, depth } => Some((*speed << 4) | depth),
        Effect::SetPanning { pan } => Some(*pan),
        Effect::SetSampleOffset { offset } => Some((*offset >> 8) as u8),
        Effect::VolumeSlide { up, down } => Some((*up << 4) | down),
        Effect::PositionJump { order } => Some(*order as u8),
        Effect::SetVolume { volume } => Some(*volume),
        Effect::PatternBreak { row } => Some(*row as u8),
        Effect::ExtendedEffect { param } => Some(*param),
        Effect::SetSpeed { speed } => Some(*speed),
        Effect::SetTempo { bpm } => Some(*bpm),
        Effect::SetGlobalVolume { volume } => Some(*volume),
        Effect::GlobalVolumeSlide { up, down } => Some(((*up as u8) << 4) | (*down as u8)),
        Effect::SetEnvelopePosition { tick } => Some(*tick as u8),
        Effect::Panbrello { speed, depth } => Some((*speed << 4) | depth),
        Effect::PatternDelay { ticks } => Some(*ticks),
        Effect::SetPanPosition { pan } => Some(*pan),
        Effect::PanningSlide { speed } => Some(speed.unsigned_abs()),
        Effect::GlissandoControl { on } => Some(if *on { 0x0F } else { 0x00 }),
        Effect::VibratoWaveform { waveform } => Some(*waveform),
        Effect::SetFineTune { tune } => Some(*tune),
        Effect::PatternLoop { count } => Some(*count),
        Effect::TremoloWaveform { waveform } => Some(*waveform),
        Effect::SetPanning16 { pan } => Some(*pan),
        Effect::Retrigger { interval } => Some(*interval),
        Effect::NoteCutAfter { ticks } => Some(*ticks),
        Effect::NoteDelay { ticks } => Some(*ticks),
        Effect::ExtraFinePortamentoUp { speed } => Some(*speed),
        Effect::ExtraFinePortamentoDown { speed } => Some(*speed),
        Effect::FinePortamentoUp { speed } => Some(*speed),
        Effect::FinePortamentoDown { speed } => Some(*speed),
        Effect::FineVolumeSlideUp { amount } => Some(*amount),
        Effect::FineVolumeSlideDown { amount } => Some(*amount),
        Effect::Tremor { ontime, offtime } => Some((*ontime << 4) | offtime),
        Effect::VolSetVolume { vol } => Some(*vol),
        Effect::VolFineSlideUp { amount } => Some(*amount),
        Effect::VolFineSlideDown { amount } => Some(*amount),
        Effect::VolSlideUp { amount } => Some(*amount),
        Effect::VolSlideDown { amount } => Some(*amount),
        Effect::VolPortamento { speed } => Some(*speed),
        Effect::VolVibrato { speed } => Some(*speed),
        Effect::SetFilterCutoff { cutoff } => Some((*cutoff >> 8) as u8),
        Effect::SetFilterResonance { resonance } => Some(*resonance),
        Effect::SetFilterType { filter_type } => Some(*filter_type),
        Effect::FilterCutoffSlide { amount } => Some(amount.unsigned_abs() as u8),
        Effect::SetSendLevel { send_index, level } => Some((*send_index << 4) | level),
        Effect::SetSendBusParam { bus, param, value: _ } => Some((*bus << 4) | param),
        Effect::None | Effect::FormatSpecific(_) => None,
    }
}

use crate::sequencer::pattern::Cell;

/// Construct the canonical "zero parameter" Effect for a given 0-F hex digit.
/// Mirrors the look-up in `hex_to_effect()` in keyboard.rs but is shared
/// with the right-click "Set Effect" submenu so the two paths agree.
pub fn effect_from_hex_digit(d: u8) -> Effect {
    match d {
        0 => Effect::Arpeggio { note1: 0, note2: 0 },
        1 => Effect::PortamentoUp { speed: 0 },
        2 => Effect::PortamentoDown { speed: 0 },
        3 => Effect::TonePortamento { speed: 0 },
        4 => Effect::Vibrato { speed: 0, depth: 0 },
        5 => Effect::TonePortamentoVolumeSlide { up: 0 },
        6 => Effect::VibratoVolumeSlide { up: 0 },
        7 => Effect::Tremolo { speed: 0, depth: 0 },
        8 => Effect::SetPanning { pan: 0 },
        9 => Effect::SetSampleOffset { offset: 0 },
        0xA => Effect::VolumeSlide { up: 0, down: 0 },
        0xB => Effect::PositionJump { order: 0 },
        0xC => Effect::SetVolume { volume: 0 },
        0xD => Effect::PatternBreak { row: 0 },
        0xE => Effect::ExtendedEffect { param: 0 },
        0xF => Effect::SetSpeed { speed: 0 },
        _ => Effect::None,
    }
}

/// Construct the named parameter-style effect that maps to the keyboard
/// letters P / Z / S / R / X. The `command` discriminant is defined by
/// `ParamEffectCommand` in the UI layer.
pub fn effect_from_param_command(command_kind: u8) -> Effect {
    // The discriminant values are arbitrary but must be stable across
    // the UI and the sequencer. They are NOT persisted.
    match command_kind {
        0 => Effect::SetSendBusParam { bus: 0, param: 0, value: 0 },
        1 => Effect::SetFilterCutoff { cutoff: 0 },
        2 => Effect::SetSendLevel { send_index: 0, level: 0 },
        3 => Effect::SetFilterResonance { resonance: 0 },
        4 => Effect::SetFilterType { filter_type: 0 },
        _ => Effect::None,
    }
}

pub fn set_effect_param_value(mut cell: Cell, val: u8) -> Cell {
    match &mut cell.effect {
        Effect::Arpeggio { note1, note2 } => { *note1 = val >> 4; *note2 = val & 0x0F; }
        Effect::PortamentoUp { speed } => *speed = val,
        Effect::PortamentoDown { speed } => *speed = val,
        Effect::TonePortamento { speed } => *speed = val,
        Effect::Vibrato { speed, depth } => { *speed = val >> 4; *depth = val & 0x0F; }
        Effect::TonePortamentoVolumeSlide { up } => *up = val as i8,
        Effect::VibratoVolumeSlide { up } => *up = val as i8,
        Effect::Tremolo { speed, depth } => { *speed = val >> 4; *depth = val & 0x0F; }
        Effect::SetPanning { pan } => *pan = val,
        Effect::SetSampleOffset { offset } => *offset = (val as u16) << 8,
        Effect::VolumeSlide { up, down } => { *up = val >> 4; *down = val & 0x0F; }
        Effect::PositionJump { order } => *order = val as u16,
        Effect::SetVolume { volume } => *volume = val,
        Effect::PatternBreak { row } => *row = val as u16,
        Effect::ExtendedEffect { param } => *param = val,
        Effect::SetSpeed { speed } => *speed = val,
        Effect::SetTempo { bpm } => *bpm = val,
        Effect::SetGlobalVolume { volume } => *volume = val,
        Effect::GlobalVolumeSlide { up, down } => { *up = (val >> 4) as i8; *down = (val & 0x0F) as i8; }
        Effect::SetEnvelopePosition { tick } => *tick = val as u16,
        Effect::Panbrello { speed, depth } => { *speed = val >> 4; *depth = val & 0x0F; }
        Effect::PatternDelay { ticks } => *ticks = val,
        Effect::SetPanPosition { pan } => *pan = val,
        Effect::PanningSlide { speed } => *speed = val as i8,
        Effect::GlissandoControl { on } => *on = (val & 0x0F) != 0,
        Effect::VibratoWaveform { waveform } => *waveform = val & 0x03,
        Effect::SetFineTune { tune } => *tune = val,
        Effect::PatternLoop { count } => *count = val,
        Effect::TremoloWaveform { waveform } => *waveform = val & 0x03,
        Effect::SetPanning16 { pan } => *pan = val,
        Effect::Retrigger { interval } => *interval = val,
        Effect::NoteCutAfter { ticks } => *ticks = val,
        Effect::NoteDelay { ticks } => *ticks = val,
        Effect::ExtraFinePortamentoUp { speed } => *speed = val,
        Effect::ExtraFinePortamentoDown { speed } => *speed = val,
        Effect::FinePortamentoUp { speed } => *speed = val,
        Effect::FinePortamentoDown { speed } => *speed = val,
        Effect::FineVolumeSlideUp { amount } => *amount = val,
        Effect::FineVolumeSlideDown { amount } => *amount = val,
        Effect::Tremor { ontime, offtime } => { *ontime = val >> 4; *offtime = val & 0x0F; }
        Effect::SetFilterCutoff { cutoff } => *cutoff = (val as u16) << 8,
        Effect::SetFilterResonance { resonance } => *resonance = val,
        Effect::SetFilterType { filter_type } => *filter_type = val,
        Effect::FilterCutoffSlide { amount } => *amount = val as i16,
        Effect::SetSendLevel { send_index, level } => { *send_index = val >> 4; *level = val & 0x0F; }
        Effect::SetSendBusParam { bus, param, value: _ } => { *bus = val >> 4; *param = val & 0x0F; }
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
