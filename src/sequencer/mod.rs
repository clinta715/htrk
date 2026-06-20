pub mod automation;
pub mod effect;
pub mod envelope_generator;
pub mod instrument;
pub mod slice_detector;
pub mod module;
pub mod note;
pub mod pattern;
pub mod period;
pub mod player;
pub mod sample;

pub use automation::{
    AutomationPoint, AutomationTarget, AutomationTrack, InterpolationMode,
    remap_automation_orders,
};
pub use effect::Effect;
pub use instrument::{
    DuplicateCheckAction, DuplicateCheckType, Envelope, EnvelopeFlags, EnvelopePoint, Instrument,
    NewNoteAction,
};
pub use module::{ModVariant, Module, ModuleFlags, ModuleFormat, MAX_CHANNELS, DEFAULT_CHANNELS, MAX_ENVELOPE_POINTS};
pub use note::{Note, PERIOD_TABLE};
pub use pattern::{Cell, Pattern};
pub use period::{
    get_arp_tab, get_note_period, get_vib_tab, period_to_frequency, relocate_ton,
};
pub use sample::{LoopType, Sample, SampleFlags, VibratoWaveform};
