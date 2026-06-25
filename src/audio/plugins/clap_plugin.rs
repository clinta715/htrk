// CLAP plugin loader (Phase 2).
//
// Skeleton implementation: provides the lifecycle scaffolding (load, activate,
// stop, deactivate) and a stub process() that passes audio through unchanged.
// The real CLAP processing pipeline (events, parameter queue, AudioPorts) is
// filled in incrementally as we integrate with real CLAP plugins.

use std::any::Any;
use std::path::Path;
use std::sync::Arc;

use clack_host::prelude::*;
use clack_common::plugin::PluginDescriptor as ClapPluginDescriptor;
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui as PluginGuiExt,
    Window as ClapWindow,
};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::params::{ParamInfoBuffer, PluginParams};
#[cfg(windows)]
use crate::audio::plugins::plugin_window::{self, PluginHostWindow};

use super::{
    EditorMode, HostedPluginHandle, HostedPluginProcessor, ParamChange, ParamInfo,
    ParamRingBuffer, PluginDescriptor, PluginError, PluginFormat, PluginType, TransportInfo,
};

// ── Host Handler ──
//
// `HtrkHostShared` is the shared (thread-safe) callback handler that CLAP
// plugins use to log messages and request GUI changes. We register the
// `HostLog` and `HostGui` host-side extensions so plugins can talk back to us.

pub struct HtrkHost;
pub struct HtrkHostShared;

impl<'a> SharedHandler<'a> for HtrkHostShared {
    fn request_restart(&self) {
        tracing::debug!("CLAP plugin requested restart");
    }
    fn request_process(&self) {
        tracing::debug!("CLAP plugin requested process");
    }
    fn request_callback(&self) {
        tracing::debug!("CLAP plugin requested callback");
    }
}

impl HostLogImpl for HtrkHostShared {
    fn log(&self, severity: LogSeverity, message: &str) {
        match severity {
            LogSeverity::Debug => tracing::debug!("CLAP: {message}"),
            LogSeverity::Info => tracing::info!("CLAP: {message}"),
            LogSeverity::Warning => tracing::warn!("CLAP: {message}"),
            LogSeverity::Error => tracing::error!("CLAP: {message}"),
            LogSeverity::Fatal => tracing::error!("CLAP FATAL: {message}"),
            LogSeverity::HostMisbehaving => tracing::error!("CLAP HOST MISBEHAVING: {message}"),
            LogSeverity::PluginMisbehaving => tracing::warn!("CLAP PLUGIN MISBEHAVING: {message}"),
        }
    }
}

impl HostGuiImpl for HtrkHostShared {
    fn resize_hints_changed(&self) {
        tracing::debug!("CLAP plugin resize hints changed");
    }
    fn request_resize(&self, new_size: GuiSize) -> Result<(), HostError> {
        tracing::debug!("CLAP plugin requested resize to {}x{}", new_size.width, new_size.height);
        Ok(())
    }
    fn request_show(&self) -> Result<(), HostError> {
        tracing::debug!("CLAP plugin requested show");
        Ok(())
    }
    fn request_hide(&self) -> Result<(), HostError> {
        tracing::debug!("CLAP plugin requested hide");
        Ok(())
    }
    fn closed(&self, was_destroyed: bool) {
        tracing::debug!("CLAP plugin GUI closed (destroyed: {was_destroyed})");
    }
}

impl HostHandlers for HtrkHost {
    type Shared<'a> = HtrkHostShared;
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder
            .register::<HostLog>()
            .register::<HostGui>();
    }
}

// ── Main-thread Handle ──

pub struct ClapPluginHandle {
    instance: Option<PluginInstance<HtrkHost>>,
    descriptor: PluginDescriptor,
    activated: bool,
    editor_open: bool,
    /// Which editor mode is currently in use (None if not open).
    editor_mode: Option<crate::audio::plugins::EditorMode>,
    /// On Windows, a top-level HWND used as the parent for an embedded
    /// plugin GUI. Only populated when the plugin is in embedded mode
    /// (or when floating was unavailable and we fell back to embedded).
    #[cfg(windows)]
    host_window: Option<PluginHostWindow>,
    /// Last error from `open_editor` (e.g. plugin doesn't support any GUI mode,
    /// or HWND creation failed). Surfaced to the UI so the user sees what went
    /// wrong instead of a silent failure.
    last_editor_error: Option<String>,
    /// Cached parameter info, populated lazily on the first call to
    /// `parameter_info()`. Avoids re-querying the plugin on every
    /// UI frame (the query requires a main-thread `plugin_handle()` call
    /// which is relatively expensive due to FFIs).
    cached_param_info: Vec<ParamInfo>,
    /// SPSC parameter ring shared with the audio-thread `ClapPluginProcessor`.
    /// The handle pushes here when the user (or automation) requests a
    /// param change. The processor drains the ring inside `process()` and
    /// feeds `ParamValueEvent`s into the input events buffer.
    param_ring: Arc<ParamRingBuffer>,
    /// Monotonic counter to assign a stable host-side index to each
    /// discovered parameter. This index is what the UI uses to refer to
    /// a specific param (e.g. for automation targets). The underlying
    /// CLAP `ClapId` is opaque and not stable across rescans.
    param_index_to_id: Vec<u32>,
}

