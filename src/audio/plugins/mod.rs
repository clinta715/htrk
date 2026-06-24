// Plugin hosting for CLAP (and future VST3) plugins.
//
// Phase 1: trait, types, discovery, metadata cache.
// Phase 2: CLAP send FX integration into the audio engine.

pub mod clap_plugin;
pub mod discovery;
pub mod library;
pub mod param_ring;
#[cfg(windows)]
pub mod plugin_window;

pub use library::PluginLibrary;

use std::any::Any;
use std::path::PathBuf;

// ── Format & Type Enums ──

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PluginFormat {
    Clap,
    // Vst3,  // future
}

impl PluginFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginFormat::Clap => "clap",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PluginType {
    Instrument,
    Effect,
    Both,
    Analyzer,
}

/// Where the plugin's editor window should be hosted.
///
/// **Floating** (default): the plugin creates its own top-level OS window. The
/// host doesn't need to provide a parent. This is the safest mode because
/// many fixed-pixel plugins (Dexed, free synths) don't handle DPI scaling
/// correctly when embedded in a host window.
///
/// **Embedded**: the plugin is parented to a host-provided HWND. The HWND
/// is created by the host (a `WS_CHILD` of the eframe main window) and the
/// plugin's child HWND is sized to fill the host's client area. This gives
/// the best visual integration but may look bad with plugins that don't
/// handle DPI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorMode {
    Floating,
    /// The host HWND will be created (or reused) as a child of the eframe
    /// main window. The plugin is parented to it via `set_parent`.
    Embedded,
}

// ── Descriptor (discovered metadata) ──

#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginDescriptor {
    pub format: PluginFormat,
    pub path: PathBuf,
    pub plugin_id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub description: String,
    pub plugin_type: PluginType,
    pub audio_inputs: u8,
    pub audio_outputs: u8,
    pub has_editor: bool,
    pub supports_state: bool,
}

// ── Audio Bus (stereo planar) ──

#[derive(Clone, Debug)]
pub struct AudioBus {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl AudioBus {
    pub fn new(capacity: usize) -> Self {
        AudioBus {
            left: vec![0.0; capacity],
            right: vec![0.0; capacity],
        }
    }

    pub fn resize(&mut self, capacity: usize) {
        self.left.resize(capacity, 0.0);
        self.right.resize(capacity, 0.0);
    }

    pub fn clear(&mut self) {
        self.left.fill(0.0);
        self.right.fill(0.0);
    }

    pub fn len(&self) -> usize {
        debug_assert_eq!(self.left.len(), self.right.len());
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }
}

// ── Transport Info ──

#[derive(Clone, Copy, Debug, Default)]
pub struct TransportInfo {
    pub bpm: f64,
    pub sample_rate: f64,
    pub sample_position: u64,
    pub is_playing: bool,
}

// ── Parameter Info ──

#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub is_automatable: bool,
    pub is_modulatable: bool,
}

// ── Parameter Change Event (for SPSC queue) ──

pub use param_ring::ParamChange;
pub use param_ring::ParamRingBuffer;

// ── Audio-Thread Trait ──
//
// Implemented by anything that processes audio in the audio thread.
// Must be Send. Must not allocate in `process()`.

pub trait HostedPluginProcessor: Send + std::fmt::Debug {
    /// Process audio. Called from the audio thread.
    /// All buffers are pre-allocated and the same length.
    /// `frame_count` is the number of valid frames in each buffer.
    fn process(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        frame_count: usize,
        transport: &TransportInfo,
    );

    /// Stop the processor and return an opaque handle to the stopped state.
    /// The handle is passed back to the main-thread `HostedPluginHandle::deactivate`
    /// which downcasts it to the concrete type and calls the plugin's deactivation.
    /// Consumes self.
    fn stop(self: Box<Self>) -> Box<dyn std::any::Any>;

    /// Set a parameter value (main-thread or audio-thread safe).
    fn set_parameter(&mut self, param_id: u32, value: f32);

    /// Get a parameter value.
    fn get_parameter(&self, param_id: u32) -> f32;

    /// Number of parameters.
    fn parameter_count(&self) -> u32;

    /// Latency in samples (for compensation).
    fn latency(&self) -> u32;

    /// Plugin name.
    fn name(&self) -> &str;
}

// ── Main-Thread Trait ──
//
// Implemented by plugin handles that live on the main thread.
// NOT Send — the CLAP `PluginInstance` is `!Send` and must remain on the
// thread that created it. The Handle owns the instance and the main-thread
// state; it activates the plugin and extracts a Processor to send to the audio
// thread.

pub trait HostedPluginHandle {
    /// Returns the descriptor for this plugin.
    fn descriptor(&self) -> &PluginDescriptor;

    /// Activate the plugin and return a processor for the audio thread.
    /// Called on the main thread.
    fn activate(&mut self, sample_rate: f64, max_block: u32) -> Result<Box<dyn HostedPluginProcessor>, String>;

    /// Deactivate the plugin. Called on the main thread.
    /// The caller must have already received a StoppedAudioProcessor from the
    /// audio thread (via the processor's stop method) and pass it here.
    fn deactivate(&mut self, stopped: Box<dyn std::any::Any>) -> Result<(), String>;

