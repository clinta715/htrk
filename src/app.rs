use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::RwLock;

use eframe::egui;
use eguidev::{DevMcp, DevUiExt, FixtureSpec, FrameGuard};

use crate::app_config::AppConfig;
use crate::audio::commands::AudioCommand;
use crate::audio::engine::create_engine_and_sender;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::plugins::{PluginLibrary, PresetLibrary};
use crate::mcp::library::SampleLibrary;

use crate::sequencer::pattern::Cell;
use crate::sequencer::{DEFAULT_CHANNELS, MAX_CHANNELS};
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::audio::plugins::HostedPluginHandle;
use crate::ui::file_browser::{BrowserMode, FileBrowser};
use crate::ui::pattern_grid::{ColumnVisibility, Selection, SubColumn};
use crate::ui::panel_event::PanelEvent;
use crate::ui::theme::ThemePreset;
use crate::ui::TrackerTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppView {
    Pattern,
    Sample,
    Instrument,
    SendFx,
    Playback,
    Automation,
    Mixer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserPurpose {
    General,
    LoadInstrument,
    SaveInstrument,
    ExportInstrument(usize),
    SaveProject,
}

impl Default for BrowserPurpose {
    fn default() -> Self {
        BrowserPurpose::General
    }
}

pub struct HtrkApp {
    pub(crate) core: crate::core::HtrkCore,
    pub(crate) stream: Option<cpal::Stream>,

    pub(crate) output_device_names: Vec<String>,
    pub(crate) selected_device_name: Option<String>,
    pub(crate) current_sample_rate: u32,
    pub(crate) current_sample_format: String,
    pub(crate) pending_device_switch: Option<String>,
    pub(crate) pending_reinit: bool,

    pub(crate) file_browser: FileBrowser,
    pub(crate) browser_purpose: BrowserPurpose,

    pub(crate) current_view: AppView,

    pub(crate) pattern_view: crate::ui::pattern_view::PatternView,

    pub(crate) current_octave: u8,
    pub(crate) edit_mode: bool,
    pub(crate) follow_playback: bool,
    pub(crate) cursor_skip: u8,
    pub(crate) multichannel_enabled: bool,
    pub(crate) multichannel_channels: Vec<bool>,

    pub(crate) theme: TrackerTheme,
    pub(crate) theme_preset: crate::ui::theme::ThemePreset,
    pub(crate) show_shortcuts: bool,
    pub(crate) show_about: bool,
    pub(crate) settings_state: crate::ui::settings_window::SettingsState,
    pub(crate) wav_export_state: crate::ui::wav_export_window::WavExportState,
    pub(crate) sample_export_dialog: Option<crate::ui::sample_export_dialog::SampleExportDialog>,
    pub(crate) audio_init_failed: bool,
    pub(crate) sample_editor: crate::ui::sample_editor_panel::SampleEditor,
    pub(crate) config: AppConfig,
    pub(crate) col_vis: ColumnVisibility,
    pub(crate) playback_view: crate::ui::playback_view_panel::PlaybackView,
    pub(crate) sendfx_panel: crate::ui::sendfx_panel::SendFxPanel,
    /// Main-thread plugin handles, one per send bus. Used to open/close
    /// plugin editor windows. The PluginInstance is `!Send` so these must
    /// stay on the main thread alongside HtrkApp.
    /// Phase 5 plugin hosting.
    pub(crate) send_bus_handles: [Option<Box<dyn HostedPluginHandle>>; NUM_SEND_BUSES],
    /// Main-thread plugin handles for instrument-backed plugins, indexed
    /// by 1-based instrument index. `PluginInstance` is `!Send` so these
    /// must stay on the main thread alongside HtrkApp.
    /// Phase 3 plugin instruments.
    pub(crate) instrument_plugin_handles: Vec<Option<Box<dyn HostedPluginHandle>>>,
    /// Notes currently held by the keyboard preview system for instrument
    /// plugins. Each entry maps the egui key that triggered the note-on
    /// to the (instrument, midi_channel, note_key) so the per-frame
    /// release check can send note-off when the key is no longer down.
    pub(crate) preview_held_notes: Vec<(egui::Key, u8, u8, u8)>,
    /// Pending (send_index, state_blob) writes from the send-fx editor's
    /// "Remove" button. The editor can't write to the module directly
    /// because it doesn't own a `&mut HtrkApp`, so it pushes here and
    /// `drain_send_bus_state_writes` flushes the queue right after the
    /// editor's `ui()` call returns.
    pub(crate) pending_send_bus_state_writes: Vec<(usize, Vec<u8>)>,
    /// Direct in-memory plugin library, independent of the MCP server.
    /// Populated by `rescan_plugins()` at startup and on user request. The
    /// Send FX plugin browser reads from this so plugins are visible
    /// without enabling MCP. MCP's `plugin_library` mirrors this for the
    /// read-only MCP tools.
    pub(crate) plugin_library: PluginLibrary,
    /// True after the initial plugin scan has run. The scan happens on
    /// the first `update()` call so the constructor doesn't need to
    /// mutate `self`.
    pub(crate) plugin_scan_done: bool,
    /// Direct in-memory preset library (CLAP preset-discovery cache).
    /// Populated by `rescan_presets()`; read by MCP tools and UI.
    /// Shared with the MCP server via the same `Arc<RwLock<>>` so MCP
    /// `preset.scan` writes are visible to the UI without any explicit
    /// mirroring step.
    pub(crate) preset_library: Arc<RwLock<PresetLibrary>>,
    /// True after the initial preset scan has run.
    pub(crate) preset_scan_done: bool,
    /// Status of the plugin browser dialog (loading / error / loaded).
    /// Phase 2 plugin hosting.
    pub(crate) plugin_browser_status: crate::ui::plugin_browser::PluginBrowserStatus,
    pub(crate) alt_l_count: u8,
    pub(crate) alt_l_last: Option<std::time::Instant>,
    pub(crate) automation_editor: crate::ui::automation_editor_panel::AutomationEditor,
    pub(crate) instrument_editor: crate::ui::instrument_editor_panel::InstrumentEditor,
    pub(crate) mixer_state: crate::ui::mixer_view::MixerState,
    pub(crate) devmcp: Arc<DevMcp>,
    pub(crate) pending_view_switch: Arc<AtomicU8>,
    pub(crate) show_exit_confirm: bool,
    pub(crate) exit_confirmed: bool,
    pub(crate) close_after_save: bool,
    pub(crate) show_phrase_generator: bool,
    pub(crate) slice_dialog_open: bool,
    pub(crate) sample_library_state: crate::ui::sample_library::SampleLibraryState,
    pub(crate) sample_library: Arc<RwLock<SampleLibrary>>,
    pub(crate) slice_config: crate::actions::slice_to_instrument::SliceConfig,

    pub(crate) mcp_server: Option<crate::mcp::McpServer>,
}

impl HtrkApp {
    pub fn with_config(config: AppConfig) -> Self {
        Self::from_config(config)
    }
}

impl Default for HtrkApp {
    fn default() -> Self {
        Self::from_config(AppConfig::load())
    }
}

impl HtrkApp {
    /// Test-only constructor that builds an HtrkApp without running the
    /// default's MCP/audio side effects. Uses an in-memory atomic
    /// playback state and skips the user-config restoration.
    #[cfg(test)]
    pub fn from_config_for_tests(config: AppConfig) -> Self {
        let playback_state = std::sync::Arc::new(crate::audio::AtomicPlaybackState::default());
        let devmcp = std::sync::Arc::new(DevMcp::new());
        HtrkApp {
            core: crate::core::HtrkCore::new(playback_state),
            stream: None,
            output_device_names: Vec::new(),
            selected_device_name: None,
            current_sample_rate: 0,
            current_sample_format: String::new(),
            pending_device_switch: None,
            pending_reinit: false,
            file_browser: crate::ui::file_browser::FileBrowser::default(),
            browser_purpose: BrowserPurpose::General,
            current_view: AppView::Pattern,
            pattern_view: crate::ui::pattern_view::PatternView::default(),
            current_octave: 4,
            edit_mode: true,
            follow_playback: false,
            cursor_skip: 1,
            alt_l_count: 0,
            alt_l_last: None,
            multichannel_enabled: false,
            multichannel_channels: vec![false; crate::sequencer::module::DEFAULT_CHANNELS],
            theme: TrackerTheme::from_preset(ThemePreset::DarkModern),
            theme_preset: ThemePreset::DarkModern,
            show_shortcuts: false,
            show_about: false,
            settings_state: crate::ui::settings_window::SettingsState::from_config(&config),
            wav_export_state: crate::ui::wav_export_window::WavExportState::new(44100),
            sample_export_dialog: None,
            audio_init_failed: false,
            sample_editor: crate::ui::sample_editor_panel::SampleEditor::default(),
            col_vis: config.get_col_vis(),
            config: config.clone(),
            playback_view: crate::ui::playback_view_panel::PlaybackView::default(),
            sendfx_panel: crate::ui::sendfx_panel::SendFxPanel::default(),
            send_bus_handles: [None, None, None, None],
            instrument_plugin_handles: (0..255).map(|_| None).collect(),
            preview_held_notes: Vec::new(),
            pending_send_bus_state_writes: Vec::new(),
            plugin_library: crate::audio::plugins::PluginLibrary::new(),
            plugin_scan_done: false,
            preset_library: Arc::new(RwLock::new(
                crate::audio::plugins::PresetLibrary::new()
            )),
            preset_scan_done: false,
            plugin_browser_status: crate::ui::plugin_browser::PluginBrowserStatus::Idle,
            automation_editor: crate::ui::automation_editor_panel::AutomationEditor::default(),
            instrument_editor: crate::ui::instrument_editor_panel::InstrumentEditor {
                list_width: config.instrument_list_width.unwrap_or(150.0),
                envelope_height: config.instrument_envelope_height.unwrap_or(180.0),
                ..crate::ui::instrument_editor_panel::InstrumentEditor::default()
            },
            mixer_state: crate::ui::mixer_view::MixerState::default(),
            devmcp,
            pending_view_switch: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            show_exit_confirm: false,
            exit_confirmed: false,
            close_after_save: false,
            show_phrase_generator: false,
            slice_dialog_open: false,
            sample_library_state: crate::ui::sample_library::SampleLibraryState::default(),
            sample_library: Arc::new(RwLock::new(SampleLibrary::new())),
            slice_config: crate::actions::slice_to_instrument::SliceConfig::default(),
            mcp_server: None,
        }
    }

    fn from_config(config: AppConfig) -> Self {
        let inst_list_w = config.instrument_list_width.unwrap_or(150.0);
        let inst_env_h = config.instrument_envelope_height.unwrap_or(180.0);
        let mut file_browser = FileBrowser::default();
        file_browser.restore_last_dirs(&config);
        file_browser.restore_favorites(&config.favorites);
        file_browser.restore_widths_from_config(&config);
        let playback_state = Arc::new(AtomicPlaybackState::default());
        let pending_view_switch = Arc::new(AtomicU8::new(0));
        let mcp_enabled = config.mcp_enabled;
        let mcp_port = config.mcp_port;
        let mcp_http_enabled = config.mcp_http_enabled;
        let mcp_http_port = config.mcp_http_port;
        let library_roots: Vec<std::path::PathBuf> = config.library_roots.iter()
            .map(std::path::PathBuf::from)
            .collect();
        let plugin_scan_paths: Vec<std::path::PathBuf> = config.plugin_scan_paths.iter()
            .map(std::path::PathBuf::from)
            .collect();
        // Shared with the MCP server so MCP `preset.scan` writes are
        // visible to the UI without any explicit mirroring step.
        let preset_library = Arc::new(RwLock::new(PresetLibrary::new()));
        let app = HtrkApp {
            core: crate::core::HtrkCore::new(playback_state.clone()),
            stream: None,
            output_device_names: Vec::new(),
            selected_device_name: None,
            current_sample_rate: 0,
            current_sample_format: String::new(),
            pending_device_switch: None,
            pending_reinit: false,
            file_browser,
            browser_purpose: BrowserPurpose::General,
            current_view: AppView::Pattern,
            pattern_view: crate::ui::pattern_view::PatternView::default(),
            current_octave: 4,
            edit_mode: true,
            follow_playback: config.follow_playback_default,
            cursor_skip: 1,
            alt_l_count: 0,
            alt_l_last: None,
            multichannel_enabled: false,
            multichannel_channels: vec![false; DEFAULT_CHANNELS],
            theme: TrackerTheme::from_preset(
                ThemePreset::from_name(&config.theme_preset).unwrap_or(ThemePreset::DarkModern)
            ),
            theme_preset: ThemePreset::from_name(&config.theme_preset).unwrap_or(ThemePreset::DarkModern),
            show_shortcuts: false,
            show_about: false,
            settings_state: crate::ui::settings_window::SettingsState::from_config(&config),
            wav_export_state: crate::ui::wav_export_window::WavExportState::new(44100),
            sample_export_dialog: None,
            audio_init_failed: false,
            sample_editor: crate::ui::sample_editor_panel::SampleEditor {
                amplify_factor: config.default_amplify_factor,
                list_width: config.sample_list_width.unwrap_or(200.0),
                waveform_height: config.sample_waveform_height.unwrap_or(150.0),
                ..crate::ui::sample_editor_panel::SampleEditor::default()
            },
            col_vis: config.get_col_vis(),
            config: config.clone(),
            playback_view: crate::ui::playback_view_panel::PlaybackView::default(),
            sendfx_panel: crate::ui::sendfx_panel::SendFxPanel::default(),
            send_bus_handles: [None, None, None, None],
            instrument_plugin_handles: (0..255).map(|_| None).collect(),
            preview_held_notes: Vec::new(),
            pending_send_bus_state_writes: Vec::new(),
            plugin_library: PluginLibrary::new(),
            plugin_scan_done: false,
            preset_library: preset_library.clone(),
            preset_scan_done: false,
            plugin_browser_status: crate::ui::plugin_browser::PluginBrowserStatus::Idle,
            automation_editor: crate::ui::automation_editor_panel::AutomationEditor::default(),
            instrument_editor: crate::ui::instrument_editor_panel::InstrumentEditor {
                list_width: inst_list_w,
                envelope_height: inst_env_h,
                ..crate::ui::instrument_editor_panel::InstrumentEditor::default()
            },
            mixer_state: crate::ui::mixer_view::MixerState::default(),
            pending_view_switch: pending_view_switch.clone(),
            show_exit_confirm: false,
            exit_confirmed: false,
            close_after_save: false,
            show_phrase_generator: false,
            slice_dialog_open: false,
            sample_library_state: crate::ui::sample_library::SampleLibraryState::default(),
            sample_library: Arc::new(RwLock::new(SampleLibrary::new())),
            slice_config: crate::actions::slice_to_instrument::SliceConfig::default(),
            devmcp: {
                let ps = pending_view_switch.clone();
                let devmcp = DevMcp::new()
                    .verbose_logging(true)
                    .fixtures([
                        FixtureSpec::new("empty_project", "A brand new empty project (default after launch)")
                            .anchor("view.pattern"),
                        FixtureSpec::new("pattern_view", "Switch to the pattern editor view")
                            .anchor("view.pattern"),
                        FixtureSpec::new("sample_view", "Switch to the sample editor view")
                            .anchor("view.sample"),
                        FixtureSpec::new("instrument_view", "Switch to the instrument editor view")
                            .anchor("view.instrument"),
                        FixtureSpec::new("sendfx_view", "Switch to the send FX editor view")
                            .anchor("view.sendfx"),
                        FixtureSpec::new("playback_view", "Switch to the playback monitoring view")
                            .anchor("view.playback"),
                        FixtureSpec::new("automation_view", "Switch to the automation editor view")
                            .anchor("view.automation"),
                    ])
                    .on_fixture(move |name| {
                        let view = match name {
                            "empty_project" | "pattern_view" => 1u8,
                            "sample_view" => 2,
                            "instrument_view" => 3,
                            "sendfx_view" => 4,
                            "playback_view" => 5,
                            "automation_view" => 6,
                            _ => return Err(format!("unknown fixture: {name}")),
                        };
                        ps.store(view, Ordering::Relaxed);
                        Ok(())
                    });

                #[cfg(feature = "devtools")]
                let devmcp = eguidev_runtime::attach(devmcp);

                Arc::new(devmcp)
            },
            mcp_server: if mcp_enabled {
                let http_port = if mcp_http_enabled { Some(mcp_http_port) } else { None };
                let server = crate::mcp::McpServer::start(
                    mcp_port,
                    http_port,
                    preset_library.clone(),
                );
                eprintln!("[app] MCP server enabled (TCP port {})", server.port);
                if let Some(hp) = server.http_port {
                    eprintln!("[app] MCP HTTP SSE transport enabled (port {hp})");
                }
                if !library_roots.is_empty() {
                    if let Ok(mut lib) = server.library.write() {
                        lib.set_roots(library_roots.clone());
                        eprintln!("[app] Sample library configured with {} root(s)", library_roots.len());
                    }
                }
                // Configure plugin library scan paths and trigger an initial scan.
                // The scan finds .clap files; loading each descriptor requires
                // a dlopen + clack factory call, which is expensive. We do the
                // initial scan on the main thread to populate file paths, then
                // the UI triggers per-file descriptor extraction on demand
                // (e.g. via the Plugin browser's "Rescan" button).
                let plugin_scan_paths = plugin_scan_paths.clone();
                if let Ok(mut lib) = server.plugin_library.write() {
                    lib.set_scan_roots(plugin_scan_paths.clone());
                    let found = lib.scan();
                    eprintln!("[app] Plugin library: {} .clap file(s) found in {} root(s)",
                        found.len(), plugin_scan_paths.len() + crate::audio::plugins::default_search_paths().len());

                    // Probe each .clap file to extract its descriptor. This
                    // is done on the main thread (one-time cost at startup).
                    // For very large plugin collections this can be slow;
                    // future work can make it incremental.
                    for path in &found {
                        if let Ok(descriptor) =
                            crate::audio::plugins::clap_plugin::extract_descriptor_for_browser(path)
                        {
                            lib.add_descriptor(descriptor);
                        }
                    }
                    eprintln!("[app] Plugin library: {} descriptor(s) loaded", lib.descriptor_count());
                }
                Some(server)
            } else {
                None
            },
            // Initial plugin scan is triggered from `update()` so the
            // struct constructor doesn't need to mutate itself.
        };

        // Initialize the app's sample library roots from config
        // (separate from the MCP server's library which is initialized above).
        let lib_roots = library_roots;
        if let Ok(mut lib) = app.sample_library.write() {
            lib.set_roots(lib_roots);
        }

        app
    }
}

impl HtrkApp {
    /// Returns the list of discovered CLAP plugins. Reads from the direct
    /// `plugin_library` field (populated at startup + on rescan). Always
    /// works, regardless of whether MCP is enabled.
    pub fn discovered_plugins(&self) -> Vec<crate::audio::plugins::PluginDescriptor> {
        self.plugin_library.list_descriptors()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Trigger a rescan of all configured plugin scan paths plus the system
    /// default paths. For each `.clap` file found, extract the descriptor
    /// (real metadata, not stubs) and store it in `self.plugin_library`.
    ///
    /// Returns a summary string suitable for displaying in the UI.
    /// Also pushes the new descriptors into the MCP library (if MCP is
    /// enabled) so MCP tools see the same data.
    pub fn rescan_plugins(&mut self) -> String {
        use crate::audio::plugins::{default_search_paths, discovery};
        use std::path::PathBuf;

        let mut scan_roots: Vec<PathBuf> = default_search_paths();
        for p in &self.config.plugin_scan_paths {
            scan_roots.push(PathBuf::from(p));
        }

        // De-dupe roots
        scan_roots.sort();
        scan_roots.dedup();

        eprintln!("[plugins] Scanning {} root(s):", scan_roots.len());
        for r in &scan_roots {
            eprintln!("  - {}", r.display());
        }

        let scan = discovery::scan_paths(&scan_roots);
        eprintln!("[plugins] Found {} .clap file(s) ({} errors)",
            scan.clap_files.len(), scan.errors.len());

        // Update the direct plugin library with REAL descriptors
        self.plugin_library.set_scan_roots(scan_roots.clone());
        self.plugin_library.clear_cache();

        let mut found_count = 0;
        let mut error_count = 0;
        let mut sample_names: Vec<String> = Vec::new();
        for path in &scan.clap_files {
            match crate::audio::plugins::clap_plugin::extract_descriptor_for_browser(path) {
                Ok(d) => {
                    if sample_names.len() < 5 {
                        sample_names.push(format!("{} ({})", d.name, d.vendor));
                    }
                    self.plugin_library.add_descriptor(d);
                    found_count += 1;
                }
                Err(e) => {
                    eprintln!("[plugins] Failed to probe {}: {}", path.display(), e);
                    error_count += 1;
                }
            }
        }

        // Mirror to MCP library if MCP is enabled, so MCP tools see the
        // same data. The MCP library is read-only from MCP's perspective.
        if let Some(ref mcp) = self.mcp_server {
            if let Ok(mut mcp_lib) = mcp.plugin_library.write() {
                mcp_lib.set_scan_roots(scan_roots.clone());
                mcp_lib.clear_cache();
                for d in self.plugin_library.list_descriptors() {
                    mcp_lib.add_descriptor(d.clone());
                }
            }
        }

        eprintln!(
            "[plugins] Done: {} descriptor(s) loaded ({} probe errors). Sample: {}",
            found_count, error_count,
            if sample_names.is_empty() { "(none)".to_string() } else { sample_names.join(", ") }
        );

        format!(
            "Scan complete: {} plugin(s) found, {} error(s), {} .clap file(s) on disk",
            found_count, error_count, scan.clap_files.len()
        )
    }

    /// Trigger a rescan of all discovered plugins for presets.
    /// Only plugins that expose the preset-discovery factory are scanned.
    /// Results are stored in `self.preset_library`.
    pub fn rescan_presets(&mut self) -> String {
        use crate::audio::plugins::preset_discovery;

        // Collect all discovered plugin .clap paths
        let paths: Vec<std::path::PathBuf> = self
            .plugin_library
            .list_descriptors()
            .iter()
            .map(|d| d.path.clone())
            .collect();

        eprintln!("[presets] Scanning {} plugin(s) for presets...", paths.len());
        let start = std::time::Instant::now();

        let (entries, errors) = preset_discovery::scan_plugins_for_presets(&paths);

        let elapsed = start.elapsed();
        // The preset_library Arc is shared with the MCP server, so this
        // single write is visible to both the UI and MCP tools — no
        // explicit mirror step is needed.
        let count = if let Ok(mut lib) = self.preset_library.write() {
            lib.clear();
            lib.add_presets(entries);
            lib.set_last_scan_time(std::time::SystemTime::now());
            lib.preset_count()
        } else {
            0
        };

        eprintln!(
            "[presets] Done: {} preset(s) from {} plugin(s), {} error(s) in {:.1}s",
            count,
            paths.len(),
            errors.len(),
            elapsed.as_secs_f32()
        );

        format!(
            "Preset scan complete: {} preset(s) found from {} plugin(s), {} error(s) in {:.1}s",
            count,
            paths.len(),
            errors.len(),
            elapsed.as_secs_f32()
        )
    }

    pub fn refresh_output_devices(&mut self) {
        use cpal::traits::{HostTrait, DeviceTrait};
        let host = cpal::default_host();
        self.output_device_names = host
            .output_devices()
            .map(|iter| iter.filter_map(|d| d.description().ok().map(|desc| desc.to_string())).collect())
            .unwrap_or_default();
        if self.selected_device_name.is_none() {
            self.selected_device_name = host.default_output_device().and_then(|d| d.description().ok().map(|desc| desc.to_string()));
        }
    }

    pub fn init_audio(&mut self) {
        if self.stream.is_some() {
            return;
        }
        if self.output_device_names.is_empty() {
            self.refresh_output_devices();
        }
        // Initialize selected device from persisted config if not set
        if self.selected_device_name.is_none() {
            if let Some(ref name) = self.config.output_device_name {
                if self.output_device_names.iter().any(|d| d == name) {
                    self.selected_device_name = Some(name.clone());
                }
            }
        }

        use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};

        let host = cpal::default_host();
        let device = if let Some(ref name) = self.selected_device_name {
            host.output_devices()
                .ok()
                .and_then(|mut devs| devs.find(|d| d.description().ok().map(|desc| desc.to_string()).as_deref() == Some(name.as_str())))
        } else {
            None
        };
        let device = device.or_else(|| host.default_output_device());

        let device = match device {
            Some(d) => {
                #[cfg(feature = "audio_debug")]
                debug_log!("[AUDIO] Using device: {:?}", d.description().ok().map(|desc| desc.to_string()));
                d
            }
            None => {
                eprintln!("[AUDIO] No audio output device available");
                self.audio_init_failed = true;
                return;
            }
        };

        let supported_config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[AUDIO] Failed to get default output config: {}", e);
                self.audio_init_failed = true;
                return;
            }
        };

        let actual_sample_rate = supported_config.sample_rate();
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        #[cfg(feature = "audio_debug")]
        debug_log!("[AUDIO] Sample rate: {}, format: {:?}, channels: {}", actual_sample_rate, sample_format, config.channels);

        self.current_sample_rate = actual_sample_rate;
        self.current_sample_format = format!("{:?}", sample_format);
        if self.selected_device_name.is_none() {
            self.selected_device_name = device.description().ok().map(|desc| desc.to_string());
        }

        let mut configs_to_try = Vec::new();

        // If user has a preferred sample rate, try it first
        if let Some(pref_sr) = self.config.preferred_sample_rate {
            configs_to_try.push(cpal::StreamConfig {
                channels: config.channels,
                sample_rate: pref_sr,
                buffer_size: cpal::BufferSize::Default,
            });
        }

        configs_to_try.push(config.clone());
        let alt1 = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: config.sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };
        if alt1 != config {
            configs_to_try.push(alt1);
        }
        for &sr in &[44100u32, 48000, 22050] {
            if Some(sr) != self.config.preferred_sample_rate && sr != config.sample_rate {
                configs_to_try.push(cpal::StreamConfig {
                    channels: 2,
                    sample_rate: sr,
                    buffer_size: cpal::BufferSize::Default,
                });
            }
        }

        let mut stream_result = Err(cpal::BuildStreamError::DeviceNotAvailable);
        let mut sender = None;
        for trial_config in &configs_to_try {
            let state = self.core.playback_state.clone();
            let (mut engine, trial_sender) = create_engine_and_sender(state, trial_config.sample_rate, trial_config.channels);

            let trial_result = match sample_format {
                cpal::SampleFormat::F32 => device.build_output_stream(
                    trial_config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        engine.process_callback(data);
                    },
                    |err| eprintln!("Audio stream error: {}", err),
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_output_stream(
                    trial_config,
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        let mut float_buf = vec![0.0f32; data.len()];
                        engine.process_callback(&mut float_buf);
                        for (out, inp) in data.iter_mut().zip(float_buf.iter()) {
                            *out = (*inp * 32768.0).clamp(-32768.0, 32767.0) as i16;
                        }
                    },
                    |err| eprintln!("Audio stream error: {}", err),
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_output_stream(
                    trial_config,
                    move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                        let mut float_buf = vec![0.0f32; data.len()];
                        engine.process_callback(&mut float_buf);
                        for (out, inp) in data.iter_mut().zip(float_buf.iter()) {
                            let s = (*inp * 32768.0).clamp(-32768.0, 32767.0) as i16;
                            *out = (s as i32 + 32768) as u16;
                        }
                    },
                    |err| eprintln!("Audio stream error: {}", err),
                    None,
                ),
                _ => {
                    eprintln!("Unsupported sample format: {:?}", sample_format);
                    self.audio_init_failed = true;
                    return;
                }
            };

            match trial_result {
                Ok(s) => {
                    stream_result = Ok(s);
                    sender = Some(trial_sender);
                    break;
                }
                Err(e) => {
                    eprintln!("[AUDIO] Config {}ch/{}Hz failed: {}", trial_config.channels, trial_config.sample_rate, e);
                    stream_result = Err(e);
                }
            }
        }

        match stream_result {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    eprintln!("[AUDIO] Failed to start audio stream: {}", e);
                    self.audio_init_failed = true;
                    return;
                }
                #[cfg(feature = "audio_debug")]
                debug_log!("[AUDIO] Audio stream started successfully");
                self.audio_init_failed = false;
                self.current_sample_rate = actual_sample_rate;
                self.current_sample_format = format!("{:?}", sample_format);
                self.stream = Some(stream);
                self.core.command_sender = sender;
                if let Some(ref module) = self.core.module {
                    self.core.send_command(AudioCommand::LoadModule(module.clone()));
                }
                // Apply persisted audio settings to the new engine
                self.apply_audio_settings_to_engine();
            }
            Err(e) => {
                eprintln!("[AUDIO] Failed to create audio stream: {}", e);
                self.audio_init_failed = true;
            }
        }
    }

    fn switch_output_device(&mut self, device_name: String) {
        self.stream = None;
        self.core.command_sender = None;
        self.selected_device_name = Some(device_name);
        self.init_audio();
    }


    pub(crate) fn sync_channel_fields(&mut self) {
        self.core.sync_channel_fields();
        let count = self.core.num_channels();
        self.multichannel_channels.resize(count, false);
        if self.pattern_view.channel_names.len() < count {
            let old = self.pattern_view.channel_names.len();
            self.pattern_view.channel_names.resize_with(count, || String::new());
            for i in old..count {
                self.pattern_view.channel_names[i] = format!("Ch{}", i + 1);
            }
        }
    }

    pub(crate) fn sync_send_bus_state(&mut self) {
        if let Some(ref module) = self.core.module {
            self.sendfx_panel.effect_types = module.send_bus_config;
            self.sendfx_panel.params = [
                [module.send_return_levels[0], 0.0, 0.0, 0.0, 0.0],
                [module.send_return_levels[1], 0.0, 0.0, 0.0, 0.0],
                [module.send_return_levels[2], 0.0, 0.0, 0.0, 0.0],
                [module.send_return_levels[3], 0.0, 0.0, 0.0, 0.0],
            ];
            self.sendfx_panel.pre_fader = module.send_pre_fader;
        }
    }

    /// Reload any instrument plugin slots from the current module.
    /// Called from `load_file` and `new_song` so that opening a saved
    /// `.htk` with instrument plugin assignments brings the plugins
    /// back to life automatically (with their saved state restored).
    /// Slots whose path no longer matches a discovered plugin are
    /// left in place but not loaded; the user can re-pick from the
    /// browser to fix the path.
    pub(crate) fn sync_instrument_plugin_state(&mut self) {
        // Snapshot the list of (index, slot) pairs up front so we can
        // mutate `self` inside the loop without aliasing.
        let slots: Vec<(usize, String, String, Vec<u8>)> = match self.core.module.as_ref() {
            Some(m) => m.instruments.iter().enumerate()
                .filter_map(|(idx, inst)| inst.plugin.as_ref().map(|slot| {
                    (idx, slot.format.clone(), slot.path.clone(), slot.state.clone())
                }))
                .collect(),
            None => return,
        };

        let discovered = self.discovered_plugins();

        // Borrow the disjoint fields the two closures need as
        // separate locals so the borrow checker doesn't see them as
        // overlapping &mut self borrows.
        let core_command_sender = &mut self.core.command_sender;
        let core_selected_instrument = self.core.selected_instrument;
        let instrument_plugin_handles = &mut self.instrument_plugin_handles;
        let instrument_editor_plugin_name = &mut self.instrument_editor.plugin_name;

        crate::ui::plugin_browser::sync_plugin_slots_from_module(
            slots,
            &discovered,
            "instrument",
            |inst_idx, descriptor, initial_state| {
                let sample_rate = if self.current_sample_rate > 0 {
                    self.current_sample_rate as f64
                } else {
                    48000.0
                };
                crate::ui::plugin_browser::load_and_install_instrument_plugin(
                    descriptor, inst_idx, initial_state, sample_rate, core_command_sender,
                )
            },
            |inst_idx, handle, name| {
                if inst_idx < instrument_plugin_handles.len() {
                    instrument_plugin_handles[inst_idx] = Some(handle);
                }
                if core_selected_instrument == inst_idx {
                    *instrument_editor_plugin_name = name;
                }
            },
        );
    }

    fn apply_audio_settings_to_engine(&mut self) {
        let interp = match self.config.default_interpolation.as_str() {
            "Nearest" => crate::audio::commands::InterpolationType::Nearest,
            "Cubic" => crate::audio::commands::InterpolationType::Cubic,
            _ => crate::audio::commands::InterpolationType::Linear,
        };
        self.core.send_command(crate::audio::commands::AudioCommand::SetInterpolation(interp));

        let limiter = match self.config.limiter_mode.as_str() {
            "SoftKnee" => crate::audio::commands::LimiterMode::SoftKnee,
            "SoftKneeSmooth" => crate::audio::commands::LimiterMode::SoftKneeSmooth,
            _ => crate::audio::commands::LimiterMode::HardClip,
        };
        self.core.send_command(crate::audio::commands::AudioCommand::SetLimiterMode(limiter));
    }

    fn apply_config_to_live_state(&mut self) {
        self.follow_playback = self.config.follow_playback_default;
        self.sample_editor.amplify_factor = self.config.default_amplify_factor;
        if let Some(preset) = ThemePreset::from_name(&self.config.theme_preset) {
            self.theme_preset = preset;
            self.theme = TrackerTheme::from_preset(preset);
        }
        self.file_browser.restore_last_dirs(&self.config);

        self.apply_audio_settings_to_engine();

        // Apply debug logging
        let config_dir = crate::app_config::AppConfig::config_dir();
        crate::debug_log::init(self.config.debug, config_dir);
    }



    pub(crate) fn new_song(&mut self) {
        self.core.new_song();
        self.pattern_view.scroll_row = 0;
        self.pattern_view.scroll_channel = 0;
        self.sync_channel_fields();
        self.sync_send_bus_state();
        // A new song has no plugin slots, but call these anyway for
        // symmetry with load_file.
        self.sync_send_bus_plugin_state();
        self.sync_instrument_plugin_state();
    }


    #[allow(dead_code)]
    fn current_pattern_mut(&mut self) -> Option<&mut crate::sequencer::Pattern> {
        self.core.current_pattern_mut()
    }



    pub(crate) fn change_selected_sample(&mut self, delta: i32) {
        let len = self.core.module.as_ref().map_or(0, |m| m.samples.len());
        if len <= 1 {
            return;
        }
        let next = (self.core.selected_sample as i32 + delta).clamp(1, (len - 1) as i32);
        self.core.selected_sample = next as usize;
        self.sample_editor.selection = None;
    }

    /// Load a WAV file from the sample library into the currently
    /// selected sample slot and map the current instrument to use it.
    pub(crate) fn import_sample_from_library(&mut self, path: &str) {
        use crate::formats::wav::import_wav;
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => { self.sample_library_state.status_message = Some(format!("Read error: {e}")); return; }
        };
        match import_wav(&data) {
            Ok(mut sample) => {
                if let Some(name) = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()) {
                    sample.name = name.to_string();
                }
                let sample_idx = self.core.selected_sample;
                self.core.import_wav_to_sample(sample_idx, sample);
                self.core.ensure_module_ownership();
                if let Some(ref mut module) = self.core.module {
                    if let Some(arc_module) = std::sync::Arc::get_mut(module) {
                        let inst_idx = self.core.selected_instrument;
                        if inst_idx > 0 && inst_idx < arc_module.instruments.len() {
                            for i in 0..120 { arc_module.instruments[inst_idx].sample_map[i] = sample_idx as u8; }
                        }
                    }
                }
                self.core.sync_module_to_audio();
                self.sample_library_state.status_message = Some("Imported".to_string());
            }
            Err(e) => { self.sample_library_state.status_message = Some(format!("Failed: {e}")); }
        }
    }

    pub(crate) fn change_selected_instrument(&mut self, delta: i32) {
        let len = self.core.module.as_ref().map_or(0, |m| m.instruments.len());
        if len <= 1 {
            return;
        }
        let next = (self.core.selected_instrument as i32 + delta).clamp(1, (len - 1) as i32);
        self.core.selected_instrument = next as usize;
    }


    pub(crate) fn set_cell_at_cursor(&mut self, new_cell: Cell) {
        self.core.set_cell_at_cursor(new_cell, &self.multichannel_channels, self.multichannel_enabled);
    }

    pub(crate) fn advance_cursor_down(&mut self, step: usize) {
        let max_row = self.core.current_pattern_or_default().num_rows.max(1);
        self.core.cursor.row = (self.core.cursor.row + step).min(max_row - 1);
        self.ensure_cursor_visible();
    }

    pub(crate) fn advance_cursor_up(&mut self, step: usize) {
        self.core.cursor.row = self.core.cursor.row.saturating_sub(step);
        self.ensure_cursor_visible();
    }

    pub(crate) fn ensure_cursor_visible(&mut self) {
        if self.core.cursor.row < self.pattern_view.scroll_row {
            self.pattern_view.scroll_row = self.core.cursor.row;
        }
        if self.core.cursor.row >= self.pattern_view.scroll_row + self.pattern_view.last_visible_rows {
            self.pattern_view.scroll_row = self.core.cursor.row - self.pattern_view.last_visible_rows + 1;
        }

        if self.core.cursor.channel < self.pattern_view.scroll_channel {
            self.pattern_view.scroll_channel = self.core.cursor.channel;
        }
        if self.core.cursor.channel >= self.pattern_view.scroll_channel + self.pattern_view.last_visible_channels {
            self.pattern_view.scroll_channel = self.core.cursor.channel - self.pattern_view.last_visible_channels + 1;
        }
    }


    pub(crate) fn move_cursor_right(&mut self) {
        let num_ch = self.core.num_channels();
        if self.core.cursor.channel < num_ch - 1 {
            self.core.cursor.channel += 1;
            self.core.cursor.sub_column = SubColumn::Note;
        }
        self.ensure_cursor_visible();
    }

    pub(crate) fn move_cursor_left(&mut self) {
        if self.core.cursor.channel > 0 {
            self.core.cursor.channel -= 1;
            self.core.cursor.sub_column = SubColumn::Note;
        }
        self.ensure_cursor_visible();
    }

    pub(crate) fn first_visible_sub_column(col_vis: crate::ui::pattern_grid::ColumnVisibility) -> crate::ui::pattern_grid::SubColumn {
        use crate::ui::pattern_grid::SubColumn;
        if col_vis.note { return SubColumn::Note; }
        if col_vis.instrument { return SubColumn::InstrumentTens; }
        if col_vis.volume { return SubColumn::VolumeTens; }
        if col_vis.effect { return SubColumn::EffectType; }
        SubColumn::Note
    }

    pub(crate) fn last_visible_sub_column(col_vis: crate::ui::pattern_grid::ColumnVisibility) -> crate::ui::pattern_grid::SubColumn {
        use crate::ui::pattern_grid::SubColumn;
        if col_vis.effect { return SubColumn::EffectParamLow; }
        if col_vis.volume { return SubColumn::VolumeOnes; }
        if col_vis.instrument { return SubColumn::InstrumentOnes; }
        if col_vis.note { return SubColumn::Note; }
        SubColumn::Note
    }

    pub(crate) fn step_sub_column_forward(&mut self) {
        let col_vis = self.config.get_col_vis();
        if let Some(next) = self.core.cursor.sub_column.next_visible(col_vis) {
            self.core.cursor.sub_column = next;
        } else {
            self.core.cursor.sub_column = Self::first_visible_sub_column(col_vis);
            self.advance_cursor_down(1);
        }
    }

    pub(crate) fn step_sub_column_backward(&mut self) {
        let col_vis = self.config.get_col_vis();
        if let Some(prev) = self.core.cursor.sub_column.prev_visible(col_vis) {
            self.core.cursor.sub_column = prev;
        } else {
            self.core.cursor.sub_column = Self::last_visible_sub_column(col_vis);
            self.advance_cursor_up(1);
        }
    }

    pub(crate) fn cycle_spacing_mode(&mut self) {
        use crate::app_config::SpacingMode;
        let modes = [SpacingMode::Compact, SpacingMode::Normal, SpacingMode::Wide, SpacingMode::ExtraWide];
        let current = self.config.get_spacing_mode();
        let idx = modes.iter().position(|&m| m == current).unwrap_or(1);
        let next = modes[(idx + 1) % modes.len()];
        self.config.set_spacing_mode(next);
    }

    pub(crate) fn extend_selection_down(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.advance_cursor_down(1);
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_up(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.advance_cursor_up(1);
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_right(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.move_cursor_right();
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_left(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.move_cursor_left();
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }


    pub(crate) fn mark_block_begin(&mut self) {
        let cur = self.core.cursor;
        self.core.selection_anchor = Some(cur);
        self.core.selection = Some(Selection { start: cur, end: cur });
    }

    pub(crate) fn mark_block_end(&mut self) {
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection { start: anchor, end: self.core.cursor });
        }
    }

    pub(crate) fn cut_selection(&mut self) {
        self.core.copy_selection();
        self.core.delete_selection();
    }


    pub(crate) fn handle_context_menu_action(&mut self, action: crate::ui::pattern_grid::ContextMenuAction) {
        if !self.edit_mode {
            return;
        }
        use crate::ui::pattern_grid::ContextMenuAction;
        match action {
            ContextMenuAction::CopySelection => self.core.copy_selection(),
            ContextMenuAction::PasteClipboard => self.core.paste_at_cursor(),
            ContextMenuAction::CutSelection => self.cut_selection(),
            ContextMenuAction::SelectAll => self.core.select_all(),
            ContextMenuAction::TransposeUp => self.core.transpose_selection(1),
            ContextMenuAction::TransposeDown => self.core.transpose_selection(-1),
            other => self.core.handle_context_menu_action(other),
        }
    }

    fn handle_automation_interaction(&mut self, interaction: crate::ui::pattern_grid::AutomationInteraction) {
        self.core.handle_automation_interaction(interaction);
    }

    #[allow(dead_code)] // exposed for future automation UI / MCP tools
    pub(crate) fn enter_automation_hex(&mut self, channel: usize, row: usize, digit: u8) {
        self.core.enter_automation_hex(channel, row, digit);
    }

    pub(crate) fn delete_automation_point(&mut self, channel: usize, row: usize) {
        self.core.delete_automation_point(channel, row);
    }






    pub(crate) fn open_file_dialog(&mut self) {
        self.file_browser.open(BrowserMode::Modules, crate::ui::file_browser::DialogMode::Open, &mut self.config);
    }





    pub(crate) fn save_as_dialog(&mut self) {
        self.browser_purpose = BrowserPurpose::SaveProject;
        self.file_browser.open(BrowserMode::Projects, crate::ui::file_browser::DialogMode::Save, &mut self.config);
    }

    pub(crate) fn copy_track(&mut self) {
        self.core.copy_channel(self.core.cursor.channel);
    }

    pub(crate) fn cut_track(&mut self) {
        let ch = self.core.cursor.channel;
        self.core.copy_channel(ch);
        self.core.clear_channel(ch);
    }

    pub(crate) fn delete_track(&mut self) {
        self.core.clear_channel(self.core.cursor.channel);
    }

    pub(crate) fn copy_column(&mut self) {
        let ch = self.core.cursor.channel;
        let sc = self.core.cursor.sub_column;
        self.core.copy_column(ch, sc);
    }

    pub(crate) fn cut_column(&mut self) {
        let ch = self.core.cursor.channel;
        let sc = self.core.cursor.sub_column;
        self.core.copy_column(ch, sc);
        self.core.clear_channel(ch);
    }

    pub(crate) fn preview_browser_sample(&mut self, note_key: u8) -> bool {
        use crate::ui::file_browser::BrowserMode;
        if !self.file_browser.show || self.file_browser.mode != BrowserMode::Samples {
            return false;
        }
        let entry = match self.file_browser.selected_entry() {
            Some(e) => e.clone(),
            None => return false,
        };
        if entry.is_dir {
            return false;
        }
        let audio_exts = ["wav", "mp3", "ogg", "flac", "it", "xm", "s3m", "mod", "669"];
        if !audio_exts.contains(&entry.extension.as_str()) {
            return false;
        }
        let (data, sample_rate) = match self.file_browser.get_preview_data(&entry.path) {
            Some(d) => d,
            None => return false,
        };
        self.core.send_command(crate::audio::commands::AudioCommand::PreviewBuffer {
            data,
            sample_rate,
            note_key,
            volume: 0.75,
            panning: 0.5,
        });
        true
    }

    fn draw_preamble(&mut self, ctx: &egui::Context) -> (Option<usize>, Option<usize>, Option<usize>, Option<u8>, u8) {
        ctx.set_zoom_factor(self.config.zoom_factor);
        ctx.set_visuals(self.theme.to_visuals());

        if self.stream.is_none() && !self.audio_init_failed {
            self.init_audio();
        }

        // Process MCP mutation requests on the main thread.
        if let Some(ref mut server) = self.mcp_server {
            while let Ok(cmd) = server.command_rx.try_recv() {
                let result = crate::mcp::mutations::execute_mutation(&mut self.core, &cmd.method, &cmd.params);
                let _ = cmd.response_tx.send(result);
            }
        }

        crate::actions::handle_keyboard_input(self, ctx);

        if self.current_view == AppView::Pattern && ctx.memory(|m| m.focused().is_none()) {
            ctx.input_mut(|i| {
                i.events.retain(|e| !matches!(e,
                    egui::Event::Key { key: egui::Key::Tab, pressed: true, .. }
                    | egui::Event::Key { key: egui::Key::ArrowUp, pressed: true, .. }
                    | egui::Event::Key { key: egui::Key::ArrowDown, pressed: true, .. }
                    | egui::Event::Key { key: egui::Key::ArrowLeft, pressed: true, .. }
                    | egui::Event::Key { key: egui::Key::ArrowRight, pressed: true, .. }
                ));
            });
        }

        let nch = self.core.num_channels();
        self.core.cursor.channel = self.core.cursor.channel.min(nch.saturating_sub(1));
        self.pattern_view.scroll_channel = self.pattern_view.scroll_channel.min(nch.saturating_sub(1));

        crate::actions::update_wav_export_progress(self);

        let pending = self.pending_view_switch.swap(0, Ordering::Relaxed);
        if pending > 0 {
            self.current_view = match pending {
                1 => AppView::Pattern,
                2 => AppView::Sample,
                3 => AppView::Instrument,
                4 => AppView::SendFx,
                5 => AppView::Playback,
                6 => AppView::Automation,
                7 => AppView::Mixer,
                _ => self.current_view,
            };
        }

        let (playback_row, playback_order, playback_pattern, playback_tick, playback_speed) = {
            let playing = self.core.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed);
            if playing {
                let order = self.core.playback_state.current_order.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let row = self.core.playback_state.current_row.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let pat = self.core.playback_state.current_pattern.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let tick = self.core.playback_state.current_tick.load(std::sync::atomic::Ordering::Relaxed);
                let speed = self.core.playback_state.speed.load(std::sync::atomic::Ordering::Relaxed);
                (Some(row), Some(order), Some(pat), Some(tick), speed)
            } else {
                (None, None, None, None, 6)
            }
        };

        if let Some(order) = playback_order {
            if self.follow_playback && order != self.core.selected_order {
                if let Some(ref module) = self.core.module {
                    if order < module.order_list.len() {
                        self.core.selected_order = order;
                        self.core.cursor.row = 0;
                        self.ensure_cursor_visible();
                    }
                }
            }
        }

        if let Some(row) = playback_row {
            if self.follow_playback {
                if let (Some(active_pat), Some(ref module)) = (playback_pattern, self.core.module.as_ref()) {
                    if !module.order_list.is_empty() {
                        let order_idx = self.core.selected_order.min(module.order_list.len().saturating_sub(1));
                        let displayed_pat = module.order_list[order_idx] as usize;
                        if displayed_pat == active_pat {
                            if row < self.pattern_view.scroll_row {
                                self.pattern_view.scroll_row = row;
                            }
                            if row >= self.pattern_view.scroll_row + self.pattern_view.last_visible_rows {
                                self.pattern_view.scroll_row = row - self.pattern_view.last_visible_rows + 1;
                            }
                        }
                    }
                }
            }
        }

        // Update MCP module snapshot for the MCP thread.
        if let Some(ref mut server) = self.mcp_server {
            if let Some(ref module) = self.core.module {
                let patterns_json: Vec<(usize, serde_json::Value)> = module.patterns.iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let rows: Vec<Vec<crate::sequencer::pattern::Cell>> = p.data.iter()
                            .map(|row| row[..module.channel_panning.len()].to_vec())
                            .collect();
                        (i, serde_json::json!({"num_rows": p.num_rows, "data": rows}))
                    })
                    .collect();
                let instruments_json: Vec<(usize, serde_json::Value)> = module.instruments.iter()
                    .enumerate()
                    .map(|(i, inst)| (i, serde_json::to_value(inst).unwrap_or_default()))
                    .collect();
                let samples_json: Vec<(usize, serde_json::Value)> = module.samples.iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let info = serde_json::json!({
                            "name": s.name,
                            "sample_rate": s.sample_rate,
                            "bits_per_sample": s.bits_per_sample,
                            "length": s.data.len(),
                            "loop_type": format!("{:?}", s.loop_type),
                            "loop_start": s.loop_start,
                            "loop_end": s.loop_end,
                            "default_volume": s.default_volume,
                            "default_panning": s.default_panning,
                            "global_volume": s.global_volume,
                            "relative_note": s.relative_note,
                            "fine_tune": s.fine_tune,
                        });
                        (i, info)
                    })
                    .collect();

                let module_json = serde_json::to_value(&**module).unwrap_or_default();
                let pb = &self.core.playback_state;
                let playing = pb.playing.load(std::sync::atomic::Ordering::Relaxed);
                let pb_snapshot = crate::mcp::protocol::PlaybackSnapshot {
                    playing,
                    current_order: pb.current_order.load(std::sync::atomic::Ordering::Relaxed),
                    current_row: pb.current_row.load(std::sync::atomic::Ordering::Relaxed),
                    current_pattern: pb.current_pattern.load(std::sync::atomic::Ordering::Relaxed),
                    current_tick: pb.current_tick.load(std::sync::atomic::Ordering::Relaxed),
                    bpm: pb.bpm.load(std::sync::atomic::Ordering::Relaxed),
                    speed: pb.speed.load(std::sync::atomic::Ordering::Relaxed),
                    active_voices: pb.active_voices.load(std::sync::atomic::Ordering::Relaxed),
                    cpu_usage_pct: pb.cpu_usage_pct.load(std::sync::atomic::Ordering::Relaxed),
                };
                let ch_snapshot = crate::mcp::protocol::ChannelsSnapshot {
                    panning: module.channel_panning.clone(),
                    volume: module.channel_volume.clone(),
                    muted: self.core.muted_channels.clone(),
                    solo: self.core.solo_channels.clone(),
                };

                let snapshot = crate::mcp::protocol::ModuleSnapshot {
                    module_json: Some(module_json),
                    patterns_json,
                    instruments_json,
                    samples_json,
                };
                if let Ok(mut lock) = server.snapshot.write() {
                    *lock = snapshot;
                }
                if let Ok(mut lock) = server.playback_snapshot.write() {
                    *lock = pb_snapshot;
                }
                if let Ok(mut lock) = server.channels_snapshot.write() {
                    *lock = ch_snapshot;
                }
            }
        }

        (playback_row, playback_order, playback_pattern, playback_tick, playback_speed)
    }

    fn draw_dialogs(&mut self, ctx: &egui::Context) {
        if self.show_shortcuts {
            crate::ui::help_screen::draw_shortcuts_window(ctx, &mut self.show_shortcuts, &self.theme);
        }

        if self.settings_state.open {
            let action = crate::ui::settings_window::draw_settings_window(
                ctx,
                &mut self.settings_state,
                &self.output_device_names,
                self.selected_device_name.as_deref(),
                &self.theme,
            );
            match action {
                crate::ui::settings_window::SettingsAction::Save | crate::ui::settings_window::SettingsAction::Apply => {
                    self.settings_state.apply_to_config(&mut self.config);
                    self.config.save();
                    self.apply_config_to_live_state();
                    self.pending_reinit = true;
                }
                crate::ui::settings_window::SettingsAction::Cancel => {
                    self.settings_state = crate::ui::settings_window::SettingsState::from_config(&self.config);
                }
                crate::ui::settings_window::SettingsAction::RefreshDevices => {
                    self.refresh_output_devices();
                }
                crate::ui::settings_window::SettingsAction::SelectDevice(name) => {
                    self.pending_device_switch = Some(name);
                }
                crate::ui::settings_window::SettingsAction::None => {}
            }
        }

        if crate::ui::wav_export_window::draw_wav_export(ctx, &mut self.wav_export_state) {
            crate::actions::export_wav_with_settings(self);
        }

        if self.show_about {
            egui::Window::new("About Holofonic Tracker")
                .open(&mut self.show_about)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .default_width(350.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading(concat!("Holofonic Tracker v", env!("CARGO_PKG_VERSION")));
                        ui.add_space(8.0);
                        ui.label("A modern tracker / music sequencer");
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Built with Rust + egui + cpal")
                                .size(11.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(12.0);
                        ui.label("Supports .htk, .it, .xm, .s3m, .mod formats");
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("Development guided by Clint Anderson")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(180, 180, 200)),
                        );
                        ui.label(
                            egui::RichText::new("clinta@gmail.com")
                                .size(10.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("AI-assisted code from GLM, DeepSeek, and MiniMax")
                                .size(10.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(8.0);
                    });
                });
        }

        if self.show_exit_confirm {
            let mut open = true;
            egui::Window::new("Unsaved Changes")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .default_width(400.0)
                .show(ctx, |ui| {
                    ui.label("The current project has unsaved changes.");
                    ui.label("Do you want to save before exiting?");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            crate::actions::save_current_file(self);
                            self.show_exit_confirm = false;
                            if !self.core.module_dirty() {
                                self.exit_confirmed = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            } else {
                                self.close_after_save = true;
                            }
                        }
                        if ui.button("Don't Save").clicked() {
                            self.show_exit_confirm = false;
                            self.exit_confirmed = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_exit_confirm = false;
                        }
                    });
                });
            if !open {
                self.show_exit_confirm = false;
            }
        }

        if self.show_phrase_generator {
            let num_ch = self.core.num_channels_checked();
            let num_rows = self.core.current_pattern().map_or(64, |p| p.num_rows);
            let cursor_ch = self.core.cursor.channel;
            if let Some(params) = crate::ui::phrase_generator_dialog::draw_phrase_generator(
                ctx,
                &mut self.show_phrase_generator,
                &self.theme,
                num_ch,
                num_rows,
                cursor_ch,
            ) {
                let (start_row, end_row) = match &self.core.selection {
                    Some(sel) => {
                        let (min, max) = sel.normalized();
                        (min.row, max.row)
                    }
                    None => (0, num_rows.saturating_sub(1)),
                };
                let notes = crate::tools::phrase_generator::generate_phrase(
                    &params, start_row, end_row, num_ch,
                );
                if !notes.is_empty() {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut m) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(m) {
                            let pat_idx = *arc_module.order_list.get(self.core.selected_order).unwrap_or(&0) as usize;
                            if pat_idx < arc_module.patterns.len() {
                                let pattern = &arc_module.patterns[pat_idx];
                                let mut old_cells = Vec::new();
                                let mut new_cells = Vec::new();
                                for &(row, ch, cell) in &notes {
                                    if row < pattern.num_rows && ch < crate::sequencer::pattern::MAX_CHANNELS {
                                        old_cells.push((row, ch, pattern.data[row][ch]));
                                        new_cells.push((row, ch, cell));
                                    }
                                }
                                let cmd = Box::new(crate::edit::BulkSetCellsCommand {
                                    order: self.core.selected_order,
                                    old_cells,
                                    new_cells,
                                });
                                let _ = self.core.undo_manager.execute(cmd, arc_module);
                            }
                        }
                    }
                    self.core.sync_module_to_audio();
                }
            }
        }

        if let Some(ref mut dialog) = self.sample_export_dialog {
            if let Some((path, bit_depth)) = dialog.show(ctx) {
                let sample_idx = dialog.sample_index;
                if let Some(ref module) = self.core.module {
                    if let Some(sample) = module.samples.get(sample_idx) {
                        let wav_data = crate::formats::wav::export_wav(sample, bit_depth);
                        if let Err(e) = std::fs::write(&path, wav_data) {
                            eprintln!("Failed to write sample: {}", e);
                        } else {
                            if let Some(parent) = path.parent() {
                                self.config.default_wav_path = Some(parent.to_string_lossy().into_owned());
                            }
                            self.config.set_sample_export_bit_depth(bit_depth);
                        }
                    }
                }
                self.sample_export_dialog = None;
            }
        }

        if self.slice_dialog_open {
            let mut open = true;
            let source_sample = self.slice_config.source_sample;
            let has_module = self.core.module.is_some();

            egui::Window::new("Slice to Instrument")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .default_size([320.0, 380.0])
                .show(ctx, |ui| {
                    use crate::actions::slice_to_instrument::SliceMode;

                    let config = &mut self.slice_config;
                    let theme = &self.theme;

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Source:").color(theme.fg_dim));
                        if let Some(ref module) = self.core.module {
                            ui.label(
                                module.samples.get(config.source_sample)
                                    .map(|s| if s.name.is_empty() { format!("Sample {:02X}", config.source_sample) } else { s.name.clone() })
                                    .unwrap_or_default()
                            );
                        }
                    });

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Mode").size(13.0).strong().color(egui::Color32::from_rgb(100, 200, 255)));

                    let modes = ["Time Divisions", "Onset Detection"];
                    let mut mode_idx = if config.mode == SliceMode::TimeDivisions { 0 } else { 1 };
                    egui::ComboBox::from_id_salt("slice_mode")
                        .selected_text(modes[mode_idx])
                        .show_ui(ui, |ui| {
                            for (i, name) in modes.iter().enumerate() {
                                if ui.selectable_label(mode_idx == i, *name).clicked() {
                                    mode_idx = i;
                                    config.mode = if i == 0 { SliceMode::TimeDivisions } else { SliceMode::Onsets };
                                }
                            }
                        });

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Parameters").size(13.0).strong().color(egui::Color32::from_rgb(100, 200, 255)));
                    ui.add_space(2.0);

                    match config.mode {
                        SliceMode::TimeDivisions => {
                            egui::Grid::new("slice_time_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                                ui.label(egui::RichText::new("BPM:").color(theme.fg_dim));
                                ui.add(egui::Slider::new(&mut config.bpm, 30.0..=300.0));
                                ui.end_row();

                                ui.label(egui::RichText::new("Division:").color(theme.fg_dim));
                                let divisions = [4u8, 8, 16, 32];
                                let div_names = ["1/4", "1/8", "1/16", "1/32"];
                                let mut div_idx = divisions.iter().position(|d| *d == config.division).unwrap_or(2);
                                egui::ComboBox::from_id_salt("slice_division")
                                    .selected_text(div_names[div_idx])
                                    .show_ui(ui, |ui| {
                                        for (i, name) in div_names.iter().enumerate() {
                                            if ui.selectable_label(div_idx == i, *name).clicked() {
                                                div_idx = i;
                                                config.division = divisions[i];
                                            }
                                        }
                                    });
                                ui.end_row();
                            });
                        }
                        SliceMode::Onsets => {
                            egui::Grid::new("slice_onset_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                                ui.label(egui::RichText::new("Sensitivity:").color(theme.fg_dim));
                                ui.add(egui::Slider::new(&mut config.sensitivity, 0.05..=1.0));
                                ui.end_row();

                                ui.label(egui::RichText::new("Min Spacing:").color(theme.fg_dim));
                                ui.add(egui::Slider::new(&mut config.min_spacing_ms, 5.0..=500.0).suffix("ms"));
                                ui.end_row();
                            });
                        }
                    }

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Instrument").size(13.0).strong().color(egui::Color32::from_rgb(100, 200, 255)));
                    ui.add_space(2.0);
                    egui::Grid::new("slice_inst_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label(egui::RichText::new("Base Note:").color(theme.fg_dim));
                        let note_names = ["C-","C#","D-","D#","E-","F-","F#","G-","G#","A-","A#","B-"];
                        let oct = config.base_note / 12;
                        let note = config.base_note % 12;
                        let note_str = format!("{}{}", note_names[note as usize], oct);
                        egui::ComboBox::from_id_salt("slice_base_note")
                            .selected_text(&note_str)
                            .show_ui(ui, |ui| {
                                    for o in 0..10u8 {
                                            for n in 0..12u8 {
                                                let midi = o * 12 + n;
                                                let name = format!("{}{}", note_names[n as usize], o);
                                                let selected = config.base_note == midi;
                                                if ui.selectable_label(selected, &name).clicked() {
                                                    config.base_note = midi;
                                                }
                                            }
                                        }
                            });
                        ui.end_row();

                        ui.label(egui::RichText::new("Target:").color(theme.fg_dim));
                        if let Some(ref module) = self.core.module {
                            let mut inst_names: Vec<String> = module.instruments.iter().enumerate()
                                .map(|(i, inst)| {
                                    if inst.name.is_empty() { format!("Inst {:02X}", i) } else { format!("{:02X}:{}", i, inst.name) }
                                }).collect();
                            inst_names[0] = "---".to_string();
                            inst_names.push("+ Create New".to_string());

                            let current = config.target_instrument.map_or(inst_names.len() - 1, |idx| {
                                if idx < module.instruments.len() { idx } else { inst_names.len() - 1 }
                            });

                            let mut sel = current;
                            egui::ComboBox::from_id_salt("slice_target_inst")
                                .selected_text(&inst_names[sel])
                                .show_ui(ui, |ui| {
                                    for (i, name) in inst_names.iter().enumerate() {
                                        if ui.selectable_label(sel == i, name.as_str()).clicked() {
                                            sel = i;
                                            if i < module.instruments.len() {
                                                config.target_instrument = Some(i);
                                            } else {
                                                config.target_instrument = None;
                                            }
                                        }
                                    }
                                });
                        }
                        ui.end_row();
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() && has_module {
                            self.core.ensure_module_ownership();
                            if let Some(ref mut module) = self.core.module {
                                if let Some(arc_module) = Arc::get_mut(module) {
                                    // Capture pre-state
                                    let target_inst = config.target_instrument.unwrap_or_else(|| {
                                        let mut free = 1usize;
                                        while free < arc_module.instruments.len()
                                            && !arc_module.instruments[free].name.is_empty()
                                            && arc_module.instruments[free].sample_map.iter().any(|&s| s != 0)
                                        {
                                            free += 1;
                                        }
                                        if free >= arc_module.instruments.len() && arc_module.instruments.len() < crate::sequencer::module::MAX_INSTRUMENTS {
                                            arc_module.instruments.push(crate::sequencer::Instrument::default());
                                        }
                                        free.min(arc_module.instruments.len().saturating_sub(1))
                                    });

                                    let pre_sample_count = arc_module.samples.len();
                                    let pre_instrument_name = arc_module.instruments[target_inst].name.clone();
                                    let pre_sample_map = arc_module.instruments[target_inst].sample_map;

                                    let mut slice_config = config.clone();
                                    slice_config.target_instrument = Some(target_inst);

                                    match crate::actions::slice_to_instrument::compute_slices(arc_module, &slice_config) {
                                        Ok((slice_samples, result)) => {
                                            let cmd = Box::new(crate::edit::SliceToInstrumentCommand {
                                                target_instrument: result.target_instrument,
                                                pre_sample_count,
                                                pre_instrument_name,
                                                pre_sample_map,
                                                slice_samples,
                                                post_name: format!("Sliced: {}", {
                                                    arc_module.samples.get(source_sample)
                                                        .map(|s| s.name.as_str())
                                                        .unwrap_or("")
                                                }),
                                                post_base_note: config.base_note,
                                                post_slice_count: result.slice_count as u8,
                                            });
                                            let _ = self.core.undo_manager.execute(cmd, arc_module);
                                            self.core.sync_module_to_audio();
                                            self.slice_dialog_open = false;
                                        }
                                        Err(e) => {
                                            eprintln!("Slice error: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.slice_dialog_open = false;
                        }
                    });
                });
            if !open {
                self.slice_dialog_open = false;
            }
        }

        if self.file_browser.show {
            let mut file_browser_open = true;

            egui::Window::new("File Browser")
                .open(&mut file_browser_open)
                .default_size([550.0, 400.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(true)
                .show(ctx, |ui| {
                    if let Some(path) = self.file_browser.render(ui, Some(&mut self.config), self.theme.clone()) {
                        let path_str = path.to_string_lossy().to_string();
                        match self.browser_purpose {
                            BrowserPurpose::General => {
                                let ext = path.extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                if ext == "wav" {
                                    crate::actions::import_wav(self, &path_str);
                                } else {
                                    crate::actions::load_file(self, &path_str);
                                }
                            }
                            BrowserPurpose::LoadInstrument => {
                                crate::actions::load_instrument_from_file(self, &path_str);
                            }
                            BrowserPurpose::SaveInstrument => {
                                let inst_idx = self.core.selected_instrument;
                                crate::actions::save_instrument_to_file(self, inst_idx, &path_str);
                            }
                            BrowserPurpose::ExportInstrument(idx) => {
                                crate::actions::save_instrument_to_file(self, idx, &path_str);
                            }
                            BrowserPurpose::SaveProject => {
                                self.core.save_file(&path_str);
                            }
                        }
                        self.browser_purpose = BrowserPurpose::General;
                    }
                });
            if self.file_browser.preview_requested {
                self.file_browser.preview_requested = false;
                self.preview_browser_sample(60);
            }
            if !file_browser_open {
                self.file_browser.close();
            }
        }

        // ── Sample Library dialog ──
        if self.sample_library_state.open {
            if let Some(module) = self.core.module.as_ref() {
                let import_path = crate::ui::sample_library::draw_sample_library(
                    ctx,
                    &mut self.sample_library_state,
                    &self.sample_library,
                    module,
                    &self.theme,
                );
                if let Some(path) = import_path {
                    self.import_sample_from_library(&path);
                }
            }
        }

        if self.close_after_save && !self.core.module_dirty() {
            self.close_after_save = false;
            self.exit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        crate::actions::check_auto_backup(self);

        ctx.request_repaint();
    }

    fn handle_sample_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(module) = &self.core.module {
            if let Some(event) = self.sample_editor.ui(
                ui,
                module,
                &mut self.core.selected_sample,
                &self.theme,
                &self.core.playback_state,
            ) {
                if matches!(event, crate::ui::sample_editor::SampleEditEvent::OpenSampleLibrary) {
                    self.sample_library_state.open = true;
                } else if let Some(sel_update) = crate::actions::handle_sample_edit(self, event) {
                    match sel_update {
                        crate::actions::SelectionUpdate::Clear => {
                            self.sample_editor.selection = None;
                        }
                        crate::actions::SelectionUpdate::Set(start, end) => {
                            self.sample_editor.selection = Some((start, end));
                        }
                    }
                }
            }
        }
    }

    fn handle_mixer_tab(&mut self, ui: &mut egui::Ui) {
        let mut plugin_slots: [Option<crate::sequencer::plugin::PluginSlot>; 4] = Default::default();
        let mut return_levels = [0.0f32; 4];
        if let Some(ref module) = self.core.module {
            for bus in 0..4 {
                if let Some(slot) = module.send_bus_plugins[bus].clone() {
                    plugin_slots[bus] = Some(slot);
                }
                return_levels[bus] = module
                    .send_return_levels
                    .get(bus)
                    .copied()
                    .unwrap_or(1.0);
            }
        }
        self.mixer_state.ui(
            ui,
            &mut self.core,
            &self.theme,
            &plugin_slots,
            &return_levels,
        );
    }

    fn handle_playback_tab(
        &mut self,
        ui: &mut egui::Ui,
        playback_pattern: Option<usize>,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
    ) {
        let num_channels = self.core.num_channels();
        let current_pattern = playback_pattern
            .and_then(|pat| self.core.module.as_ref()?.patterns.get(pat))
            .or_else(|| {
                let pat_idx = self.core.module.as_ref()
                    .and_then(|m| m.order_list.get(self.core.selected_order))
                    .copied().unwrap_or(0) as usize;
                self.core.module.as_ref()?.patterns.get(pat_idx)
            });
        let current_module = self.core.module.as_ref().map(|m| &**m);
        let grid_playback_row = playback_row;

        self.playback_view.ui(
            ui,
            &self.core.playback_state,
            &mut self.core.command_sender,
            &self.theme,
            num_channels,
            current_pattern,
            current_module,
            self.config.row_highlight_minor,
            self.config.row_highlight_major,
            self.config.get_sample_length_bg(),
            self.config.get_col_vis(),
            grid_playback_row,
            if grid_playback_row.is_some() { playback_tick } else { None },
            playback_speed,
            self.config.get_spacing_mode(),
        );
    }

    fn handle_instrument_tab(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        #[cfg(windows)]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> = {
            crate::audio::plugins::plugin_window::get_eframe_hwnd(frame)
                .map(|h| h as usize)
        };
        #[cfg(not(windows))]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> = None;

        // X-close poll and embedded editor panel rendering now happen
        // in HtrkApp::tick_plugin_editors (called from the frame-level
        // ui() pass) so editors stay alive in any tab.

        if let Some(module) = &self.core.module {
            // Cache the plugin editor state for the currently selected
            // instrument so the editor UI can read it without needing
            // a borrow of HtrkApp.
            let inst_idx = self.core.selected_instrument;
            self.instrument_editor.plugin_has_editor = self
                .instrument_plugin_handles
                .get(inst_idx)
                .and_then(|h| h.as_ref())
                .map(|h| h.has_editor())
                .unwrap_or(false);
            self.instrument_editor.plugin_editor_is_open = self
                .instrument_plugin_handles
                .get(inst_idx)
                .and_then(|h| h.as_ref())
                .map(|h| h.is_editor_open())
                .unwrap_or(false);
            self.instrument_editor.plugin_editor_mode = self
                .instrument_plugin_handles
                .get(inst_idx)
                .and_then(|h| h.as_ref())
                .and_then(|h| h.editor_mode());
            self.instrument_editor.plugin_editor_error = self
                .instrument_plugin_handles
                .get(inst_idx)
                .and_then(|h| h.as_ref())
                .and_then(|h| h.last_editor_error());

            if let Some(event) = self.instrument_editor.ui(
                ui,
                module,
                &mut self.core.selected_instrument,
                &mut self.core.selected_sample,
                &self.theme,
                &self.core.playback_state,
                &mut self.config,
                eframe_hwnd,
            ) {
                match event {
                    crate::ui::instrument_editor::InstrumentEditEvent::SaveInstrument => {
                        self.browser_purpose = BrowserPurpose::SaveInstrument;
                        let inst_idx = self.core.selected_instrument;
                        if let Some(ref m) = self.core.module {
                            if let Some(inst) = m.instruments.get(inst_idx) {
                                self.file_browser.file_name = format!("{}.hti", inst.name.trim());
                            }
                        }
                        self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Save, &mut self.config);
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::LoadInstrument => {
                        self.browser_purpose = BrowserPurpose::LoadInstrument;
                        self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut self.config);
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::ExportInstrument(idx) => {
                        self.browser_purpose = BrowserPurpose::ExportInstrument(idx);
                        if let Some(ref m) = self.core.module {
                            if let Some(inst) = m.instruments.get(idx) {
                                self.file_browser.file_name = format!("{}.hti", inst.name.trim());
                            }
                        }
                        self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Save, &mut self.config);
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::ImportInstrument => {
                        self.browser_purpose = BrowserPurpose::LoadInstrument;
                        self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut self.config);
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::PluginUnload => {
                        // Unload the running instance but KEEP the PluginSlot
                        // (path/id/state) so the user can re-load the same
                        // plugin later with their saved state preserved.
                        let instrument_idx = self.core.selected_instrument;
                        self.unload_instrument_plugin(instrument_idx);
                        self.instrument_editor.plugin_name.clear();
                        self.core.sync_module_to_audio();
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::OpenPluginEditor { floating } => {
                        let inst_idx = self.core.selected_instrument;
                        let mode = if floating {
                            crate::audio::plugins::EditorMode::Floating
                        } else {
                            crate::audio::plugins::EditorMode::Embedded
                        };
                        let parent: Option<*mut std::ffi::c_void> = if !floating {
                            eframe_hwnd.map(|h| h as *mut _)
                        } else {
                            None
                        };
                        if let Some(handle) = self.instrument_plugin_handles.get_mut(inst_idx) {
                            if let Some(ref mut h) = handle {
                                if let Err(e) = h.open_editor(mode, parent) {
                                    eprintln!("[plugin] instrument plugin open editor failed: {e}");
                                }
                            }
                        }
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::ClosePluginEditor => {
                        let inst_idx = self.core.selected_instrument;
                        if let Some(handle) = self.instrument_plugin_handles.get_mut(inst_idx) {
                            if let Some(ref mut h) = handle {
                                h.close_editor();
                            }
                        }
                    }
                    other => crate::actions::handle_instrument_edit(self, other),
                }
            }
        }

        // Plugin browser dialog for instrument plugins
        if self.instrument_editor.plugin_browser_open {
            let instrument_idx = self.core.selected_instrument;
            let discovered = self.discovered_plugins();
            let mut open = true;
            let (result, action) = crate::ui::plugin_browser::draw_plugin_browser(
                &ui.ctx(),
                &mut open,
                instrument_idx,
                &format!("Instr {:02X}", instrument_idx),
                &self.theme,
                &discovered,
                &self.plugin_browser_status,
            );

            if action.rescan_requested {
                let summary = self.rescan_plugins();
                self.plugin_browser_status =
                    crate::ui::plugin_browser::PluginBrowserStatus::Error(summary);
            }

            match result {
                crate::ui::plugin_browser::PluginSelectResult::Selected {
                    descriptor,
                    send_index,
                } => {
                    self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Loading(descriptor.name.clone());
                    // If the user picks the same plugin that was previously
                    // loaded on this instrument, restore its saved state.
                    // A different plugin path starts fresh (state cleared).
                    let descriptor_path = descriptor.path.to_string_lossy().to_string();
                    let initial_state: Option<Vec<u8>> = self.core.module.as_ref()
                        .and_then(|m| m.instruments.get(send_index))
                        .and_then(|i| i.plugin.as_ref())
                        .filter(|slot| slot.path == descriptor_path && !slot.state.is_empty())
                        .map(|slot| slot.state.clone());
                    match self.load_and_install_instrument_plugin(
                        &descriptor,
                        send_index,
                        initial_state.as_deref(),
                    ) {
                        Ok((handle, name)) => {
                            self.instrument_plugin_handles[send_index] = Some(handle);
                            self.instrument_editor.plugin_name = name.clone();
                            self.instrument_editor.plugin_browser_open = false;

                            // Capture the freshly-loaded plugin's state so
                            // the very first project save persists the
                            // plugin's default patch.
                            self.save_all_instrument_plugin_states();

                            self.core.ensure_module_ownership();
                            if let Some(ref mut module) = self.core.module {
                                if let Some(arc_module) = Arc::get_mut(module) {
                                    if send_index < arc_module.instruments.len() {
                                        arc_module.instruments[send_index].plugin = Some(
                                            crate::sequencer::plugin::PluginSlot::new(
                                                "clap",
                                                descriptor.path.to_string_lossy().to_string(),
                                                descriptor.plugin_id,
                                            )
                                        );
                                    }
                                }
                            }

                            self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Loaded(name);
                        }
                        Err(e) => {
                            eprintln!("[plugin] instrument plugin load failed: {e}");
                            self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Error(e);
                        }
                    }
                }
                crate::ui::plugin_browser::PluginSelectResult::Cancelled => {
                    // No selection this frame — keep the dialog open.
                    // Only close when `open` becomes false (X button / Close).
                }
            }

            if !open {
                self.instrument_editor.plugin_browser_open = false;
            }
        }
    }

    fn handle_automation_tab(&mut self, ui: &mut egui::Ui) {
        use crate::audio::plugins::ParamInfo;
        self.automation_editor.state.selected_order = self.core.selected_order as u16;
        self.core.ensure_module_ownership();
        if let Some(ref mut module) = self.core.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                // Capture the param_info lookups as closures so the
                // automation editor can enumerate plugin params without
                // needing a borrow of HtrkApp. The closures borrow
                // disjoint fields of `self` so the borrow checker
                // allows them alongside the `Arc::get_mut` borrow.
                let send_bus_handles_ref = &self.send_bus_handles;
                let instrument_plugin_handles_ref = &self.instrument_plugin_handles;
                let get_send_bus_params = |si: usize| -> Vec<ParamInfo> {
                    send_bus_handles_ref
                        .get(si)
                        .and_then(|h| h.as_ref())
                        .map(|h| h.parameter_info())
                        .unwrap_or_default()
                };
                let get_instrument_params = |inst: u8| -> Vec<ParamInfo> {
                    instrument_plugin_handles_ref
                        .get(inst as usize)
                        .and_then(|h| h.as_ref())
                        .map(|h| h.parameter_info())
                        .unwrap_or_default()
                };
                let auto_resp = self.automation_editor.ui(
                    ui,
                    arc_module,
                    &self.theme,
                    get_send_bus_params,
                    get_instrument_params,
                );
                if let Some((target, channel)) = auto_resp.track_added {
                    let id = arc_module.next_automation_id;
                    arc_module.next_automation_id += 1;
                    arc_module.automation_tracks.push(
                        crate::sequencer::AutomationTrack::new(id, target, channel)
                    );
                    self.automation_editor.state.selected_track_id = Some(id);
                }
                if let Some(tid) = auto_resp.track_removed {
                    arc_module.automation_tracks.retain(|t| t.id != tid);
                    if self.automation_editor.state.selected_track_id == Some(tid) {
                        self.automation_editor.state.selected_track_id = None;
                    }
                }
                if let Some(tid) = auto_resp.track_toggled {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == tid) {
                        t.enabled = !t.enabled;
                    }
                }
                if let Some((track_id, point)) = auto_resp.point_changed {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                        t.insert_point(point);
                    }
                }
                if let Some((track_id, order, row)) = auto_resp.point_removed {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                        t.remove_point_at(order, row);
                    }
                }
                if let Some((track_id, mode)) = auto_resp.interp_changed {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                        t.default_interp = mode;
                    }
                }
                if let Some((track_id, points)) = auto_resp.generator_points {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                        t.points = points;
                    }
                }
                self.core.sync_module_to_audio();
            }
        }
    }

    fn handle_transport_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("transport_bar").show_inside(ui, |ui| {
            let transport_resp = crate::ui::transport::draw_transport(
                ui,
                &self.core.playback_state,
                &mut self.core.command_sender,
                &self.theme,
            );
            if transport_resp.prev_pattern_clicked {
                self.core.skip_to_prev_pattern();
                self.ensure_cursor_visible();
            }
            if transport_resp.next_pattern_clicked {
                self.core.skip_to_next_pattern();
                self.ensure_cursor_visible();
            }
        });
    }

    fn handle_oscilloscope(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let num_ch = self.core.num_channels();
        let panel_w = ctx.content_rect().width() - 12.0;
        let scope_height = crate::ui::oscilloscope::compute_scope_height(panel_w, num_ch);
        egui::Panel::top("oscilloscope")
            .exact_size(scope_height)
            .show_inside(ui, |ui| {
                crate::ui::oscilloscope::draw_oscilloscope(
                    ui,
                    &self.core.playback_state,
                    &self.theme,
                    num_ch,
                );
            });
    }

    fn handle_status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(22.0)
            .show_inside(ui, |ui| {
                let cpu = self.core.playback_state.cpu_usage_pct.load(std::sync::atomic::Ordering::Relaxed);
                let total_rows = self.core.current_pattern_or_default().num_rows;
                let hint = format!("Ins: {} | Smp: {}", self.core.selected_instrument, self.core.selected_sample);
                let sample_delta = crate::ui::status_bar::draw_status_bar(
                    ui,
                    self.core.module.as_ref().map(|m| m.as_ref()),
                    self.core.selected_order,
                    self.core.cursor.row,
                    total_rows,
                    self.core.num_channels(),
                    cpu,
                    self.current_octave,
                    self.cursor_skip,
                    self.core.selected_instrument,
                    self.core.selected_sample,
                    &self.core.playback_state,
                    self.edit_mode,
                    self.core.cursor.sub_column,
                    &hint,
                    &self.theme,
                );
                if let Some(d) = sample_delta {
                    self.change_selected_sample(d);
                }
            });
    }

    fn handle_pattern_tab(
        &mut self,
        ui: &mut egui::Ui,
        playback_pattern: Option<usize>,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
    ) {
        let events = self.pattern_view.ui(
            ui,
            &mut self.core,
            self.config.editor_font_size,
            self.config.get_spacing_mode(),
            self.config.get_col_vis(),
            self.config.row_highlight_minor,
            self.config.row_highlight_major,
            self.config.get_sample_length_bg(),
            &self.theme,
            playback_pattern,
            playback_row,
            playback_tick,
            playback_speed,
        );
        let mut cursor_changed = false;
        for event in events {
            match event {
                PanelEvent::AddChannel => {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut module) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            if arc_module.channel_panning.len() < MAX_CHANNELS {
                                arc_module.channel_panning.push(crate::sequencer::module::PANNING_CENTER);
                                arc_module.channel_volume.push(crate::sequencer::module::VOLUME_MAX);
                                self.core.sync_module_to_audio();
                                self.sync_channel_fields();
                            }
                        }
                    }
                }
                PanelEvent::RemoveChannel => {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut module) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            arc_module.channel_panning.pop();
                            arc_module.channel_volume.pop();
                            self.core.sync_module_to_audio();
                            self.sync_channel_fields();
                            if self.core.cursor.channel >= self.core.num_channels() {
                                self.core.cursor.channel = self.core.num_channels().saturating_sub(1);
                            }
                            if self.pattern_view.scroll_channel >= self.core.num_channels() {
                                self.pattern_view.scroll_channel = self.core.num_channels().saturating_sub(1);
                            }
                        }
                    }
                }
                PanelEvent::SetAutomationTarget { channel, target } => {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut module) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            let exists = arc_module.automation_tracks.iter().any(
                                |tr| tr.channel == Some(channel) && tr.target == target
                            );
                            if !exists {
                                let id = arc_module.next_automation_id;
                                arc_module.next_automation_id += 1;
                                arc_module.automation_tracks.push(
                                    crate::sequencer::AutomationTrack::new(id, target, Some(channel))
                                );
                            }
                        }
                    }
                    self.core.sync_module_to_audio();
                }
                PanelEvent::ContextMenuAction(action) => {
                    self.handle_context_menu_action(action);
                }
                PanelEvent::AutomationInteraction(interaction) => {
                    self.handle_automation_interaction(interaction);
                }
                PanelEvent::ToggleSampleLengthBg => {
                    self.config.toggle_sample_length_bg();
                }
                PanelEvent::SyncToAudio => {
                    cursor_changed = true;
                }
                PanelEvent::ShowPhraseGenerator => {
                    self.show_phrase_generator = true;
                }
                _ => {}
            }
        }
        if cursor_changed {
            self.ensure_cursor_visible();
        }
    }

    fn handle_sendfx_tab(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        ctx: &egui::Context,
    ) {
        #[cfg(windows)]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> = {
            crate::audio::plugins::plugin_window::get_eframe_hwnd(frame)
                .map(|h| h as usize)
        };
        #[cfg(not(windows))]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> = None;

        self.sendfx_panel.ui(
            ui,
            &mut self.core.command_sender,
            &mut self.send_bus_handles,
            eframe_hwnd,
            |send_index, state| {
                // The editor can't write to the module directly (it
                // doesn't own a &mut HtrkApp), so push to a queue that
                // gets drained immediately after the editor returns.
                self.pending_send_bus_state_writes.push((send_index, state));
            },
        );

        // Flush any pending state writes from the editor's "Remove" button.
        if !self.pending_send_bus_state_writes.is_empty() {
            self.core.ensure_module_ownership();
            if let Some(ref mut module) = self.core.module {
                if let Some(arc_module) = std::sync::Arc::get_mut(module) {
                    for (send_index, state) in self.pending_send_bus_state_writes.drain(..) {
                        if send_index < arc_module.send_bus_plugins.len() {
                            if let Some(ref mut slot) = arc_module.send_bus_plugins[send_index] {
                                slot.state = state;
                            }
                        }
                    }
                }
            }
        }

        if let Some(si) = self.sendfx_panel.plugin_browser_open_for {
            let discovered = self.discovered_plugins();
            let bus_letter = char::from(b'A' + si as u8);
            let mut open = true;
            let (result, action) = crate::ui::plugin_browser::draw_plugin_browser(
                &ctx,
                &mut open,
                si,
                &bus_letter.to_string(),
                &self.theme,
                &discovered,
                &self.plugin_browser_status,
            );
            if action.rescan_requested {
                let summary = self.rescan_plugins();
                self.plugin_browser_status =
                    crate::ui::plugin_browser::PluginBrowserStatus::Error(summary);
            }
            match result {
                crate::ui::plugin_browser::PluginSelectResult::Selected {
                    descriptor,
                    send_index,
                } => {
                    self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Loading(descriptor.name.clone());
                    let sample_rate = if self.current_sample_rate > 0 {
                        self.current_sample_rate as f64
                    } else {
                        48000.0
                    };
                    let max_block = 512;
                    // If the user picks the same plugin that was previously
                    // loaded on this bus, restore its saved state. A
                    // different plugin path starts fresh.
                    let descriptor_path = descriptor.path.to_string_lossy().to_string();
                    let initial_state: Option<Vec<u8>> = self.core.module.as_ref()
                        .and_then(|m| m.send_bus_plugins.get(send_index))
                        .and_then(|opt| opt.as_ref())
                        .filter(|slot| slot.path == descriptor_path && !slot.state.is_empty())
                        .map(|slot| slot.state.clone());
                    match crate::ui::plugin_browser::load_and_install_plugin(
                        &descriptor,
                        send_index,
                        sample_rate,
                        max_block,
                        &mut self.core.command_sender,
                        initial_state.as_deref(),
                    ) {
                        Ok((handle, name)) => {
                            self.send_bus_handles[send_index] = Some(handle);
                            self.sendfx_panel.plugin_names[send_index] = Some(name.clone());
                            // Populate the module's send_bus_plugins slot so
                            // the .htk file persists the assignment and the
                            // plugin can auto-reload next time. Capture the
                            // freshly-loaded state so the very first project
                            // save persists the plugin's default patch.
                            self.core.ensure_module_ownership();
                            if let Some(ref mut module) = self.core.module {
                                if let Some(arc_module) = std::sync::Arc::get_mut(module) {
                                    if send_index < arc_module.send_bus_plugins.len() {
                                        arc_module.send_bus_plugins[send_index] = Some(
                                            crate::sequencer::plugin::PluginSlot::new(
                                                descriptor.format.as_str(),
                                                descriptor_path,
                                                descriptor.plugin_id,
                                            )
                                        );
                                    }
                                }
                            }
                            self.save_all_send_bus_plugin_states();
                            self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Loaded(name);
                        }
                        Err(e) => {
                            eprintln!("[plugin] load failed: {e}");
                            self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Error(e);
                        }
                    }
                }
                crate::ui::plugin_browser::PluginSelectResult::Cancelled => {
                    self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Idle;
                }
            }
            if !open {
                self.sendfx_panel.plugin_browser_open_for = None;
            }
        }
    }

    fn handle_order_list(
        &mut self,
        ui: &mut egui::Ui,
        playback_order: Option<usize>,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
    ) {
        let order_list_width = self.config.order_list_width.unwrap_or(150.0);
        let order_panel_resp = egui::Panel::left("order_list")
            .resizable(true)
            .min_size(120.0)
            .default_size(order_list_width)
            .show_inside(ui, |ui| {
                if let Some(ref module) = self.core.module {
                    let order_resp = crate::ui::order_list::draw_order_list(
                        ui,
                        module,
                        self.core.selected_order,
                        playback_order,
                        playback_row,
                        playback_tick,
                        playback_speed,
                        &self.theme,
                    );
                    let should_insert = order_resp.insert_clicked;
                    let should_delete = order_resp.delete_clicked;
                    let should_duplicate = order_resp.duplicate_clicked;
                    let pattern_changed = order_resp.pattern_changed;
                    let pattern_resized = order_resp.pattern_resized;
                    let order_reordered = order_resp.order_reordered;
                    if let Some(idx) = order_resp.selected_order {
                        self.core.selected_order = idx;
                        self.core.cursor.row = 0;
                        self.ensure_cursor_visible();
                    }
                    let mut changed = false;
                    self.core.ensure_module_ownership();
                    if let Some(ref mut m) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(m) {
                            if let Some((order_idx, new_pat)) = pattern_changed {
                                if order_idx < arc_module.order_list.len() {
                                    arc_module.order_list[order_idx] = new_pat;
                                    changed = true;
                                }
                            }
                            if should_insert || should_delete {
                                if should_insert {
                                    let new_pat = arc_module.patterns.len() as u8;
                                    arc_module.patterns.push(crate::sequencer::Pattern::new(64));
                                    arc_module.order_list.insert(self.core.selected_order + 1, new_pat);
                                    changed = true;
                                }
                                if should_delete && arc_module.order_list.len() > 1 {
                                    if self.core.selected_order < arc_module.order_list.len() {
                                        arc_module.order_list.remove(self.core.selected_order);
                                        if self.core.selected_order >= arc_module.order_list.len() {
                                            self.core.selected_order = arc_module.order_list.len().saturating_sub(1);
                                        }
                                        changed = true;
                                    }
                                }
                            }
                            if let Some((from, to)) = order_reordered {
                                if from < arc_module.order_list.len() {
                                    let item = arc_module.order_list.remove(from);
                                    let insert_at = if to > from { to - 1 } else { to };
                                    let insert_at = insert_at.min(arc_module.order_list.len());
                                    arc_module.order_list.insert(insert_at, item);
                                    self.core.selected_order = insert_at;
                                    changed = true;
                                }
                            }
                            if should_duplicate {
                                let cur_pat_idx = *arc_module.order_list.get(self.core.selected_order).unwrap_or(&0) as usize;
                                if cur_pat_idx < arc_module.patterns.len() {
                                    let cloned = arc_module.patterns[cur_pat_idx].clone();
                                    let new_idx = arc_module.patterns.len() as u8;
                                    arc_module.patterns.push(cloned);
                                    let insert_at = (self.core.selected_order + 1).min(arc_module.order_list.len());
                                    arc_module.order_list.insert(insert_at, new_idx);
                                    self.core.selected_order = insert_at;
                                    changed = true;
                                }
                            }
                            if let Some((order_idx, new_rows)) = pattern_resized {
                                let pat_idx = *arc_module.order_list.get(order_idx).unwrap_or(&0) as usize;
                                if pat_idx < arc_module.patterns.len() {
                                    arc_module.patterns[pat_idx].resize_rows(new_rows);
                                    changed = true;
                                }
                            }
                        }
                    }
                    if changed {
                        self.core.sync_module_to_audio();
                    }
                } else {
                    ui.label("No module loaded");
                }
            });
        self.config.order_list_width = Some(order_panel_resp.response.rect.width());
    }

    fn handle_menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            let menu_resp = crate::ui::menu_bar::draw_menu_bar(
                ui,
                self.core.undo_manager.can_undo(),
                self.core.undo_manager.can_redo(),
                self.core.selection.is_some(),
                self.follow_playback,
                self.theme_preset,
                self.config.get_spacing_mode(),
                &self.theme,
                self.current_sample_rate,
                &self.current_sample_format,
                &mut self.col_vis,
                &self.config.recent_files,
            );
            if menu_resp.new_song { self.new_song(); }
            if menu_resp.open_file { self.open_file_dialog(); }
            if let Some(ref path) = menu_resp.open_recent {
                crate::actions::load_file(self, path);
            }
            if menu_resp.import_sample {
                self.browser_purpose = BrowserPurpose::General;
                self.file_browser.open(BrowserMode::Samples, crate::ui::file_browser::DialogMode::Open, &mut self.config);
            }
            if menu_resp.import_instrument {
                self.browser_purpose = BrowserPurpose::LoadInstrument;
                self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut self.config);
            }
            if menu_resp.save_file { crate::actions::save_current_file(self); }
            if menu_resp.save_as { self.save_as_dialog(); }
            if menu_resp.export_wav { crate::actions::open_wav_export_dialog(self); }
            if menu_resp.undo {
                self.core.ensure_module_ownership();
                if let Some(ref mut module) = self.core.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let _ = self.core.undo_manager.undo(arc_module);
                    }
                }
                self.core.sync_module_to_audio();
            }
            if menu_resp.redo {
                self.core.ensure_module_ownership();
                if let Some(ref mut module) = self.core.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let _ = self.core.undo_manager.redo(arc_module);
                    }
                }
                self.core.sync_module_to_audio();
            }
            if menu_resp.cut { self.core.copy_selection(); self.core.delete_selection(); }
            if menu_resp.copy { self.core.copy_selection(); }
            if menu_resp.paste { self.core.paste_at_cursor(); }
            if menu_resp.select_all { self.core.select_all(); }
            if menu_resp.cut_track && self.edit_mode { self.cut_track(); }
            if menu_resp.copy_track { self.copy_track(); }
            if menu_resp.delete_track && self.edit_mode { self.delete_track(); }
            if menu_resp.cut_column && self.edit_mode { self.cut_column(); }
            if menu_resp.copy_column { self.copy_column(); }
            if menu_resp.follow_playback { self.follow_playback = !self.follow_playback; }
            if let Some(preset) = menu_resp.theme_changed {
                self.theme_preset = preset;
                self.theme = TrackerTheme::from_preset(preset);
                self.config.theme_preset = preset.config_key().to_string();
                self.config.save();
            }
            if let Some(mode) = menu_resp.spacing_mode_changed {
                self.config.set_spacing_mode(mode);
                self.config.save();
            }
            if let Some(col_vis) = menu_resp.col_vis {
                self.col_vis = col_vis;
                self.config.set_col_vis(col_vis);
                self.config.save();
            }
            if menu_resp.show_shortcuts { self.show_shortcuts = true; }
            if menu_resp.show_about { self.show_about = true; }
            if menu_resp.show_settings {
                self.settings_state = crate::ui::settings_window::SettingsState::from_config(&self.config);
                self.settings_state.open = true;
            }
            if menu_resp.quit {
                if self.config.confirm_on_exit && self.core.module_dirty() {
                    self.show_exit_confirm = true;
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });
        if let Some(device_name) = self.pending_device_switch.take() {
            self.switch_output_device(device_name);
        }
        if self.pending_reinit {
            self.pending_reinit = false;
            if self.stream.is_some() {
                self.stream = None;
                self.core.command_sender = None;
                self.init_audio();
            }
        }
    }

    fn load_and_install_instrument_plugin(
        &mut self,
        descriptor: &crate::audio::plugins::PluginDescriptor,
        instrument_idx: usize,
        initial_state: Option<&[u8]>,
    ) -> Result<(Box<dyn HostedPluginHandle>, String), String> {
        let sample_rate = if self.current_sample_rate > 0 {
            self.current_sample_rate as f64
        } else {
            48000.0
        };
        crate::ui::plugin_browser::load_and_install_instrument_plugin(
            descriptor,
            instrument_idx,
            initial_state,
            sample_rate,
            &mut self.core.command_sender,
        )
    }

    fn unload_instrument_plugin(&mut self, instrument_idx: usize) {
        if instrument_idx >= self.instrument_plugin_handles.len() {
            return;
        }
        // Save state before dropping the handle. The handle's save_state()
        // is &mut self so we need a temporary move out + back in.
        if let Some(mut handle) = self.instrument_plugin_handles[instrument_idx].take() {
            if let Ok(state) = handle.save_state() {
                self.write_instrument_plugin_state(instrument_idx, state);
            }
            // Release any previewed note-off for this instrument so a
            // reloaded plugin doesn't get a stale "note on" without off.
            self.send_all_preview_note_offs_for_instrument(instrument_idx);
            // Drop the handle (plugin instance deallocated); send None to
            // audio engine so the processor is dropped there too. A proper
            // deactivation sequence would need a way to retrieve the
            // stopped processor from the audio thread — deferred to Phase 4.
            drop(handle);
        }
        if let Some(ref mut sender) = self.core.command_sender {
            sender.send(AudioCommand::InstallInstrumentPlugin {
                instrument_idx,
                processor: None,
            });
        }
    }

    /// Copy the given opaque state blob into the module's instrument slot
    /// so the next save persists it. Does nothing if the slot is missing.
    fn write_instrument_plugin_state(&mut self, instrument_idx: usize, state: Vec<u8>) {
        self.core.ensure_module_ownership();
        if let Some(ref mut module) = self.core.module {
            if let Some(arc_module) = std::sync::Arc::get_mut(module) {
                crate::ui::plugin_browser::write_plugin_state_to_slot(
                    arc_module,
                    |m| {
                        if instrument_idx < m.instruments.len() {
                            m.instruments[instrument_idx].plugin.as_mut()
                        } else {
                            None
                        }
                    },
                    state,
                );
            }
        }
    }

    /// Iterate all loaded instrument plugins, capture their state, and
    /// write it into the module's `instrument.plugin.state` field. Call
    /// this on project save so plugins reload with the same patches.
    pub(crate) fn save_all_instrument_plugin_states(&mut self) {
        // Borrow the two pieces we need as separate locals so the
        // closure passed to save_all_plugin_states can borrow them
        // independently (avoids &mut self / &mut self conflict).
        let core_module = &mut self.core.module;
        let handles = std::mem::take(&mut self.instrument_plugin_handles);
        let mut handles = handles;
        crate::ui::plugin_browser::save_all_plugin_states(&mut handles, |idx, state| {
            // Write the state into the module slot (if it still exists).
            if let Some(arc_module) =
                core_module.as_mut().and_then(|m| std::sync::Arc::get_mut(m))
            {
                crate::ui::plugin_browser::write_plugin_state_to_slot(
                    arc_module,
                    |m| {
                        if idx < m.instruments.len() {
                            m.instruments[idx].plugin.as_mut()
                        } else {
                            None
                        }
                    },
                    state,
                );
            }
        });
        self.instrument_plugin_handles = handles;
    }

    /// Send note-off for all currently-held preview notes for the given
    /// instrument. Called when the instrument is unloaded or the module
    /// is replaced.
    fn send_all_preview_note_offs_for_instrument(&mut self, instrument_idx: usize) {
        let mut remaining = Vec::new();
        for entry in self.preview_held_notes.drain(..) {
            let (key, inst, midi_ch, note_key) = entry;
            if inst as usize == instrument_idx {
                self.core.send_command(AudioCommand::PreviewInstrumentPluginNoteOff {
                    instrument_idx: inst as usize,
                    midi_channel: midi_ch,
                    note_key,
                });
            } else {
                remaining.push((key, inst, midi_ch, note_key));
            }
        }
        self.preview_held_notes = remaining;
    }

    /// Release any previewed instrument-plugin notes whose trigger key
    /// is no longer held. Called every frame from `ui()`.
    fn release_unheld_preview_notes(&mut self, ctx: &egui::Context) {
        if self.preview_held_notes.is_empty() {
            return;
        }
        let mut remaining = Vec::with_capacity(self.preview_held_notes.len());
        for (key, inst, midi_ch, note_key) in self.preview_held_notes.drain(..) {
            let still_held = ctx.input(|i| i.key_down(key));
            if still_held {
                remaining.push((key, inst, midi_ch, note_key));
            } else {
                self.core.send_command(AudioCommand::PreviewInstrumentPluginNoteOff {
                    instrument_idx: inst as usize,
                    midi_channel: midi_ch,
                    note_key,
                });
            }
        }
        self.preview_held_notes = remaining;
    }

    /// Frame-level pass for plugin editor X-close detection. Called
    /// every frame from `ui()` regardless of which tab is active so
    /// that editors don't get stuck "open" in our state when the user
    /// X-closes them from another tab.
    fn poll_plugin_editors(&mut self) {
        // Send-bus plugins.
        for si in 0..self.send_bus_handles.len() {
            if let Some(ref mut handle) = self.send_bus_handles[si] {
                if handle.is_editor_open()
                    && !crate::ui::sendfx_editor::is_editor_hwnd_visible(handle.as_ref())
                {
                    handle.close_editor();
                }
            }
        }
        // Instrument plugins.
        for handle_opt in self.instrument_plugin_handles.iter_mut() {
            if let Some(ref mut handle) = handle_opt {
                if handle.is_editor_open()
                    && !crate::ui::sendfx_editor::is_editor_hwnd_visible(handle.as_ref())
                {
                    handle.close_editor();
                }
            }
        }
    }

    /// Frame-level pass that renders a global "Embedded Plugin
    /// Editors" dock at the bottom of the window. Every plugin whose
    /// editor is currently open in Embedded mode (send-bus or
    /// instrument) gets a panel here, visible in any tab. Called
    /// every frame from `ui()`.
    fn draw_embedded_plugin_dock(&mut self, ui: &mut egui::Ui) {
        // Collect (label, &mut handle) pairs for every plugin with an
        // open embedded editor. We can't borrow self in the inner
        // loop, so we collect labels first.
        let mut send_bus_labels: Vec<(usize, String)> = Vec::new();
        for (si, opt) in self.send_bus_handles.iter().enumerate() {
            if let Some(h) = opt.as_ref() {
                if h.is_editor_open()
                    && h.editor_mode() == Some(crate::audio::plugins::EditorMode::Embedded)
                {
                    let bus_letter = char::from(b'A' + si as u8);
                    send_bus_labels.push((
                        si,
                        format!("Send Bus {} — {} (embedded)", bus_letter, h.descriptor().name),
                    ));
                }
            }
        }
        let mut instrument_labels: Vec<(usize, String)> = Vec::new();
        for (idx, opt) in self.instrument_plugin_handles.iter().enumerate() {
            if let Some(h) = opt.as_ref() {
                if h.is_editor_open()
                    && h.editor_mode() == Some(crate::audio::plugins::EditorMode::Embedded)
                {
                    instrument_labels.push((
                        idx,
                        format!("Instrument {:02X} — {} (embedded)", idx, h.descriptor().name),
                    ));
                }
            }
        }

        if send_bus_labels.is_empty() && instrument_labels.is_empty() {
            return;
        }

        ui.add_space(4.0);
        ui.separator();
        ui.label(
            egui::RichText::new("Embedded Plugin Editors")
                .strong()
                .size(13.0),
        );

        for (si, label) in send_bus_labels {
            if let Some(ref mut handle) = self.send_bus_handles[si] {
                crate::ui::sendfx_editor::draw_embedded_editor_panel(
                    ui,
                    &label,
                    handle.as_mut(),
                );
            }
        }
        for (idx, label) in instrument_labels {
            if let Some(ref mut handle) = self.instrument_plugin_handles[idx] {
                crate::ui::sendfx_editor::draw_embedded_editor_panel(
                    ui,
                    &label,
                    handle.as_mut(),
                );
            }
        }
    }

    // ─── Send-bus plugin state persistence (mirrors instrument side) ─────

    /// Iterate all loaded send-bus plugin handles, capture their state, and
    /// write it into the module's `send_bus_plugins[i].state` field. Call
    /// this on project save so plugins reload with the same patches.
    pub(crate) fn save_all_send_bus_plugin_states(&mut self) {
        // Borrow the two pieces we need as separate locals so the
        // closure passed to save_all_plugin_states can borrow them
        // independently (avoids &mut self / &mut self conflict).
        let core_module = &mut self.core.module;
        let handles = std::mem::take(&mut self.send_bus_handles);
        let mut handles = handles;
        crate::ui::plugin_browser::save_all_plugin_states(&mut handles, |idx, state| {
            if let Some(arc_module) =
                core_module.as_mut().and_then(|m| std::sync::Arc::get_mut(m))
            {
                crate::ui::plugin_browser::write_plugin_state_to_slot(
                    arc_module,
                    |m| {
                        if idx < m.send_bus_plugins.len() {
                            m.send_bus_plugins[idx].as_mut()
                        } else {
                            None
                        }
                    },
                    state,
                );
            }
        });
        self.send_bus_handles = handles;
    }

    /// Reload any send-bus plugin slots from the current module. Called
    /// from `load_file` and `new_song` so opening a saved `.htk` with
    /// send-bus plugin assignments brings the plugins back to life
    /// automatically (with their saved state restored). Slots whose path
    /// is no longer in the discovered library are left in place but not
    /// loaded; the user can re-pick from the browser to fix the path.
    pub(crate) fn sync_send_bus_plugin_state(&mut self) {
        let slots: Vec<(usize, String, String, Vec<u8>)> = match self.core.module.as_ref() {
            Some(m) => m.send_bus_plugins.iter().enumerate()
                .filter_map(|(idx, opt)| opt.as_ref().map(|slot| {
                    (idx, slot.format.clone(), slot.path.clone(), slot.state.clone())
                }))
                .collect(),
            None => return,
        };
        let discovered = self.discovered_plugins();

        // Borrow the disjoint fields the two closures need.
        let core_command_sender = &mut self.core.command_sender;
        let send_bus_handles = &mut self.send_bus_handles;
        let sendfx_panel_plugin_names = &mut self.sendfx_panel.plugin_names;
        let sample_rate = if self.current_sample_rate > 0 {
            self.current_sample_rate as f64
        } else {
            48000.0
        };

        crate::ui::plugin_browser::sync_plugin_slots_from_module(
            slots,
            &discovered,
            "send bus",
            |send_index, descriptor, initial_state| {
                let (handle, processor, name) =
                    crate::ui::plugin_browser::load_and_activate_clap_plugin(
                        descriptor, sample_rate, 512, initial_state,
                    )?;
                if let Some(ref mut sender) = core_command_sender {
                    sender.send(AudioCommand::SetSendPlugin {
                        send_index,
                        processor: Some(processor),
                    });
                } else {
                    return Err("No command sender — audio engine not running?".into());
                }
                Ok((handle, name))
            },
            |send_index, handle, name| {
                send_bus_handles[send_index] = Some(handle);
                sendfx_panel_plugin_names[send_index] = Some(name);
            },
        );
    }
}

