use crate::sequencer::module::Module;
use crate::sequencer::pattern::Cell;
use crate::sequencer::player::PlayMode;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum InterpolationType {
    Nearest,
    Linear,
    Cubic,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum AudioCommand {
    Play,
    PlayFrom { order: u16, row: u16 },
    Stop,
    Pause,
    SetBPM(u16),
    SetSpeed(u8),
    SetGlobalVolume(u8),

    LoadModule(Arc<Module>),

    SetChannelMuted { channel: usize, muted: bool },
    SetChannelSolo { channel: usize, solo: bool },

    SetPatternCell { order: usize, row: usize, channel: usize, cell: Cell },

    SetMasterVolume(f32),
    SetInterpolation(InterpolationType),
    SetPlayMode(PlayMode),

    SeekTo { order: u16, row: u16 },
}
