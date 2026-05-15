use crate::sequencer::module::Module;
use crate::sequencer::player::PlayMode;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpolationType {
    Nearest,
    Linear,
    Cubic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimiterMode {
    HardClip,
    SoftKnee,
    SoftKneeSmooth,
}

#[derive(Clone, Debug)]
pub enum AudioCommand {
    Play,
    PlayFrom { order: u16, row: u16 },
    Stop,
    Pause,
    SetBPM(u16),
    SetSpeed(u8),

    LoadModule(Arc<Module>),

    SetChannelMuted { channel: usize, muted: bool },
    SetChannelSolo { channel: usize, solo: bool },

    SetMasterVolume(f32),
    SetPlayMode(PlayMode),

    SetInterpolation(InterpolationType),
    SetLimiterMode(LimiterMode),

    TriggerPreviewNote {
        sample_index: usize,
        note_key: u8,
        volume: f32,
        panning: f32,
    },
}