impl eframe::App for HtrkApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let devmcp = self.devmcp.clone();
        let _guard = FrameGuard::new(devmcp.as_ref(), &ctx);

        let vp_rect = ctx.viewport_rect();
        self.config.window_width = Some(vp_rect.width());
        self.config.window_height = Some(vp_rect.height());

        // Initial plugin scan. Done on the first frame so the struct
        // constructor doesn't need to mutate `self`. The scan blocks the
        // main thread for ~1s on a typical install (50-100 plugins);
        // for very large collections we can make this incremental later.
        if !self.plugin_scan_done {
            let _ = self.rescan_plugins();
            self.plugin_scan_done = true;
        }

        if !self.preset_scan_done && self.plugin_scan_done {
            let cache_path = crate::app_config::AppConfig::config_dir().join("preset_cache.json");
            if let Ok(cached) = crate::audio::plugins::PresetLibrary::load_from_file(&cache_path) {
                eprintln!("[presets] Loaded {} preset(s) from cache", cached.preset_count());
                if let Ok(mut lib) = self.preset_library.write() {
                    *lib = cached;
                }
            } else {
                let _ = self.rescan_presets();
                if let Ok(lib) = self.preset_library.read() {
                    if lib.preset_count() > 0 {
                        let _ = lib.save_to_file(&cache_path);
                    }
                }
            }
            self.preset_scan_done = true;
        }

        // Release any previewed instrument-plugin notes whose trigger
        // key is no longer held.
        self.release_unheld_preview_notes(&ctx);

        // Sync the cached state of any open plugin editors (X-close
        // detection) regardless of which tab the user is in, so
        // editors don't get stuck "open" in our state when the user
        // X-closes them while looking at a different tab.
        self.poll_plugin_editors();

        let (playback_row, playback_order, playback_pattern, playback_tick, playback_speed) =
            self.draw_preamble(&ctx);

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.config.confirm_on_exit && self.core.module_dirty() && !self.show_exit_confirm && !self.exit_confirmed {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_exit_confirm = true;
            }
        }

        self.handle_menu_bar(ui, &ctx);

        self.handle_transport_bar(ui);
        self.handle_oscilloscope(ui, &ctx);
        self.handle_status_bar(ui);

        self.handle_order_list(ui, playback_order, playback_row, playback_tick, playback_speed);

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.core.module.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("htrk - tracker");
                    ui.add_space(20.0);
                    ui.label("No module loaded. Press Ctrl+O to open a file, or Ctrl+N for a new song.");
                });
                return;
            }

            ui.horizontal(|ui| {
            ui.dev_selectable_value("view.pattern", &mut self.current_view, AppView::Pattern, "Pattern");
            ui.dev_selectable_value("view.sample", &mut self.current_view, AppView::Sample, "Sample");
            ui.dev_selectable_value("view.instrument", &mut self.current_view, AppView::Instrument, "Instrument");
            ui.dev_selectable_value("view.sendfx", &mut self.current_view, AppView::SendFx, "Send FX");
            ui.dev_selectable_value("view.playback", &mut self.current_view, AppView::Playback, "Playback");
            ui.dev_selectable_value("view.automation", &mut self.current_view, AppView::Automation, "Automation");
            ui.dev_selectable_value("view.mixer", &mut self.current_view, AppView::Mixer, "Mixer");
            });
            ui.dev_separator("view.separator");

            match self.current_view {
                AppView::Pattern => self.handle_pattern_tab(
                    ui,
                    playback_pattern,
                    playback_row,
                    playback_tick,
                    playback_speed,
                ),
                AppView::Sample => self.handle_sample_tab(ui),
                AppView::Instrument => self.handle_instrument_tab(ui, frame),
                AppView::SendFx => self.handle_sendfx_tab(ui, frame, &ctx),
                AppView::Playback => self.handle_playback_tab(
                    ui,
                    playback_pattern,
                    playback_row,
                    playback_tick,
                    playback_speed,
                ),
                AppView::Automation => self.handle_automation_tab(ui),
                AppView::Mixer => self.handle_mixer_tab(ui),
            }
        });

        let size = ctx.viewport_rect().size();
        self.config.window_width = Some(size.x);
        self.config.window_height = Some(size.y);

        self.draw_dialogs(&ctx);

        // Global "Embedded Plugin Editors" dock at the bottom of the
        // window, visible in any tab. Resizable; user can shrink to
        // 0 height when no editors are open (which is the default
        // since we render nothing in that case).
        egui::Panel::bottom("embedded_plugin_dock")
            .resizable(true)
            .default_size(320.0)
            .min_size(0.0)
            .max_size(900.0)
            .show_inside(ui, |ui| {
                self.draw_embedded_plugin_dock(ui);
            });
    }

    fn on_exit(&mut self) {
        if let Some(ref mut server) = self.mcp_server {
            server.stop();
        }
        self.core.send_command(crate::audio::commands::AudioCommand::Stop);
        self.stream = None;
        self.core.command_sender = None;
        // Persist the preset cache atomically so the next launch can skip
        // the (potentially ~30s) rescan.
        let cache_path = crate::app_config::AppConfig::config_dir().join("preset_cache.json");
        if let Ok(lib) = self.preset_library.read() {
            if lib.preset_count() > 0 {
                let _ = lib.save_to_file(&cache_path);
            }
        }
        crate::actions::save_config(self);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        eguidev::raw_input_hook(self.devmcp.as_ref(), ctx, raw_input);
    }
}
