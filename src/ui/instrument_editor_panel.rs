use crate::audio::engine::CommandSender;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::plugins::HostedPluginHandle;
use crate::edit::EnvelopeType;
use crate::sequencer::module::Module;
use crate::ui::instrument_editor::InstrumentEditEvent;
use crate::ui::theme::TrackerTheme;
use eframe::egui;
use std::sync::Arc;

pub struct InstrumentEditor {
    pub list_width: f32,
    pub envelope_height: f32,
    pub plugin_browser_open: bool,
    pub envelope_type: EnvelopeType,
    pub envelope_visible: bool,
    /// Sample-map paint index for the sample selector popup (typed UI state, P3).
    pub paint_sample_idx: u8,
    /// "Select Sample" browser window open (P2 dialog registry).
    pub sample_browser_open: bool,
    /// Previously selected instrument (palette scroll-reset frame diff, P3).
    pub prev_selected_instrument: usize,
    /// "Generate Envelope" window open (P2 dialog registry).
    pub envelope_generator_open: bool,
    /// Hovered envelope point index from the envelope editor (P3).
    pub envelope_hovered: Option<usize>,
    /// Envelope generator dialog parameters (typed UI state, P3).
    pub envelope_gen: EnvelopeGenParams,
    /// Drag-and-drop payload from the sample palette to the sample map (P3).
    pub sample_drag_payload: Option<u8>,
    /// "Select Sample" browser search text (typed UI state, P3).
    pub sample_browser_filter: String,
    /// CLAP plugin browser search text, instrument instance (P3).
    pub plugin_browser_filter: String,
    /// Plugin parameter grid filter, instrument instance (P3).
    pub param_filter: String,
}

/// Envelope generator dialog parameters.
/// Replaces untyped `ui.data()` temp storage (P3).
#[derive(Clone, Debug)]
pub struct EnvelopeGenParams {
    pub shape_idx: usize,
    pub length: u16,
    pub cycles: f32,
    pub depth: u8,
    pub offset: u8,
    pub duty: f32,
}

impl Default for EnvelopeGenParams {
    fn default() -> Self {
        EnvelopeGenParams {
            shape_idx: 0,
            length: 256,
            cycles: 1.0,
            depth: 48,
            offset: 32,
            duty: 50.0,
        }
    }
}

impl Default for InstrumentEditor {
    fn default() -> Self {
        InstrumentEditor {
            list_width: 150.0,
            envelope_height: 180.0,
            plugin_browser_open: false,
            envelope_type: EnvelopeType::Volume,
            envelope_visible: true,
            paint_sample_idx: 0,
            sample_browser_open: false,
            prev_selected_instrument: 0,
            envelope_generator_open: false,
            envelope_hovered: None,
            envelope_gen: EnvelopeGenParams::default(),
            sample_drag_payload: None,
            sample_browser_filter: String::new(),
            plugin_browser_filter: String::new(),
            param_filter: String::new(),
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
        plugin_handle: Option<&dyn HostedPluginHandle>,
        command_sender: &mut Option<CommandSender>,
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
            plugin_handle,
            command_sender,
        )
    }
}