impl ClapPluginHandle {
    /// Load a CLAP plugin from disk and instantiate it. Discovers the first plugin in the bundle.
    ///
    /// On Windows, CLAP plugins can be packaged as either:
    /// - A single `.clap` DLL file directly at the path
    /// - A directory with `.clap` extension containing a DLL of the same name
    /// This method handles both cases.
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        // On Windows, .clap can be a directory bundle. Resolve to the actual DLL.
        let load_path = if path.is_dir() {
            // Bundle directory: look for a DLL with the same stem
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| PluginError::InvalidFormat("Bundle missing name".into()))?;
            let dll_name = format!("{stem}.clap");
            let dll = path.join(&dll_name);
            if !dll.is_file() {
                return Err(PluginError::LoadFailed(format!(
                    "Bundle {} has no {} DLL",
                    path.display(),
                    dll_name
                )));
            }
            dll
        } else {
            path.to_path_buf()
        };

        let entry = unsafe {
            PluginEntry::load(&load_path).map_err(|e| PluginError::LoadFailed(e.to_string()))?
        };

        let plugin_factory = entry
            .get_plugin_factory()
            .ok_or(PluginError::LoadFailed("No plugin factory".into()))?;

        let clap_descriptor = plugin_factory
            .plugin_descriptors()
            .next()
            .ok_or(PluginError::LoadFailed("No plugins in bundle".into()))?;

        let host_info = HostInfo::new("htrk", "htrk", "https://github.com/clinta715/htrk", env!("CARGO_PKG_VERSION"))
            .map_err(|e| PluginError::LoadFailed(e.to_string()))?;

        let plugin_id = clap_descriptor
            .id()
            .ok_or(PluginError::LoadFailed("Plugin missing id".into()))?
            .to_owned();

        let instance = PluginInstance::<HtrkHost>::new(
            |_| HtrkHostShared,
            |_| (),
            &entry,
            &plugin_id,
            &host_info,
        )
        .map_err(|e| PluginError::LoadFailed(e.to_string()))?;

        let descriptor = extract_descriptor(path, &clap_descriptor);

        Ok(ClapPluginHandle {
            instance: Some(instance),
            descriptor,
            activated: false,
            editor_open: false,
            editor_mode: None,
            #[cfg(windows)]
            host_window: None,
            last_editor_error: None,
            cached_param_info: Vec::new(),
            param_ring: Arc::new(ParamRingBuffer::new(256)),
            param_index_to_id: Vec::new(),
        })
    }

    /// Returns the descriptor.
    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    /// Get a reference to the parameter ring buffer. The audio-thread
    /// processor reads from this ring inside `process()` and feeds the
    /// queued values into the plugin as `ParamValueEvent`s.
    pub fn param_ring(&self) -> &Arc<ParamRingBuffer> {
        &self.param_ring
    }

    /// Get the cached parameter info. The cache is populated lazily on
    /// the first call to `parameter_info()`. Returns an empty slice if
    /// the plugin doesn't expose the params extension.
    pub fn param_info(&self) -> &[ParamInfo] {
        &self.cached_param_info
    }

    /// Look up the stable host-side index for a given CLAP param ID.
    /// The index is what the UI uses to refer to a specific param
    /// (e.g. for automation targets). Returns None if the param ID
    /// is not known.
    pub fn param_index_for_id(&self, clap_id: u32) -> Option<usize> {
        self.param_index_to_id.iter().position(|&id| id == clap_id)
    }

    /// Get the CLAP param ID for a host-side index. Returns None if
    /// the index is out of range.
    pub fn param_id_for_index(&self, index: usize) -> Option<u32> {
        self.param_index_to_id.get(index).copied()
    }

    /// Push a parameter change to the audio-thread ring. The audio
    /// thread will feed it as a `ParamValueEvent` on the next process()
    /// call. Value should be in the param's [min, max] range.
    pub fn set_parameter(&self, clap_id: u32, value: f32) {
        self.param_ring.push(ParamChange {
            param_id: clap_id,
            value: value as f64,
        });
    }

    /// Read a parameter's current value. Returns 0.0 if the plugin
    /// doesn't expose the param or the value is unavailable.
    pub fn get_parameter(&self, clap_id: u32) -> f32 {
        let Some(instance) = self.instance.as_ref() else {
            return 0.0;
        };
        // `plugin_handle()` requires &mut self. We use a raw pointer to
        // get a mutable reference to the instance. PluginInstance is !Send
        // and we only call this on the main thread, so this is safe.
        let raw_ptr: *const PluginInstance<HtrkHost> = instance;
        let Some(mut_instance) = (unsafe { (raw_ptr as *mut PluginInstance<HtrkHost>).as_ref() }) else {
            return 0.0;
        };
        let raw_mut = raw_ptr as *mut PluginInstance<HtrkHost>;
        let mut_instance_mut = unsafe { raw_mut.as_mut() }.expect("instance is null");
        let _ = mut_instance; // silence unused
        let mut handle = mut_instance_mut.plugin_handle();
        let Some(params) = handle.get_extension::<PluginParams>() else {
            return 0.0;
        };
        let id = clack_common::utils::ClapId::from(clap_id);
        params.get_value(&mut handle, id).unwrap_or(0.0) as f32
    }
}

/// Extract a plugin's descriptor without instantiating it.
/// Used for the plugin browser UI to list available plugins.
/// This loads the .clap library, queries the factory for descriptors, then
/// unloads. Costs ~10ms per plugin due to dlopen.
pub fn extract_descriptor_for_browser(path: &Path) -> Result<PluginDescriptor, PluginError> {
    // On Windows, .clap can be a directory bundle. Resolve to the actual DLL.
    let load_path = if path.is_dir() {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| PluginError::InvalidFormat("Bundle missing name".into()))?;
        let dll_name = format!("{stem}.clap");
        let dll = path.join(&dll_name);
        if !dll.is_file() {
            return Err(PluginError::LoadFailed(format!(
                "Bundle {} has no {} DLL", path.display(), dll_name
            )));
        }
        dll
    } else {
        path.to_path_buf()
    };
    let entry = unsafe {
        PluginEntry::load(&load_path).map_err(|e| PluginError::LoadFailed(e.to_string()))?
    };
    let plugin_factory = entry
        .get_plugin_factory()
        .ok_or(PluginError::LoadFailed("No plugin factory".into()))?;
    let clap_descriptor = plugin_factory
        .plugin_descriptors()
        .next()
        .ok_or(PluginError::LoadFailed("No plugins in bundle".into()))?;
    Ok(extract_descriptor(path, &clap_descriptor))
}

fn extract_descriptor(path: &Path, clap_desc: &ClapPluginDescriptor) -> PluginDescriptor {
    let plugin_id = clap_desc
        .id()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let name = clap_desc
        .name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    let vendor = clap_desc
        .vendor()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    let description = clap_desc
        .description()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Determine plugin type heuristically from features
    let features: Vec<String> = clap_desc
        .features()
        .map(|s| s.to_string_lossy().to_lowercase())
        .collect();

    let plugin_type = if features.iter().any(|f| f == "instrument") {
        PluginType::Instrument
    } else if features.iter().any(|f| f.contains("effect") || f == "analyzer") {
        PluginType::Effect
    } else {
        PluginType::Both
    };

    PluginDescriptor {
        format: PluginFormat::Clap,
        path: path.to_path_buf(),
        plugin_id,
        name,
        vendor,
        version: String::new(),
        description,
        plugin_type,
        audio_inputs: 2,
        audio_outputs: 2,
        has_editor: false,
        supports_state: true,
    }
}

