use crate::audio::plugins::HostedPluginProcessor;
use crate::sequencer::effect::SendEffectType;
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

#[derive(Debug)]
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

    PreviewBuffer {
        data: Arc<Vec<f32>>,
        sample_rate: u32,
        note_key: u8,
        volume: f32,
        panning: f32,
    },

    SetSendLevel { channel: usize, send_index: usize, level: f32 },
    SetSendReturnLevel { send_index: usize, level: f32 },
    SetSendFxParam { send_index: usize, param: u32, value: f32 },
    SetSendEffectType { send_index: usize, effect_type: SendEffectType },
    SetSendPreFader { send_index: usize, pre_fader: bool },

    /// Install a hosted plugin processor on a send bus, replacing any built-in
    /// SendEffect. The plugin is already activated by the main thread; the
    /// audio thread just calls process() each callback. Pass `None` to clear.
    SetSendPlugin {
        send_index: usize,
        processor: Option<Box<dyn HostedPluginProcessor>>,
    },
    /// Set a parameter on the hosted plugin processor on a send bus.
    SetSendPluginParam {
        send_index: usize,
        param_id: u32,
        value: f32,
    },
    /// Set a parameter on a hosted instrument plugin processor.
    /// Routed to the audio thread's `instrument_plugin_processors[idx]`
    /// via its param ring.
    SetInstrumentPluginParam {
        instrument_idx: usize,
        param_id: u32,
        value: f32,
    },
    /// Install or remove a hosted plugin processor for an instrument slot.
    /// Pass `None` to clear (unload). The processor is already activated by
    /// the main thread. instrument_idx is the 1-based instrument index
    /// matching `last_instrument`.
    InstallInstrumentPlugin {
        instrument_idx: usize,
        processor: Option<Box<dyn HostedPluginProcessor>>,
    },

    /// Preview a note on a hosted instrument plugin (e.g. for keyboard
    /// preview when a CLAP instrument is selected). Sends a MIDI note-on
    /// to the processor at the given index. Pair with
    /// `PreviewInstrumentPluginNoteOff` when the key is released.
    PreviewInstrumentPlugin {
        instrument_idx: usize,
        midi_channel: u8,
        note_key: u8,
        velocity: u8,
    },

    /// Release a previously-previewed note on a hosted instrument plugin.
    /// Sent when the user releases the key, when a different key takes
    /// over the preview slot, or when the instrument is unloaded.
    PreviewInstrumentPluginNoteOff {
        instrument_idx: usize,
        midi_channel: u8,
        note_key: u8,
    },
}