    /// Save plugin state as opaque bytes. Called on the main thread.
    fn save_state(&self) -> Result<Vec<u8>, String>;

    /// Load plugin state from opaque bytes. Called on the main thread.
    fn load_state(&mut self, state: &[u8]) -> Result<(), String>;

    /// Get all parameter info. Called on the main thread.
    fn parameter_info(&self) -> Vec<ParamInfo>;

    /// Returns self as Any for downcasting to a concrete type.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Open the plugin's editor (if it has one). Called on the main thread.
    ///
    /// - `mode = EditorMode::Floating`: the plugin creates its own top-level
    ///   OS window. The host doesn't need to provide a parent.
    /// - `mode = EditorMode::Embedded`: the plugin is parented to a host-
    ///   provided HWND (`parent_hwnd`). The HWND should be a child of the
    ///   main application window. Ignored on non-Windows platforms.
    ///
    /// The implementation may fall back from one mode to the other if the
    /// plugin doesn't support the requested mode.
    #[cfg(windows)]
    fn open_editor(
        &mut self,
        mode: EditorMode,
        parent_hwnd: Option<*mut std::ffi::c_void>,
    ) -> Result<(), String>;

    /// Non-Windows fallback. Always uses floating mode.
    #[cfg(not(windows))]
    fn open_editor(&mut self, mode: EditorMode) -> Result<(), String>;

    /// Close the plugin's editor (if open). Called on the main thread.
    /// Safe to call even if the editor is not open.
    fn close_editor(&mut self);

    /// Returns true if the plugin's editor is currently open.
    fn is_editor_open(&self) -> bool;

    /// Returns true if the plugin has an editor at all.
    fn has_editor(&self) -> bool;

    /// Returns the current editor mode (only meaningful if `is_editor_open`).
    /// Returns `None` if no editor is open.
    fn editor_mode(&self) -> Option<EditorMode>;

    /// Returns the host-side container HWND for an embedded-mode editor
    /// (Windows only). Used by the UI to detect when the user X-closes the
    /// window and update the button label.
    #[cfg(windows)]
    fn editor_hwnd(&self) -> Option<*mut std::ffi::c_void>;

    /// Returns the last error message from `open_editor`, if any. Cleared
    /// on the next `open_editor` call.
    fn last_editor_error(&self) -> Option<String>;
}

// ── Plugin Slot (persistence model) ──

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PluginSlot {
    pub format: String,
    pub path: String,
    pub plugin_id: String,
    pub state: Vec<u8>,
}

// ── Errors ──

#[derive(Debug)]
pub enum PluginError {
    LoadFailed(String),
    ActivationFailed(String),
    InvalidFormat(String),
    NotFound(String),
    StateError(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::LoadFailed(s) => write!(f, "Plugin load failed: {s}"),
            PluginError::ActivationFailed(s) => write!(f, "Plugin activation failed: {s}"),
            PluginError::InvalidFormat(s) => write!(f, "Invalid format: {s}"),
            PluginError::NotFound(s) => write!(f, "Not found: {s}"),
            PluginError::StateError(s) => write!(f, "State error: {s}"),
        }
    }
}

impl std::error::Error for PluginError {}

impl From<PluginError> for String {
    fn from(e: PluginError) -> Self {
        e.to_string()
    }
}

// ── Standard CLAP Discovery Paths ──

pub fn default_search_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Some(p) = std::env::var_os("COMMONPROGRAMFILES") {
            paths.push(std::path::PathBuf::from(p).join("CLAP"));
        }
        paths.push(std::path::PathBuf::from(r"C:\Program Files\Common Files\CLAP"));
    }
    #[cfg(target_os = "macos")]
    {
        paths.push(std::path::PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join("Library/Audio/Plug-Ins/CLAP"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        paths.push(std::path::PathBuf::from("/usr/lib/clap"));
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".clap"));
        }
        paths.push(std::path::PathBuf::from("/usr/local/lib/clap"));
    }
    paths
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_bus() {
        let mut bus = AudioBus::new(4);
        assert_eq!(bus.len(), 4);
        bus.left[2] = 0.5;
        bus.clear();
        assert_eq!(bus.left[2], 0.0);

        bus.resize(8);
        assert_eq!(bus.len(), 8);
    }

    #[test]
    fn test_plugin_format_str() {
        assert_eq!(PluginFormat::Clap.as_str(), "clap");
    }

    #[test]
    fn test_default_search_paths_nonempty() {
        let paths = default_search_paths();
        assert!(!paths.is_empty(), "default search paths should not be empty");
    }

    #[test]
    fn test_plugin_slot_default() {
        let slot = PluginSlot::default();
        assert_eq!(slot.format, "");
        assert_eq!(slot.path, "");
        assert_eq!(slot.plugin_id, "");
        assert!(slot.state.is_empty());
    }

    #[test]
    fn test_param_change_construction() {
        let change = ParamChange { param_id: 7, value: 0.5 };
        assert_eq!(change.param_id, 7);
        assert!((change.value - 0.5).abs() < 1e-6);
    }
}
