// PluginSlot — persistence model for CLAP (and future VST3) plugins.
//
// Used by Module to record which plugin is loaded on a send bus (Phase 2)
// or instrument slot (Phase 3). On .htk load, the main thread re-instantiates
// the plugin, activates it, restores state, and sends the audio processor
// to the audio thread.

use serde::{Deserialize, Serialize};

/// Lightweight, format-agnostic plugin reference. The runtime loads the
/// concrete plugin via clack-host (or vst3-host in the future) and feeds
/// it the saved `state` blob via the plugin's `state.load` extension.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PluginSlot {
    /// Format identifier: "clap" (future: "vst3").
    pub format: String,
    /// Absolute path to the .clap bundle or DLL.
    pub path: String,
    /// Stable plugin id (CLAP id or VST3 CID).
    pub plugin_id: String,
    /// Opaque state blob from the plugin's state-save extension.
    /// Empty if the plugin doesn't implement state, or if state was never saved.
    pub state: Vec<u8>,
    /// Last known editor window size, for editor-restoration in Phase 5.
    /// `None` if the plugin has no editor or the size is unknown.
    #[serde(default)]
    pub last_window_size: Option<(u32, u32)>,
}

impl PluginSlot {
    /// Construct a new slot with the minimum required fields. State is empty.
    pub fn new(format: impl Into<String>, path: impl Into<String>, plugin_id: impl Into<String>) -> Self {
        PluginSlot {
            format: format.into(),
            path: path.into(),
            plugin_id: plugin_id.into(),
            state: Vec::new(),
            last_window_size: None,
        }
    }

    /// True if this slot has enough info to attempt to load the plugin.
    pub fn is_loadable(&self) -> bool {
        !self.format.is_empty() && !self.path.is_empty() && !self.plugin_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_slot_is_not_loadable() {
        let slot = PluginSlot::default();
        assert!(!slot.is_loadable());
    }

    #[test]
    fn test_new_slot_is_loadable() {
        let slot = PluginSlot::new("clap", "/path/to/plugin.clap", "com.example.plugin");
        assert!(slot.is_loadable());
        assert_eq!(slot.format, "clap");
        assert_eq!(slot.path, "/path/to/plugin.clap");
        assert_eq!(slot.plugin_id, "com.example.plugin");
        assert!(slot.state.is_empty());
    }

    #[test]
    fn test_partial_slot_not_loadable() {
        let mut slot = PluginSlot::new("clap", "/path", "id");
        slot.path = String::new();
        assert!(!slot.is_loadable());
    }
}