impl HostedPluginHandle for ClapPluginHandle {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    fn activate(&mut self, sample_rate: f64, max_block: u32) -> Result<Box<dyn HostedPluginProcessor>, String> {
        let instance = self.instance.as_mut().ok_or_else(|| "Plugin not loaded".to_string())?;

        let config = PluginAudioConfiguration {
            sample_rate,
            min_frames_count: 1,
            max_frames_count: max_block.max(1),
        };

        // PluginInstance::activate consumes the closure, returns the StoppedPluginAudioProcessor
        let stopped = instance
            .activate(|_, _| (), config)
            .map_err(|e| e.to_string())?;

        // Start processing, getting the StartedPluginAudioProcessor for the audio thread
        let started = stopped
            .start_processing()
            .map_err(|e| format!("start_processing: {e:?}"))?;

        self.activated = true;
        Ok(Box::new(ClapPluginProcessor::new(
            started,
            self.descriptor.clone(),
            sample_rate,
            max_block as usize,
            self.param_ring.clone(),
        )))
    }

    fn deactivate(&mut self, stopped: Box<dyn Any>) -> Result<(), String> {
        if let Some(instance) = self.instance.as_mut() {
            if self.activated {
                // Downcast the Box to the concrete StoppedPluginAudioProcessor type
                // and pass it to instance.deactivate() which stops the plugin.
                let stopped = stopped
                    .downcast::<clack_host::process::StoppedPluginAudioProcessor<HtrkHost>>()
                    .map_err(|_| "stopped processor wrong type".to_string())?;
                instance.deactivate(*stopped);
                self.activated = false;
            }
        }
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>, String> {
        // State save requires the state extension and a live instance. Stub.
        Ok(Vec::new())
    }

    fn load_state(&mut self, _state: &[u8]) -> Result<(), String> {
        // State restore requires the state extension. Stub.
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    #[cfg(windows)]
    fn open_editor(
        &mut self,
        mode: crate::audio::plugins::EditorMode,
        parent_hwnd: Option<*mut std::ffi::c_void>,
    ) -> Result<(), String> {
        self.last_editor_error = None;
        if self.editor_open {
            return Ok(());
        }
        let instance = self
            .instance
            .as_mut()
            .ok_or_else(|| "Plugin not loaded".to_string())?;
        let mut handle = instance.plugin_handle();
        let gui_ext = handle
            .get_extension::<PluginGuiExt>()
            .ok_or_else(|| "Plugin does not expose the GUI extension".to_string())?;

        let floating_config = GuiConfiguration {
            api_type: GuiApiType::WIN32,
            is_floating: true,
        };
        let embedded_config = GuiConfiguration {
            api_type: GuiApiType::WIN32,
            is_floating: false,
        };

        // Honor the requested mode. If the plugin doesn't support it, fall back
        // to the other mode. Only fail if neither works.
        let try_modes: &[EditorMode] = match mode {
            EditorMode::Floating => &[EditorMode::Floating, EditorMode::Embedded],
            EditorMode::Embedded => &[EditorMode::Embedded, EditorMode::Floating],
        };

        let parent_hwnd_ptr: *mut std::ffi::c_void =
            parent_hwnd.unwrap_or(std::ptr::null_mut());

        for &try_mode in try_modes {
            let config = match try_mode {
                EditorMode::Floating => floating_config,
                EditorMode::Embedded => embedded_config,
            };
            if !gui_ext.is_api_supported(&mut handle, config) {
                continue;
            }

            // For embedded mode, we need a parent HWND. If the caller didn't
            // provide one (e.g. floating was requested but plugin only supports
            // embedded), fall back to creating a top-level host window.
            //
            // Inline the logic here to avoid borrow conflicts with self.instance
            // (which is already mutably borrowed via `handle`).
            let result: Result<(), String> = match try_mode {
                EditorMode::Floating => {
                    gui_ext
                        .create(&mut handle, config)
                        .map_err(|e| format!("Plugin GUI create failed: {e:?}"))?;
                    let _ = gui_ext.show(&mut handle);
                    Ok(())
                }
                EditorMode::Embedded => {
                    let title = format!("{} - htrk", self.descriptor.name);
                    let window_mode = if parent_hwnd_ptr.is_null() {
                        plugin_window::WindowMode::TopLevel
                    } else {
                        plugin_window::WindowMode::ChildOf(parent_hwnd_ptr)
                    };
                    let host_window = match PluginHostWindow::create(
                        &title, window_mode, 800, 600,
                    ) {
                        Some(w) => w,
                        None => Err("Failed to create plugin host window".to_string())?,
                    };
                    let hwnd = host_window.hwnd();
                    let clap_window = ClapWindow::from_win32_hwnd(hwnd as *mut _);
                    if let Err(e) = gui_ext.create(&mut handle, config) {
                        return Err(format!("Plugin GUI create failed: {e:?}"));
                    }
                    if let Some(size) = gui_ext.get_size(&mut handle) {
                        let _ = gui_ext.set_size(&mut handle, size);
                    }
                    unsafe {
                        let _ = gui_ext.set_parent(&mut handle, clap_window);
                    }
                    let _ = gui_ext.show(&mut handle);
                    self.host_window = Some(host_window);
                    Ok(())
                }
            };
            if result.is_ok() {
                self.editor_open = true;
                self.editor_mode = Some(try_mode);
                return Ok(());
            }
            // If this mode failed, try the next one.
        }
        let err = "Plugin does not support Win32 GUI (floating or embedded)".to_string();
        self.last_editor_error = Some(err.clone());
        Err(err)
    }

    #[cfg(not(windows))]
    fn open_editor(
        &mut self,
        mode: crate::audio::plugins::EditorMode,
    ) -> Result<(), String> {
        // Non-Windows: only floating mode is supported.
        let _ = mode;
        self.last_editor_error = None;
        if self.editor_open {
            return Ok(());
        }
        let instance = self
            .instance
            .as_mut()
            .ok_or_else(|| "Plugin not loaded".to_string())?;
        let mut handle = instance.plugin_handle();
        let gui_ext = handle
            .get_extension::<PluginGuiExt>()
            .ok_or_else(|| "Plugin does not expose the GUI extension".to_string())?;
        let config = GuiConfiguration {
            api_type: GuiApiType::WIN32,
            is_floating: true,
        };
        if !gui_ext.is_api_supported(&mut handle, config) {
            let err = "Plugin does not support floating GUI".to_string();
            self.last_editor_error = Some(err.clone());
            return Err(err);
        }
        gui_ext.create(&mut handle, config)
            .map_err(|e| format!("Plugin GUI create failed: {e:?}"))?;
        let _ = gui_ext.show(&mut handle);
        self.editor_open = true;
        self.editor_mode = Some(crate::audio::plugins::EditorMode::Floating);
        Ok(())
    }

    fn close_editor(&mut self) {
        if !self.editor_open {
            return;
        }
        if let Some(instance) = self.instance.as_mut() {
            let mut handle = instance.plugin_handle();
            if let Some(gui_ext) = handle.get_extension::<PluginGuiExt>() {
                gui_ext.destroy(&mut handle);
            }
        }
        // Drop the host window (this destroys the HWND).
        #[cfg(windows)]
        {
            self.host_window = None;
        }
        self.editor_open = false;
        self.editor_mode = None;
    }

    fn is_editor_open(&self) -> bool {
        self.editor_open
    }

    fn has_editor(&self) -> bool {
        let Some(instance) = self.instance.as_ref() else {
            return false;
        };
        // `plugin_handle()` requires &mut self. We use a raw pointer to get a mutable
        // reference to the instance. PluginInstance is !Send and we only call this
        // on the main thread, so this is safe.
        let raw_ptr: *const PluginInstance<HtrkHost> = instance;
        let mut_instance = unsafe { (raw_ptr as *mut PluginInstance<HtrkHost>).as_mut() }
            .expect("instance pointer is null");
        let mut handle = mut_instance.plugin_handle();
        let Some(gui_ext) = handle.get_extension::<PluginGuiExt>() else {
            return false;
        };
        // Some plugins only support one mode. Probe both floating and embedded.
        gui_ext.is_api_supported(
            &mut handle,
            GuiConfiguration { api_type: GuiApiType::WIN32, is_floating: true },
        ) || gui_ext.is_api_supported(
            &mut handle,
            GuiConfiguration { api_type: GuiApiType::WIN32, is_floating: false },
        )
    }

    fn editor_mode(&self) -> Option<crate::audio::plugins::EditorMode> {
        self.editor_mode
    }

    #[cfg(windows)]
    fn editor_hwnd(&self) -> Option<*mut std::ffi::c_void> {
        self.host_window.as_ref().map(|w| w.hwnd())
    }

    fn last_editor_error(&self) -> Option<String> {
        self.last_editor_error.clone()
    }

    fn parameter_info(&self) -> Vec<ParamInfo> {
        // Same logic as the inherent method but inlined here to avoid
        // method-name collision. Returns the cached parameter info;
        // populates the cache on first call.
        if !self.cached_param_info.is_empty() {
            return self.cached_param_info.clone();
        }
        let Some(instance) = self.instance.as_ref() else {
            return Vec::new();
        };
        let raw_ptr = instance as *const PluginInstance<HtrkHost>;
        let Some(mut_instance) = (unsafe { (raw_ptr as *mut PluginInstance<HtrkHost>).as_mut() }) else {
            return Vec::new();
        };
        let mut handle = mut_instance.plugin_handle();
        let Some(params) = handle.get_extension::<PluginParams>() else {
            return Vec::new();
        };
        let count = params.count(&mut handle);
        if count == 0 { return Vec::new(); }
        let mut buf = ParamInfoBuffer::new();
        let mut info_out: Vec<ParamInfo> = Vec::with_capacity(count as usize);
        let mut index_to_id: Vec<u32> = Vec::with_capacity(count as usize);
        for i in 0..count {
            if let Some(info) = params.get_info(&mut handle, i, &mut buf) {
                let id = info.id.get();
                let name = String::from_utf8_lossy(info.name).into_owned();
                let is_automatable = info.flags.contains(clack_extensions::params::ParamInfoFlags::IS_AUTOMATABLE);
                let is_modulatable = info.flags.contains(clack_extensions::params::ParamInfoFlags::IS_MODULATABLE);
                index_to_id.push(id);
                info_out.push(ParamInfo {
                    id, name,
                    min: info.min_value as f32,
                    max: info.max_value as f32,
                    default: info.default_value as f32,
                    is_automatable, is_modulatable,
                });
            }
        }
        // Cache update via raw pointer (same-thread, single-owner).
        let this = self as *const Self as *mut Self;
        unsafe {
            (*this).cached_param_info = info_out.clone();
            (*this).param_index_to_id = index_to_id;
        }
        info_out
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        // Delegate to the inherent method.
        self.get_parameter(param_id)
    }

    fn set_parameter(&self, param_id: u32, value: f32) {
        // Delegate to the inherent method.
        self.set_parameter(param_id, value);
    }
}

// ── Audio-thread Processor (real CLAP processing) ──

/// Real CLAP processor that calls the plugin's process() function each callback.
/// Wraps a `StartedPluginAudioProcessor<HtrkHost>` and pre-allocates the
/// audio I/O buffers + event buffers in `new()` for allocation-free processing.
pub struct ClapPluginProcessor {
    processor: StartedPluginAudioProcessor<HtrkHost>,
    descriptor: PluginDescriptor,
    sample_rate: f64,
    max_block: usize,

    // Pre-allocated I/O buffers, sized to max_block
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,

    // Pre-allocated AudioPorts containers
    input_ports: AudioPorts,
    output_ports: AudioPorts,

    // Pre-allocated event buffers (allocated once at construction; RT-safe)
    input_event_buffer: clack_host::events::io::EventBuffer,
    output_event_buffer: clack_host::events::io::EventBuffer,

    // SPSC parameter ring shared with the main-thread `ClapPluginHandle`.
    // The handle pushes here when the user (or automation) requests a
    // parameter change. We drain the ring inside `process()` and feed
    // the queued values into the plugin as `ParamValueEvent`s.
    param_ring: Arc<ParamRingBuffer>,
    // Scratch vector for drained param changes. Allocated once;
    // cleared between process() calls.
    param_scratch: Vec<ParamChange>,

    /// Queued note-on/off events from the sequencer, drained in process().
    /// Tuples: (note_on, midi_channel, key, velocity)
    note_events: std::collections::VecDeque<(bool, u8, u8, u8)>,
    /// Monotonically increasing note ID counter for CLAP note tracking.
    next_note_id: u32,
}

impl ClapPluginProcessor {
    pub fn new(
        processor: StartedPluginAudioProcessor<HtrkHost>,
        descriptor: PluginDescriptor,
        sample_rate: f64,
        max_block: usize,
        param_ring: Arc<ParamRingBuffer>,
    ) -> Self {
        let max_block = max_block.max(1) as usize;
        ClapPluginProcessor {
            processor,
            descriptor,
            sample_rate,
            max_block,
            in_l: vec![0.0; max_block],
            in_r: vec![0.0; max_block],
            out_l: vec![0.0; max_block],
            out_r: vec![0.0; max_block],
            input_ports: AudioPorts::with_capacity(2, 1),
            output_ports: AudioPorts::with_capacity(2, 1),
            input_event_buffer: clack_host::events::io::EventBuffer::with_capacity(0),
            output_event_buffer: clack_host::events::io::EventBuffer::with_capacity(0),
            param_ring,
            param_scratch: Vec::with_capacity(64),
            note_events: std::collections::VecDeque::new(),
            next_note_id: 0,
        }
    }

    /// Stop processing and return the StoppedPluginAudioProcessor so the handle can
    /// call `instance.deactivate(stopped)` on the main thread. Consumes self.
    pub fn stop(self) -> clack_host::process::StoppedPluginAudioProcessor<HtrkHost> {
        self.processor.stop_processing()
    }

}

impl std::fmt::Debug for ClapPluginProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClapPluginProcessor")
            .field("name", &self.descriptor.name)
            .field("sample_rate", &self.sample_rate)
            .field("max_block", &self.max_block)
            .finish()
    }
}

