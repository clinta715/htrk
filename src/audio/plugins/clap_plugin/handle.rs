use super::*;

// ── Main-thread Handle ──

pub struct ClapPluginHandle {
    instance: Option<PluginInstance<HtrkHost>>,
    descriptor: PluginDescriptor,
    activated: bool,
    editor_open: bool,
    /// Which editor mode is currently in use (None if not open).
    editor_mode: Option<crate::audio::plugins::EditorMode>,
    /// Last error from `open_editor` (e.g. plugin doesn't support any GUI mode,
    /// or HWND creation failed). Surfaced to the UI so the user sees what went
    /// wrong instead of a silent failure.
    last_editor_error: Option<String>,
    /// Cached parameter info, populated lazily on the first call to
    /// `parameter_info()`. Avoids re-querying the plugin on every
    /// UI frame (the query requires a main-thread `plugin_handle()` call
    /// which is relatively expensive due to FFIs).
    cached_param_info: Vec<ParamInfo>,
    /// Main-thread producer half of the parameter ring. The audio-thread
    /// `ClapPluginProcessor` owns the matching consumer and feeds the queued
    /// values to the plugin as `ParamValueEvent`s.
    param_tx: std::cell::RefCell<ParamSender>,
    /// Audio-thread consumer half, handed to the processor in `activate()`.
    /// Holding it here guarantees a consumer is only ever handed out once.
    param_rx: Option<ParamReceiver>,
    /// Monotonic counter to assign a stable host-side index to each
    /// discovered parameter. This index is what the UI uses to refer to
    /// a specific param (e.g. for automation targets). The underlying
    /// CLAP `ClapId` is opaque and not stable across rescans.
    param_index_to_id: Vec<u32>,
    /// The plugin's state extension, if the plugin implements the CLAP
    /// state extension. Populated during `load()`. Used by `save_state()`
    /// and `load_state()`.
    state_ext: Option<clack_extensions::state::PluginState>,
    /// Container window for embedded-mode plugin GUI (the only mode most
    /// CLAP plugins support). Created in `open_editor()` when the plugin
    /// does not support floating mode. Destroyed in `close_editor()`.
    #[cfg(windows)]
    host_container: Option<crate::audio::plugins::plugin_window::PluginHostWindow>,
}

impl ClapPluginHandle {
    /// Load a CLAP plugin from disk and instantiate it. Discovers the first plugin in the bundle.
    ///
    /// On Windows, CLAP plugins can be packaged as either:
    /// - A single `.clap` DLL file directly at the path
    /// - A directory with `.clap` extension containing a DLL of the same name
    ///
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

        let host_info = HostInfo::new(
            "htrk",
            "htrk",
            "https://github.com/clinta715/htrk",
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(|e| PluginError::LoadFailed(e.to_string()))?;

        let plugin_id = clap_descriptor
            .id()
            .ok_or(PluginError::LoadFailed("Plugin missing id".into()))?
            .to_owned();

        let instance = PluginInstance::<HtrkHost>::new(
            |_| HtrkHostShared::new(),
            |_| HtrkMainThread,
            &entry,
            &plugin_id,
            &host_info,
        )
        .map_err(|e| PluginError::LoadFailed(e.to_string()))?;

        // Capture the plugin's state extension (if any) so we can
        // save/restore state from the main thread without needing to
        // access the instance's shared handler every time.
        let state_ext: Option<clack_extensions::state::PluginState> =
            instance.access_shared_handler(|h| h.state_ext.get().and_then(|o| o.as_ref().cloned()));

        let descriptor = extract_descriptor(path, clap_descriptor);
        let (param_tx, param_rx) = param_channel(256);

        Ok(ClapPluginHandle {
            instance: Some(instance),
            descriptor,
            activated: false,
            editor_open: false,
            editor_mode: None,
            last_editor_error: None,
            cached_param_info: Vec::new(),
            param_tx: std::cell::RefCell::new(param_tx),
            param_rx: Some(param_rx),
            param_index_to_id: Vec::new(),
            state_ext,
            #[cfg(windows)]
            host_container: None,
        })
    }

