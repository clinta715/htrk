use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::RwLock;

use eframe::egui;
use eguidev::{DevMcp, DevUiExt, FixtureSpec, FrameGuard};

use crate::app_config::AppConfig;
use crate::audio::commands::AudioCommand;
use crate::audio::engine::create_engine_and_sender;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::plugins::{PluginLibrary, PresetLibrary};
#[cfg(feature = "audio_debug")]
use crate::debug_log;
use crate::mcp::library::SampleLibrary;

use crate::audio::plugins::HostedPluginHandle;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::sequencer::pattern::Cell;
use crate::sequencer::{DEFAULT_CHANNELS, MAX_CHANNELS};
use crate::ui::file_browser::{BrowserMode, FileBrowser};
use crate::ui::panel_event::PanelEvent;
use crate::ui::pattern_grid::{ColumnVisibility, Selection, SubColumn};
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

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserPurpose {
    #[default]
    General,
    LoadInstrument,
    SaveInstrument,
    ExportInstrument(usize),
    SaveProject,
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
    /// True after the initial sample library cache load (or attempted
    /// load) has run, so we don't try to read the cache file every frame.
    pub(crate) sample_library_loaded: bool,
    /// Status of the plugin browser dialog (loading / error / loaded).
    /// Phase 2 plugin hosting.
    pub(crate) plugin_browser_status: crate::ui::plugin_browser::PluginBrowserStatus,
    pub(crate) alt_l_count: u8,
    pub(crate) alt_l_last: Option<std::time::Instant>,
    pub(crate) automation_editor: crate::ui::automation_editor_panel::AutomationEditor,
    pub(crate) instrument_editor: crate::ui::instrument_editor_panel::InstrumentEditor,
    pub(crate) mixer_state: crate::ui::mixer_view::MixerState,
    pub(crate) devmcp: Arc<DevMcp>,
    pub(crate) pending_new_song: Arc<AtomicBool>,
    pub(crate) pending_view_switch: Arc<AtomicU8>,
    pub(crate) show_exit_confirm: bool,
    pub(crate) exit_confirmed: bool,
    pub(crate) close_after_save: bool,
    pub(crate) show_phrase_generator: bool,
    /// Phrase generator dialog parameters (typed session-only UI state, P3).
    pub(crate) phrase_gen_state: crate::ui::phrase_generator_dialog::PhraseGenState,
    pub(crate) slice_dialog_open: bool,
    pub(crate) sample_library_state: crate::ui::sample_library::SampleLibraryState,
    pub(crate) sample_library: Arc<RwLock<SampleLibrary>>,
    pub(crate) slice_config: crate::actions::slice_to_instrument::SliceConfig,

    // --- Menu bar keyboard navigation (Alt-tap / Alt+letter) ---
    /// Folded Alt-menu state machine (§24, UI_BUG_AUDIT P1). All menu-nav
    /// logic lives in `MenuNavState::handle_key`/`frame`
    /// (`src/actions/input.rs`); the bar renderer reads through the
    /// snapshot taken in `handle_menu_bar`.
    pub(crate) menu_nav: crate::actions::MenuNavState,

    pub(crate) mcp_server: Option<crate::mcp::McpServer>,
    /// Pointer identity (as `usize`) of the last `Arc<Module>` serialized
    /// into the MCP snapshot. Used to skip the expensive per-frame
    /// re-serialization when the module hasn't changed (the `Arc` pointer
    /// only changes when `ensure_module_ownership()` clones the Module
    /// during an edit). `None` forces a rebuild on the next frame.
    mcp_last_module_ptr: Option<usize>,
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
            preset_library: Arc::new(RwLock::new(crate::audio::plugins::PresetLibrary::new())),
            preset_scan_done: false,
            sample_library_loaded: false,
            plugin_browser_status: crate::ui::plugin_browser::PluginBrowserStatus::Idle,
            automation_editor: crate::ui::automation_editor_panel::AutomationEditor::default(),
            instrument_editor: crate::ui::instrument_editor_panel::InstrumentEditor {
                list_width: config.instrument_list_width.unwrap_or(150.0),
                envelope_height: config.instrument_envelope_height.unwrap_or(180.0),
                envelope_type: config
                    .instrument_envelope_type
                    .and_then(|v| {
                        use crate::edit::EnvelopeType;
                        match v {
                            0 => Some(EnvelopeType::Volume),
                            1 => Some(EnvelopeType::Panning),
                            2 => Some(EnvelopeType::Pitch),
                            3 => Some(EnvelopeType::Filter),
                            _ => None,
                        }
                    })
                    .unwrap_or(crate::edit::EnvelopeType::Volume),
                envelope_visible: config.instrument_envelope_visible.unwrap_or(true),
                ..crate::ui::instrument_editor_panel::InstrumentEditor::default()
            },
            mixer_state: crate::ui::mixer_view::MixerState::default(),
            devmcp,
            pending_view_switch: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            pending_new_song: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            show_exit_confirm: false,
            exit_confirmed: false,
            close_after_save: false,
            show_phrase_generator: false,
            phrase_gen_state: crate::ui::phrase_generator_dialog::PhraseGenState::default(),
            slice_dialog_open: false,
            sample_library_state: crate::ui::sample_library::SampleLibraryState::default(),
            sample_library: Arc::new(RwLock::new(SampleLibrary::new())),
            slice_config: crate::actions::slice_to_instrument::SliceConfig::default(),
            menu_nav: crate::actions::MenuNavState::default(),
            mcp_server: None,
            mcp_last_module_ptr: None,
        }
    }

    fn from_config(config: AppConfig) -> Self {
        let inst_list_w = config.instrument_list_width.unwrap_or(150.0);
        let inst_env_h = config.instrument_envelope_height.unwrap_or(180.0);
        let inst_env_type = config
            .instrument_envelope_type
            .and_then(|v| {
                use crate::edit::EnvelopeType;
                match v {
                    0 => Some(EnvelopeType::Volume),
                    1 => Some(EnvelopeType::Panning),
                    2 => Some(EnvelopeType::Pitch),
                    3 => Some(EnvelopeType::Filter),
                    _ => None,
                }
            })
            .unwrap_or(crate::edit::EnvelopeType::Volume);
        let inst_env_visible = config.instrument_envelope_visible.unwrap_or(true);
        let mut file_browser = FileBrowser::default();
        file_browser.restore_last_dirs(&config);
        file_browser.restore_favorites(&config.favorites);
        file_browser.restore_widths_from_config(&config);
        let playback_state = Arc::new(AtomicPlaybackState::default());
        let pending_view_switch = Arc::new(AtomicU8::new(0));
        let pending_new_song = Arc::new(AtomicBool::new(false));
        let mcp_enabled = config.mcp_enabled;
        let mcp_port = config.mcp_port;
        let mcp_http_enabled = config.mcp_http_enabled;
        let mcp_http_port = config.mcp_http_port;
        let library_roots: Vec<std::path::PathBuf> = config
            .library_roots
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        let plugin_scan_paths: Vec<std::path::PathBuf> = config
            .plugin_scan_paths
            .iter()
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
                ThemePreset::from_name(&config.theme_preset).unwrap_or(ThemePreset::DarkModern),
            ),
            theme_preset: ThemePreset::from_name(&config.theme_preset)
                .unwrap_or(ThemePreset::DarkModern),
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
            sample_library_loaded: false,
            plugin_browser_status: crate::ui::plugin_browser::PluginBrowserStatus::Idle,
            automation_editor: crate::ui::automation_editor_panel::AutomationEditor::default(),
            instrument_editor: crate::ui::instrument_editor_panel::InstrumentEditor {
                list_width: inst_list_w,
                envelope_height: inst_env_h,
                envelope_type: inst_env_type,
                envelope_visible: inst_env_visible,
                ..crate::ui::instrument_editor_panel::InstrumentEditor::default()
            },
            mixer_state: crate::ui::mixer_view::MixerState::default(),
            pending_view_switch: pending_view_switch.clone(),
            pending_new_song: pending_new_song.clone(),
            show_exit_confirm: false,
            exit_confirmed: false,
            close_after_save: false,
            show_phrase_generator: false,
            phrase_gen_state: crate::ui::phrase_generator_dialog::PhraseGenState::default(),
            slice_dialog_open: false,
            sample_library_state: crate::ui::sample_library::SampleLibraryState::default(),
            sample_library: Arc::new(RwLock::new(SampleLibrary::new())),
            slice_config: crate::actions::slice_to_instrument::SliceConfig::default(),
            menu_nav: crate::actions::MenuNavState::default(),
            devmcp: {
                let ps = pending_view_switch.clone();
                let pns = pending_new_song.clone();
                let devmcp = DevMcp::new()
                    .verbose_logging(true)
                    .fixtures([
                        FixtureSpec::new(
                            "empty_project",
                            "A brand new empty project (default after launch)",
                        )
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
                        if name == "empty_project" || name == "pattern_view" {
                            pns.store(true, Ordering::Relaxed);
                        }
                        ps.store(view, Ordering::Relaxed);
                        Ok(())
                    });

                #[cfg(feature = "devtools")]
                let devmcp = eguidev_runtime::attach(devmcp);

                Arc::new(devmcp)
            },
            mcp_server: if mcp_enabled {
                let http_port = if mcp_http_enabled {
                    Some(mcp_http_port)
                } else {
                    None
                };
                let server =
                    crate::mcp::McpServer::start(mcp_port, http_port, preset_library.clone());
                eprintln!("[app] MCP server enabled (TCP port {})", server.port);
                if let Some(hp) = server.http_port {
                    eprintln!("[app] MCP HTTP SSE transport enabled (port {hp})");
                }
                if !library_roots.is_empty() {
                    if let Ok(mut lib) = server.library.write() {
                        lib.set_roots(library_roots.clone());
                        eprintln!(
                            "[app] Sample library configured with {} root(s)",
                            library_roots.len()
                        );
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
                    eprintln!(
                        "[app] Plugin library: {} .clap file(s) found in {} root(s)",
                        found.len(),
                        plugin_scan_paths.len()
                            + crate::audio::plugins::default_search_paths().len()
                    );

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
                    eprintln!(
                        "[app] Plugin library: {} descriptor(s) loaded",
                        lib.descriptor_count()
                    );
                }
                Some(server)
            } else {
                None
            },
            // Initial plugin scan is triggered from `update()` so the
            // struct constructor doesn't need to mutate itself.
            mcp_last_module_ptr: None,
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
        self.plugin_library
            .list_descriptors()
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
        eprintln!(
            "[plugins] Found {} .clap file(s) ({} errors)",
            scan.clap_files.len(),
            scan.errors.len()
        );

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
            found_count,
            error_count,
            if sample_names.is_empty() {
                "(none)".to_string()
            } else {
                sample_names.join(", ")
            }
        );

        format!(
            "Scan complete: {} plugin(s) found, {} error(s), {} .clap file(s) on disk",
            found_count,
            error_count,
            scan.clap_files.len()
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

        eprintln!(
            "[presets] Scanning {} plugin(s) for presets...",
            paths.len()
        );
        let start = std::time::Instant::now();

        let summary = preset_discovery::scan_plugins_for_presets(&paths);

        let elapsed = start.elapsed();
        // The preset_library Arc is shared with the MCP server, so this
        // single write is visible to both the UI and MCP tools — no
        // explicit mirror step is needed.
        let count = if let Ok(mut lib) = self.preset_library.write() {
            lib.clear();
            lib.add_presets(summary.presets);
            lib.set_last_scan_time(std::time::SystemTime::now());
            lib.preset_count()
        } else {
            0
        };

        eprintln!(
            "[presets] Done: {} preset(s) from {} plugin(s), {} error(s), {} skipped (no factory) in {:.1}s",
            count,
            paths.len(),
            summary.errors.len(),
            summary.skipped_no_factory,
            elapsed.as_secs_f32()
        );

        format!(
            "Preset scan complete: {} preset(s) found from {} plugin(s), {} error(s), {} skipped (no factory) in {:.1}s",
            count,
            paths.len(),
            summary.errors.len(),
            summary.skipped_no_factory,
            elapsed.as_secs_f32()
        )
    }
}

mod audio_setup;
mod dialog_registry;
mod dialogs;
mod editing;
mod eframe_impl;
mod lifecycle;
mod panels;
mod plugins;
mod preamble;
