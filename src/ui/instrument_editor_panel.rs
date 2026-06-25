use std::sync::Arc;
use eframe::egui;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::plugins::EditorMode;
use crate::sequencer::module::Module;
use crate::ui::theme::TrackerTheme;
use crate::ui::instrument_editor::InstrumentEditEvent;
use crate::ui::sendfx_panel::EframeHwnd;

pub struct InstrumentEditor {
    pub list_width: f32,
    pub envelope_height: f32,
    pub plugin_browser_open: bool,
    pub plugin_name: String,
    // Cached plugin-editor state for the currently selected instrument.
    // HtrkApp updates these each frame from the plugin handle, so the
    // editor UI can read them without needing a borrow of HtrkApp.
    pub plugin_has_editor: bool,
    pub plugin_editor_is_open: bool,
    pub plugin_editor_mode: Option<EditorMode>,
    pub plugin_editor_error: Option<String>,
}

impl Default for InstrumentEditor {
    fn default() -> Self {
        InstrumentEditor {
            list_width: 150.0,
            envelope_height: 180.0,
            plugin_browser_open: false,
            plugin_name: String::new(),
            plugin_has_editor: false,
            plugin_editor_is_open: false,
            plugin_editor_mode: None,
            plugin_editor_error: None,
        }
    }
}

impl InstrumentEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &Module,
        selected_instrument: &mut usize,
        selected_sample: &mut usize,
        theme: &TrackerTheme,
        playback_state: &Arc<AtomicPlaybackState>,
        config: &mut crate::app_config::AppConfig,
        eframe_hwnd: Option<EframeHwnd>,
    ) -> Option<InstrumentEditEvent> {
        crate::ui::instrument_editor::draw_instrument_editor(
            ui,
            module,
            selected_instrument,
            selected_sample,
            theme,
            playback_state,
            self,
            config,
            eframe_hwnd,
        )
    }
}
