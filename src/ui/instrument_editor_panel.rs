use std::sync::Arc;
use eframe::egui;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::module::Module;
use crate::ui::theme::TrackerTheme;
use crate::ui::instrument_editor::InstrumentEditEvent;

pub struct InstrumentEditor {
    pub list_width: f32,
    pub envelope_height: f32,
    pub plugin_browser_open: bool,
    pub plugin_name: String,
}

impl Default for InstrumentEditor {
    fn default() -> Self {
        InstrumentEditor {
            list_width: 150.0,
            envelope_height: 180.0,
            plugin_browser_open: false,
            plugin_name: String::new(),
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
        )
    }
}