    /// Returns the descriptor.
    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
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
        self.param_tx.borrow_mut().push(ParamChange {
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
        let Some(mut_instance) = (unsafe { (raw_ptr as *mut PluginInstance<HtrkHost>).as_ref() })
        else {
            return 0.0;
        };
        let raw_mut = raw_ptr as *mut PluginInstance<HtrkHost>;
        // Gracefully handle a null instance pointer rather than panicking
        // at this CLAP FFI boundary; the preceding as_ref() check should
        // make this unreachable, but a misbehaving plugin could still
        // return a null extension pointer.
        let Some(mut_instance_mut) = (unsafe { raw_mut.as_mut() }) else {
            return 0.0;
        };
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
    Ok(extract_descriptor(path, clap_descriptor))
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
    } else if features
        .iter()
        .any(|f| f.contains("effect") || f == "analyzer")
    {
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

    fn activate(
        &mut self,
        sample_rate: f64,
        max_block: u32,
    ) -> Result<Box<dyn HostedPluginProcessor>, String> {
        let instance = self
            .instance
            .as_mut()
            .ok_or_else(|| "Plugin not loaded".to_string())?;

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

        // Hand the consumer half to the processor. If this plugin was
        // previously activated without a clean deactivate, the old consumer
        // is gone; rebuild a fresh ring so both halves still match.
        if self.param_rx.is_none() {
            let (tx, rx) = param_channel(256);
            self.param_tx = std::cell::RefCell::new(tx);
            self.param_rx = Some(rx);
        }
        let param_rx = self.param_rx.take().expect("param consumer missing");

        Ok(Box::new(ClapPluginProcessor::new(
            started,
            self.descriptor.clone(),
            sample_rate,
            max_block as usize,
            param_rx,
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

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        let state_ext = self
            .state_ext
            .as_ref()
            .ok_or("Plugin does not support state extension")?;
        let instance = self.instance.as_mut().ok_or("No plugin instance")?;
        let mut handle = instance.plugin_handle();
        let mut buffer = Vec::new();
        state_ext
            .save(&mut handle, &mut buffer)
            .map_err(|e| format!("State save failed: {e}"))?;
        Ok(buffer)
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), String> {
        let state_ext = self
            .state_ext
            .as_ref()
            .ok_or("Plugin does not support state extension")?;
        let instance = self.instance.as_mut().ok_or("No plugin instance")?;
        let mut handle = instance.plugin_handle();
        let mut reader = Cursor::new(state);
        state_ext
            .load(&mut handle, &mut reader)
            .map_err(|e| format!("State load failed: {e}"))?;
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
        _mode: crate::audio::plugins::EditorMode,
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

        // ── Try floating mode first ──
        let float_config = GuiConfiguration {
            api_type: GuiApiType::WIN32,
            is_floating: true,
        };
        if gui_ext.is_api_supported(&mut handle, float_config) {
            let parent_hwnd_ptr: *mut std::ffi::c_void =
                parent_hwnd.unwrap_or(std::ptr::null_mut());
            gui_ext
                .create(&mut handle, float_config)
                .map_err(|e| format!("Plugin GUI create failed: {e:?}"))?;
            if !parent_hwnd_ptr.is_null() {
                let host_win = ClapWindow::from_win32_hwnd(parent_hwnd_ptr as *mut _);
                unsafe {
                    let _ = gui_ext.set_transient(&mut handle, host_win);
                }
            }
            let title = std::ffi::CString::new(self.descriptor.name.as_str())
                .unwrap_or_else(|_| std::ffi::CString::new("Plugin").unwrap());
            gui_ext.suggest_title(&mut handle, &title);
            let _ = gui_ext.show(&mut handle);

            self.editor_open = true;
            self.editor_mode = Some(crate::audio::plugins::EditorMode::Floating);
            return Ok(());
        }

        // ── Fall back to embedded mode ──
        let embed_config = GuiConfiguration {
            api_type: GuiApiType::WIN32,
            is_floating: false,
        };
        if !gui_ext.is_api_supported(&mut handle, embed_config) {
            let err = "Plugin does not support any GUI mode (floating or embedded)".to_string();
            self.last_editor_error = Some(err.clone());
            return Err(err);
        }
        tracing::info!(target: "htrk::audio::plugins::clap_plugin", "embedded mode supported, calling gui_ext.create(embedded)");
        // Some plugins (e.g. JC303) have issues with the create → set_parent
        // → show embedded flow through clack's wrapper. If set_parent fails,
        // we skip it and just do create + show, which still opens the plugin's
        // native GUI in a container window.
        let gui_result = gui_ext.create(&mut handle, embed_config);
        match gui_result {
            Ok(()) => {
                tracing::info!(target: "htrk::audio::plugins::clap_plugin", "create succeeded, creating container");
                let container = crate::audio::plugins::plugin_window::PluginHostWindow::create(
                    &self.descriptor.name,
                    crate::audio::plugins::plugin_window::WindowMode::TopLevel,
                    800,
                    600,
                );
                if let Some(ref win) = container {
                    let clap_win = ClapWindow::from_win32_hwnd(win.hwnd() as *mut _);
                    tracing::info!(target: "htrk::audio::plugins::clap_plugin", "calling set_parent");
                    let sp_result = unsafe { gui_ext.set_parent(&mut handle, clap_win) };
                    if let Err(e) = sp_result {
                        tracing::warn!(target: "htrk::audio::plugins::clap_plugin", "set_parent failed (showing without parent): {e:?}");
                    }
                }
                tracing::info!(target: "htrk::audio::plugins::clap_plugin", "calling show");
                let _ = gui_ext.show(&mut handle);

                // Diagnostic: log window hierarchy
                #[cfg(windows)]
                if let Some(ref win) = container {
                    let container_hwnd = win.hwnd();
                    // SAFETY: We own the container HWND; it is valid.
                    let child_hwnd = unsafe {
                        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindow, GW_CHILD};
                        GetWindow(container_hwnd, GW_CHILD)
                    };
                    // SAFETY: We own the container HWND; all Win32 calls are on our windows.
                    unsafe {
                        use windows_sys::Win32::Foundation::{HWND, RECT};
                        use windows_sys::Win32::UI::WindowsAndMessaging::{
                            GetAncestor, GetParent, GetWindowRect, GetWindowTextW, IsChild,
                            IsWindowVisible, GA_PARENT,
                        };
                        let log_one = |h: HWND, label: &str| {
                            let mut rect = RECT {
                                left: 0,
                                top: 0,
                                right: 0,
                                bottom: 0,
                            };
                            GetWindowRect(h, &mut rect);
                            let parent = GetParent(h);
                            let ancestor = GetAncestor(h, GA_PARENT);
                            let visible = IsWindowVisible(h) != 0;
                            let mut buf = [0u16; 256];
                            let len = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
                            let title = if len > 0 {
                                String::from_utf16_lossy(&buf[..len as usize])
                            } else {
                                String::new()
                            };
                            tracing::info!(
                                target: "htrk::audio::plugins::clap_plugin",
                                "{label}: hwnd={h:?} parent={parent:?} ancestor={ancestor:?} visible={visible} rect=({},{},{},{}) title=\"{title}\"",
                                rect.left, rect.top, rect.right, rect.bottom
                            );
                        };
                        log_one(container_hwnd, "container");
                        if !child_hwnd.is_null() {
                            log_one(child_hwnd, "plugin_child");
                            let is_ch = IsChild(container_hwnd, child_hwnd);
                            tracing::info!(target: "htrk::audio::plugins::clap_plugin", "  is_child_of_container={is_ch}");
                        } else {
                            tracing::info!(target: "htrk::audio::plugins::clap_plugin", "  plugin_child=(none)");
                        }
                    }
                }

                self.host_container = container;
                self.editor_open = true;
                self.editor_mode = Some(crate::audio::plugins::EditorMode::Floating);
                Ok(())
            }
            Err(e) => {
                let err = format!("Plugin GUI create (embedded) failed: {e:?}");
                tracing::error!(target: "htrk::audio::plugins::clap_plugin", "{err}");
                self.last_editor_error = Some(err.clone());
                Err(err)
            }
        }
    }

    #[cfg(not(windows))]
    fn open_editor(&mut self, mode: crate::audio::plugins::EditorMode) -> Result<(), String> {
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
        gui_ext
            .create(&mut handle, config)
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
        #[cfg(windows)]
        {
            self.host_container = None; // Drop destroys the container window
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
        // Gracefully handle a null instance pointer at this FFI boundary
        // rather than panicking; the preceding as_ref() check should make
        // this unreachable, but be defensive with plugin host code.
        let Some(mut_instance) = (unsafe { (raw_ptr as *mut PluginInstance<HtrkHost>).as_mut() })
        else {
            return false;
        };
        let handle = mut_instance.plugin_handle();
        // Just check whether the plugin exposes the GUI extension at all.
        // We do NOT probe is_api_supported here — some plugins (JC303)
        // don't handle repeated is_api_supported calls well, which can
        // interfere with the subsequent create() in open_editor. Actual
        // mode selection (floating vs embedded) happens in open_editor.
        handle.get_extension::<PluginGuiExt>().is_some()
    }

    fn editor_mode(&self) -> Option<crate::audio::plugins::EditorMode> {
        self.editor_mode
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
        let Some(mut_instance) = (unsafe { (raw_ptr as *mut PluginInstance<HtrkHost>).as_mut() })
        else {
            return Vec::new();
        };
        let mut handle = mut_instance.plugin_handle();
        let Some(params) = handle.get_extension::<PluginParams>() else {
            return Vec::new();
        };
        let count = params.count(&mut handle);
        if count == 0 {
            return Vec::new();
        }
        let mut buf = ParamInfoBuffer::new();
        let mut info_out: Vec<ParamInfo> = Vec::with_capacity(count as usize);
        let mut index_to_id: Vec<u32> = Vec::with_capacity(count as usize);
        for i in 0..count {
            if let Some(info) = params.get_info(&mut handle, i, &mut buf) {
                let id = info.id.get();
                let name = String::from_utf8_lossy(info.name).into_owned();
                let is_automatable = info
                    .flags
                    .contains(clack_extensions::params::ParamInfoFlags::IS_AUTOMATABLE);
                let is_modulatable = info
                    .flags
                    .contains(clack_extensions::params::ParamInfoFlags::IS_MODULATABLE);
                index_to_id.push(id);
                info_out.push(ParamInfo {
                    id,
                    name,
                    min: info.min_value as f32,
                    max: info.max_value as f32,
                    default: info.default_value as f32,
                    is_automatable,
                    is_modulatable,
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

impl Drop for ClapPluginHandle {
    /// Close the editor before the instance is dropped, so the plugin's GUI
    /// resources are released cleanly via its `destroy` callback.
    fn drop(&mut self) {
        if self.editor_open {
            self.close_editor();
        }
    }
}
