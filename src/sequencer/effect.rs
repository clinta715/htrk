#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Effect {
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
}

impl Default for Effect {
    fn default() -> Self {
        Effect::None
    }
}

impl Effect {
    pub fn effect_byte(&self) -> Option<u8> {
        match self {
            Effect::None => None,
            Effect::Arpeggio { note1, note2 } => Some((note1 << 4) | (note2 & 0x0F)),
            Effect::PortamentoUp { speed } => Some(*speed),
            Effect::PortamentoDown { speed } => Some(*speed),
            Effect::TonePortamento { speed } => Some(*speed),
            Effect::Vibrato { speed, depth } => Some((speed << 4) | (depth & 0x0F)),
            Effect::TonePortamentoVolumeSlide { up } => Some(*up as u8),
            Effect::VibratoVolumeSlide { up } => Some(*up as u8),
            Effect::Tremolo { speed, depth } => Some((speed << 4) | (depth & 0x0F)),
            Effect::SetPanning { pan } => Some(*pan),
            Effect::SetSampleOffset { offset } => Some((offset >> 8) as u8),
            Effect::VolumeSlide { up, down } => Some((up << 4) | (down & 0x0F)),
            Effect::PositionJump { order } => Some(*order),
            Effect::SetVolume { volume } => Some(*volume),
            Effect::PatternBreak { row } => Some(((row / 10) << 4) | (row % 10)),
            Effect::ExtendedEffect { param } => Some(*param),
            Effect::SetSpeed { speed } => Some(*speed),
            Effect::SetTempo { bpm } => Some(*bpm),
            Effect::SetGlobalVolume { volume } => Some(*volume),
            Effect::GlobalVolumeSlide { up, down } => Some(((*up).max(0) as u8) << 4 | ((*down).unsigned_abs().min(15) as u8)),
            Effect::SetEnvelopePosition { tick } => Some((tick & 0xFF) as u8),
            Effect::Panbrello { speed, depth } => Some((speed << 4) | (depth & 0x0F)),
            Effect::PatternDelay { ticks } => Some(*ticks),
            Effect::SetPanPosition { pan } => Some(*pan),
            Effect::GlissandoControl { on } => if *on { Some(1) } else { Some(0) },
            Effect::VibratoWaveform { waveform } => Some(*waveform),
            Effect::SetFineTune { tune } => Some(*tune),
            Effect::PatternLoop { count } => Some(*count),
            Effect::TremoloWaveform { waveform } => Some(*waveform),
            Effect::SetPanning16 { pan } => Some(*pan),
            Effect::Retrigger { interval } => Some(*interval),
            Effect::NoteCutAfter { ticks } => Some(*ticks),
            Effect::NoteDelay { ticks } => Some(*ticks),
            Effect::FinePortamentoUp { speed } => Some(*speed),
            Effect::FinePortamentoDown { speed } => Some(*speed),
            Effect::FineVolumeSlideUp { amount } => Some(*amount),
            Effect::FineVolumeSlideDown { amount } => Some(*amount),
            Effect::Tremor { ontime, offtime } => Some((ontime << 4) | (offtime & 0x0F)),
            Effect::VolSetVolume { vol } => Some(*vol),
            Effect::VolFineSlideUp { amount } => Some(*amount),
            Effect::VolFineSlideDown { amount } => Some(*amount),
            Effect::VolSlideUp { amount } => Some(*amount),
            Effect::VolSlideDown { amount } => Some(*amount),
            Effect::VolPortamento { speed } => Some(*speed),
            Effect::VolVibrato { speed } => Some(*speed),
        }
    }
}#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ExtendedEffect {
    NoteCut,
    NoteDelay,
    PatternDelay,
    Glissando,
    VibratoWaveform,
    SetFineTune,
    PatternLoop,
    TremoloWaveform,
    SetPanning,
    Retrigger,
    FineVolSlideUp,
    FineVolSlideDown,
    InvertLoop,
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
