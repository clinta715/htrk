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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
