// CLAP plugin loading, hosting, and audio-thread processing.
//
// Split into focused submodules:
// - `host`      : the CLAP host handler (logging + GUI callbacks)
// - `handle`    : main-thread `ClapPluginHandle` (load, state, editor)
// - `processor` : audio-thread `ClapPluginProcessor` (process, note/param queue)
//
// The shared third-party imports are re-exported to the submodules here so the
// children can simply `use super::*;`.

pub(super) use std::any::Any;
pub(super) use std::io::Cursor;
pub(super) use std::path::Path;

pub(super) use clack_common::plugin::PluginDescriptor as ClapPluginDescriptor;
pub(super) use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui as PluginGuiExt,
    Window as ClapWindow,
};
pub(super) use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
pub(super) use clack_extensions::params::{ParamInfoBuffer, PluginParams};
pub(super) use clack_host::prelude::*;

pub(super) use super::{
    param_channel, HostedPluginHandle, HostedPluginProcessor, ParamChange, ParamInfo,
    ParamReceiver, ParamSender, PluginDescriptor, PluginError, PluginFormat, PluginType,
    TransportInfo,
};

mod handle;
mod host;
mod processor;
#[cfg(test)]
mod tests;

pub use handle::{extract_descriptor_for_browser, ClapPluginHandle};
pub use host::{HtrkHost, HtrkHostShared, HtrkMainThread};
pub use processor::ClapPluginProcessor;