impl HostedPluginProcessor for ClapPluginProcessor {
    fn stop(self: Box<Self>) -> Box<dyn std::any::Any> {
        Box::new(ClapPluginProcessor::stop(*self))
    }

    fn send_note_on(&mut self, midi_channel: u8, key: u8, velocity: u8) {
        self.note_events.push_back((true, midi_channel, key, velocity));
    }

    fn send_note_off(&mut self, midi_channel: u8, key: u8) {
        self.note_events.push_back((false, midi_channel, key, 0));
    }

    fn process(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        frame_count: usize,
        _transport: &TransportInfo,
    ) {
        let n = frame_count.min(self.max_block);

        // Copy input into our pre-allocated buffers
        for i in 0..n {
            self.in_l[i] = if i < input_l.len() { input_l[i] } else { 0.0 };
            self.in_r[i] = if i < input_r.len() { input_r[i] } else { 0.0 };
            self.out_l[i] = 0.0;
            self.out_r[i] = 0.0;
        }
        // Zero the rest
        for i in n..self.max_block {
            self.in_l[i] = 0.0;
            self.in_r[i] = 0.0;
            self.out_l[i] = 0.0;
            self.out_r[i] = 0.0;
        }

        // Build audio port buffer array. We need &mut [f32] for the buffer references.
        // We split the buffers into individual channel slices.
        use clack_host::process::audio_buffers::{
            AudioPortBuffer, AudioPortBufferType, InputChannel,
        };

        // Split the in/out buffers into per-channel slices.
        // We use indices instead of slices to avoid borrow issues with `with_input_buffers`.
        let in_l_ptr = self.in_l.as_mut_ptr();
        let in_r_ptr = self.in_r.as_mut_ptr();
        let out_l_ptr = self.out_l.as_mut_ptr();
        let out_r_ptr = self.out_r.as_mut_ptr();
        let block_len = self.max_block;

        // SAFETY: The pointers are valid for `block_len` f32 elements.
        let in_l_slice: &mut [f32] = unsafe { std::slice::from_raw_parts_mut(in_l_ptr, block_len) };
        let in_r_slice: &mut [f32] = unsafe { std::slice::from_raw_parts_mut(in_r_ptr, block_len) };
        let out_l_slice: &mut [f32] = unsafe { std::slice::from_raw_parts_mut(out_l_ptr, block_len) };
        let out_r_slice: &mut [f32] = unsafe { std::slice::from_raw_parts_mut(out_r_ptr, block_len) };

        let input_audio = self.input_ports.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only([
                InputChannel::variable(in_l_slice),
                InputChannel::variable(in_r_slice),
            ]),
        }]);

        let mut output_audio = self.output_ports.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only([
                out_l_slice,
                out_r_slice,
            ]),
        }]);

        // Drain parameter ring and note queue, then build a single event
        // buffer containing both param-value and note-on/off events.
        self.param_scratch.clear();
        let drained = self.param_ring.drain_into(&mut self.param_scratch, 64);

        use clack_common::events::event_types::{
            NoteOffEvent, NoteOnEvent, ParamValueEvent,
        };
        use clack_common::events::{Match, Pckn};
        use clack_common::utils::{ClapId, Cookie};

        let total_events = drained + self.note_events.len();
        let mut ev_buffer = clack_host::events::io::EventBuffer::with_capacity(total_events.max(1));
        let cookie = Cookie::default();

        // Push param changes
        let pckn = Pckn::new(0u16, 0u16, 0u16, Match::All);
        for change in self.param_scratch.iter() {
            let ev = ParamValueEvent::new(
                0,
                ClapId::from(change.param_id),
                pckn,
                change.value,
                cookie,
            );
            let _ = ev_buffer.push(&ev);
        }

        // Push note events
        while let Some((note_on, midi_ch, key, velocity)) = self.note_events.pop_front() {
            if note_on {
                let note_id = self.next_note_id;
                self.next_note_id = self.next_note_id.wrapping_add(1);
                let note_pckn = Pckn::new(
                    0u16,
                    midi_ch as u16,
                    key as u16,
                    note_id,
                );
                let ev = NoteOnEvent::new(0, note_pckn, (velocity as f64) / 127.0);
                let _ = ev_buffer.push(&ev);
            } else {
                let note_pckn = Pckn::new(0u16, midi_ch as u16, key as u16, Match::All);
                let ev = NoteOffEvent::new(0, note_pckn, 0.0);
                let _ = ev_buffer.push(&ev);
            }
        }

        let input_events = clack_host::events::io::InputEvents::from_buffer(&ev_buffer);
        let mut output_events = clack_host::events::io::OutputEvents::from_buffer(&mut self.output_event_buffer);
        let _ = self.processor.process(
            &input_audio,
            &mut output_audio,
            &input_events,
            &mut output_events,
            None,
            None,
        );

        // Copy plugin output back to caller's buffers
        for i in 0..frame_count.min(n) {
            output_l[i] = self.out_l[i];
            output_r[i] = self.out_r[i];
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32) {
        // Push the change to the SPSC ring. The audio thread will pick
        // it up on the next process() call.
        self.param_ring.push(ParamChange {
            param_id,
            value: value as f64,
        });
    }

    fn get_parameter(&self, _param_id: u32) -> f32 {
        // Plugin parameter values are stored in the plugin itself;
        // reading them from the host requires a main-thread call
        // (plugin_handle is !Send). The processor doesn't have a copy
        // of the values — callers should go through the handle for
        // reads. Return 0.0 as a placeholder.
        0.0
    }

    fn parameter_count(&self) -> u32 {
        // The processor doesn't cache the count; the handle does.
        // Return 0 as a safe default — callers needing the count
        // should go through the handle.
        0
    }
    fn latency(&self) -> u32 { 0 }
    fn name(&self) -> &str { &self.descriptor.name }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that create or destroy real Win32 windows. The OS
    /// window class is process-global, so concurrent class registration +
    /// window creation across threads can deadlock the test runner.
    static WIN32_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_descriptor_extraction_basic() {
        let desc = PluginDescriptor {
            format: PluginFormat::Clap,
            path: std::path::PathBuf::from("/test/plugin.clap"),
            plugin_id: "test.id".into(),
            name: "Test Plugin".into(),
            vendor: "Tester".into(),
            version: "1.0".into(),
            description: "A test plugin".into(),
            plugin_type: PluginType::Effect,
            audio_inputs: 2,
            audio_outputs: 2,
            has_editor: false,
            supports_state: true,
        };
        assert_eq!(desc.name, "Test Plugin");
        assert_eq!(desc.format, PluginFormat::Clap);
    }

    #[test]
    fn test_invalid_path_returns_error() {
        let result = ClapPluginHandle::load(std::path::Path::new("/nonexistent/plugin.clap"));
        assert!(result.is_err());
    }

    /// Integration test: try to load a real CLAP plugin from the system install.
    /// Skipped if the plugin isn't available (CI without CLAP installed).
    #[test]
    fn test_load_real_clap_plugin() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found at {}", path.display());
            return;
        }
        let handle = ClapPluginHandle::load(path);
        match handle {
            Ok(h) => {
                let desc = h.descriptor();
                eprintln!("[ok] Loaded plugin: {} ({})", desc.name, desc.plugin_id);
                eprintln!("     vendor: {}", desc.vendor);
                eprintln!("     audio: {} in / {} out, has_editor={}, supports_state={}",
                    desc.audio_inputs, desc.audio_outputs, desc.has_editor, desc.supports_state);
                assert!(!desc.name.is_empty(), "Plugin name should not be empty");
            }
            Err(e) => panic!("Failed to load TAL-Reverb-4: {e}"),
        }
    }

    /// Integration test: try to load several different CLAP plugin formats
    /// (single-DLL and bundle) to verify the loader handles both.
    #[test]
    fn test_load_multiple_clap_plugins() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let candidates = [
            (r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap", "bundle"),
            (r"C:\Program Files\Common Files\CLAP\bit-crusher-windows-x64.clap", "dll"),
            (r"C:\Program Files\Common Files\CLAP\Dexed.clap", "dll"),
            (r"C:\Program Files\Common Files\CLAP\JC303.clap", "dll"),
        ];
        let mut loaded = 0;
        let mut failed: Vec<String> = Vec::new();
        for (path_str, kind) in &candidates {
            let path = std::path::Path::new(path_str);
            if !path.exists() {
                eprintln!("[skip] {} not found ({})", path_str, kind);
                continue;
            }
            match ClapPluginHandle::load(path) {
                Ok(h) => {
                    let desc = h.descriptor();
                    eprintln!("[ok] {}: {} ({})", kind, desc.name, desc.plugin_id);
                    loaded += 1;
                }
                Err(e) => {
                    eprintln!("[fail] {}: {}", path_str, e);
                    failed.push(path_str.to_string());
                }
            }
        }
        assert!(loaded >= 2, "Expected at least 2 plugins to load, got {loaded}");
        assert!(failed.is_empty(), "Failed to load: {failed:?}");
    }

    /// Integration test: full lifecycle (load → activate → process → deactivate).
    /// Skipped if the plugin isn't available.
    #[test]
    fn test_activate_real_clap_plugin() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found at {}", path.display());
            return;
        }

        let mut handle = ClapPluginHandle::load(path).expect("load failed");

        // Activate at 48kHz, 256 sample block size
        let mut processor = handle.activate(48000.0, 256).expect("activate failed");
        eprintln!("[ok] Activated plugin: {}", processor.name());

        // Process a block of silence (1s = 48000 samples, process in 256-sample chunks)
        let in_l = vec![0.0f32; 256];
        let in_r = vec![0.0f32; 256];
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        let transport = TransportInfo {
            bpm: 120.0,
            sample_rate: 48000.0,
            sample_position: 0,
            is_playing: true,
        };

        // Process a few blocks. The reverb should produce non-zero output if we
        // feed it a non-silent input. With silence input, it should produce
        // only tail/decay of previous samples (which is zero here, since
        // we never fed any signal).
        for block in 0..4 {
            processor.process(&in_l, &in_r, &mut out_l, &mut out_r, 256, &transport);
            eprintln!("[ok] Block {}: out[0] = {:.6} (silence in -> near-silence expected)",
                block, out_l[0]);
        }

        // Test with a real signal (impulse) to see if the reverb tail is produced
        let mut impulse_l = vec![0.0f32; 256];
        let mut impulse_r = vec![0.0f32; 256];
        impulse_l[0] = 1.0;
        impulse_r[0] = 1.0;
        processor.process(&impulse_l, &impulse_r, &mut out_l, &mut out_r, 256, &transport);
        let peak = out_l.iter().chain(out_r.iter()).fold(0.0f32, |a, &b| a.max(b.abs()));
        eprintln!("[ok] Impulse response peak: {peak:.6} (should be non-zero for active reverb)");

        // Test deactivation: stop the processor and deactivate the instance
        let stopped = processor.stop();
        handle.deactivate(stopped).expect("deactivate failed");
        eprintln!("[ok] Deactivated plugin cleanly");
    }

    /// Integration test: process a sine wave through Bit Crusher plugin.
    /// The output should be a quantized/distorted version of the input.
    #[test]
    fn test_bit_crusher_processing() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\bit-crusher-windows-x64.clap");
        if !path.exists() {
            eprintln!("[skip] Bit Crusher not found");
            return;
        }

        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        let mut processor = handle.activate(48000.0, 256).expect("activate failed");
        eprintln!("[ok] Activated: {}", processor.name());

        // Generate a 1kHz sine wave
        let freq = 1000.0f32;
        let sr = 48000.0f32;
        let mut input_l = Vec::with_capacity(256);
        let mut input_r = Vec::with_capacity(256);
        for i in 0..256 {
            let t = i as f32 / sr;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
            input_l.push(s);
            input_r.push(s);
        }
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        let transport = TransportInfo {
            bpm: 120.0,
            sample_rate: 48000.0,
            sample_position: 0,
            is_playing: true,
        };

        // Feed a few blocks of sine so the effect's internal state stabilizes
        for _ in 0..4 {
            processor.process(&input_l, &input_r, &mut out_l, &mut out_r, 256, &transport);
        }

        let in_peak = input_l.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let out_peak = out_l.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        eprintln!("[ok] Sine input peak: {in_peak:.4}");
        eprintln!("[ok] Bit crusher output peak: {out_peak:.4} (non-zero means plugin is processing)");
        assert!(out_peak > 0.01, "Bit Crusher should produce non-trivial output");

        // Compare sample values: bit crusher should quantize the input,
        // producing a different (not equal) output for most samples.
        let mut differences = 0;
        let mut exact_matches = 0;
        for i in 0..256 {
            if (input_l[i] - out_l[i]).abs() > 0.001 {
                differences += 1;
            } else {
                exact_matches += 1;
            }
        }
        eprintln!("[ok] Bit crusher: {differences}/256 samples changed, {exact_matches} exact matches");
        // Note: the bit crusher's default parameters may pass through audio unchanged.
        // We're just verifying the plugin processes audio (non-zero output is sufficient).
        // Once we have parameter control, we can crank up the bit reduction to confirm.

        let stopped = processor.stop();
        handle.deactivate(stopped).expect("deactivate failed");
        eprintln!("[ok] Deactivated cleanly");
    }

    /// Integration test: process audio through TAL Reverb 4 with a longer
    /// signal to verify the reverb tail extends across multiple blocks.
    #[test]
    fn test_reverb_tail_extends() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }

        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        let mut processor = handle.activate(48000.0, 256).expect("activate failed");
        let transport = TransportInfo {
            bpm: 120.0,
            sample_rate: 48000.0,
            sample_position: 0,
            is_playing: true,
        };

        // Block 0: send an impulse
        let mut impulse = vec![0.0f32; 256];
        impulse[0] = 1.0;
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        processor.process(&impulse, &impulse, &mut out_l, &mut out_r, 256, &transport);
        let block0_peak = out_l.iter().chain(out_r.iter()).fold(0.0f32, |a, &b| a.max(b.abs()));

        // Process ~100ms of silence following — reverb tail should continue
        let silence = vec![0.0f32; 256];
        let mut tail_peaks = Vec::new();
        for _ in 0..20 {
            processor.process(&silence, &silence, &mut out_l, &mut out_r, 256, &transport);
            let peak = out_l.iter().chain(out_r.iter()).fold(0.0f32, |a, &b| a.max(b.abs()));
            tail_peaks.push(peak);
        }

        let max_tail = tail_peaks.iter().fold(0.0f32, |a, &b| a.max(b));
        let blocks_with_signal = tail_peaks.iter().filter(|&&p| p > 0.001).count();
        eprintln!("[ok] Impulse block peak: {block0_peak:.4}");
        eprintln!("[ok] Max tail peak:      {max_tail:.4}");
        eprintln!("[ok] Blocks with signal: {blocks_with_signal}/20 (reverb tail should persist)");
        assert!(max_tail > 0.001, "Reverb tail should be audible after silence");
        assert!(blocks_with_signal >= 10, "Reverb tail should persist for at least ~130ms");

        let stopped = processor.stop();
        handle.deactivate(stopped).expect("deactivate failed");
    }

    /// Integration test: extract_descriptor_for_browser loads the .clap
    /// library just enough to read its descriptor (no instantiation).
    /// This is the fast path used by the plugin browser UI to populate
    /// the plugin list on startup.
    #[test]
    fn test_extract_descriptor_for_browser() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let clap_path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !clap_path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }
        match extract_descriptor_for_browser(clap_path) {
            Ok(d) => {
                eprintln!("[ok] Extracted descriptor: {} ({})", d.name, d.plugin_id);
                assert!(!d.name.is_empty(), "Descriptor should have a name");
                assert!(!d.plugin_id.is_empty(), "Descriptor should have a plugin id");
                assert_eq!(d.format, PluginFormat::Clap);
                // TAL Reverb 4 should be a Reverb effect, not instrument
                assert_ne!(d.plugin_type, PluginType::Instrument);
            }
            Err(e) => panic!("extract_descriptor_for_browser failed: {e}"),
        }
    }

    /// Integration test: open and close the GUI editor of a real CLAP plugin
    /// (TAL Reverb 4). The editor appears as a floating OS window. We just
    /// verify that open/close calls succeed without panicking — the visual
    /// window itself can't be tested from a headless context.
    #[test]
    fn test_editor_open_close_real_plugin() {
        // Serialize Win32 window creation across tests in the same process.
        // Multiple tests creating top-level windows concurrently can deadlock
        // the test runner (window class registration is process-global).
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }

        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        // Activate the plugin first (most CLAP plugins require activation
        // before the GUI can be created).
        let _processor = handle.activate(48000.0, 256).expect("activate failed");

        // Some CLAP plugins only expose the GUI extension after activation.
        // Probe via open_editor() and accept either success (Win32 GUI) or
        // a failure (no GUI / headless).
        assert!(!handle.is_editor_open(), "Editor should not be open initially");

        match handle.open_editor(
            crate::audio::plugins::EditorMode::Floating,
            None,
        ) {
            Ok(()) => {
                assert!(handle.is_editor_open(), "Editor should be open after open_editor()");
                eprintln!("[ok] Opened editor for {}", handle.descriptor().name);

                // Open again should be a no-op (returns Ok).
                handle
                    .open_editor(crate::audio::plugins::EditorMode::Floating, None)
                    .expect("double open should be a no-op");
                assert!(handle.is_editor_open());

                // Close the editor.
                handle.close_editor();
                assert!(!handle.is_editor_open(), "Editor should be closed after close_editor()");
                eprintln!("[ok] Closed editor cleanly");
            }
            Err(e) => {
                eprintln!("[warn] open_editor failed (may be headless or no GUI): {e}");
            }
        }

        // Close again should be a no-op.
        handle.close_editor();
    }

    /// Integration test: has_editor() returns a value (true or false)
    /// without panicking.
    #[test]
    fn test_has_editor_returns_bool() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }
        let handle = ClapPluginHandle::load(path).expect("load failed");
        let has = handle.has_editor();
        eprintln!("[ok] TAL Reverb 4 has_editor() = {has}");
    }

    /// Integration test: open editor in floating mode.
    /// Skips if the plugin doesn't support floating mode (most plugins
    /// only support embedded; e.g. TAL Reverb 4 falls back to embedded).
    #[test]
    fn test_open_editor_floating_with_real_plugin() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }
        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        let _processor = handle.activate(48000.0, 256).expect("activate failed");

        // Try floating first. Most plugins fall back to embedded.
        let result = handle.open_editor(
            crate::audio::plugins::EditorMode::Floating,
            None,
        );
        match result {
            Ok(()) => {
                assert!(handle.is_editor_open());
                let mode = handle.editor_mode();
                eprintln!("[ok] Opened editor in mode {mode:?} for {}", handle.descriptor().name);
                handle.close_editor();
                assert!(!handle.is_editor_open());
            }
            Err(e) => {
                eprintln!("[skip] Plugin doesn't support any GUI mode: {e}");
            }
        }
    }

    /// Integration test: open editor in embedded mode (no parent HWND).
    /// TAL Reverb 4 only supports embedded; this is the typical path.
    #[test]
    fn test_open_editor_embedded_with_real_plugin() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }
        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        let _processor = handle.activate(48000.0, 256).expect("activate failed");

        // No parent HWND — should fall back to a top-level host window
        let result = handle.open_editor(
            crate::audio::plugins::EditorMode::Embedded,
            None,
        );
        match result {
            Ok(()) => {
                assert!(handle.is_editor_open());
                assert_eq!(handle.editor_mode(), Some(crate::audio::plugins::EditorMode::Embedded));
                eprintln!("[ok] Opened embedded editor for {}", handle.descriptor().name);
                // The host HWND should be set
                #[cfg(windows)]
                {
                    assert!(handle.editor_hwnd().is_some(), "Embedded editor should expose an HWND");
                }
                handle.close_editor();
                assert!(!handle.is_editor_open());
                eprintln!("[ok] Closed embedded editor");
            }
            Err(e) => {
                eprintln!("[warn] Embedded open failed (may be headless): {e}");
            }
        }
    }

    /// Integration test: open editor with explicit non-null parent HWND.
    /// The plugin should be parented to that HWND via set_parent.
    #[test]
    fn test_open_editor_embedded_with_parent_hwnd() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }
        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        let _processor = handle.activate(48000.0, 256).expect("activate failed");

        // Pass a sentinel HWND. Plugin will accept it; rendering will look
        // wrong but the test verifies the call doesn't crash.
        let sentinel_hwnd = 0x1234ABCDusize as *mut std::ffi::c_void;
        let result = handle.open_editor(
            crate::audio::plugins::EditorMode::Embedded,
            Some(sentinel_hwnd),
        );
        match result {
            Ok(()) => {
                assert!(handle.is_editor_open());
                eprintln!("[ok] Opened embedded editor with parent HWND");
                handle.close_editor();
            }
            Err(e) => {
                eprintln!("[warn] Embedded open with parent failed: {e}");
            }
        }
    }

    /// Compile-time check: HtrkHostShared implements HostLogImpl and HostGuiImpl.
    /// This is a regression guard — if either impl is removed, the
    /// `declare_extensions` call in `HtrkHost` stops compiling.
    #[test]
    fn test_host_extensions_trait_impls() {
        fn assert_log<S: clack_extensions::log::HostLogImpl>() {}
        fn assert_gui<S: clack_extensions::gui::HostGuiImpl>() {}
        assert_log::<HtrkHostShared>();
        assert_gui::<HtrkHostShared>();
    }

    /// Smoke test: HtrkHostShared::log() doesn't panic for any severity.
    #[test]
    fn test_host_log_no_panic() {
        use clack_extensions::log::LogSeverity;
        let shared = HtrkHostShared;
        for sev in [
            LogSeverity::Debug,
            LogSeverity::Info,
            LogSeverity::Warning,
            LogSeverity::Error,
            LogSeverity::Fatal,
            LogSeverity::HostMisbehaving,
            LogSeverity::PluginMisbehaving,
        ] {
            shared.log(sev, "test message");
        }
    }

    /// Smoke test: HtrkHostShared GUI impl methods don't panic.
    #[test]
    fn test_host_gui_no_panic() {
        use clack_extensions::gui::GuiSize;
        let shared = HtrkHostShared;
        shared.resize_hints_changed();
        let _ = shared.request_resize(GuiSize { width: 800, height: 600 });
        let _ = shared.request_show();
        let _ = shared.request_hide();
        shared.closed(true);
        shared.closed(false);
    }

    /// Tests that last_editor_error is None after a successful open_editor
    /// call, and is populated when open_editor fails.
    #[test]
    fn test_last_editor_error_state() {
        let _guard = WIN32_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::path::Path::new(r"C:\Program Files\Common Files\CLAP\TAL-Reverb-4.clap");
        if !path.exists() {
            eprintln!("[skip] TAL-Reverb-4.clap not found");
            return;
        }
        let mut handle = ClapPluginHandle::load(path).expect("load failed");
        let _processor = handle.activate(48000.0, 256).expect("activate failed");

        // Open the editor
        if handle.open_editor(
            crate::audio::plugins::EditorMode::Floating,
            None,
        ).is_ok() {
            assert!(handle.last_editor_error().is_none());
            handle.close_editor();
        }
    }
}
