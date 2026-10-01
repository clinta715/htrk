use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::module::Module;
use crate::ui::sample_editor::SampleEditEvent;
use crate::ui::theme::TrackerTheme;
use eframe::egui;
use std::sync::Arc;

pub struct SampleEditor {
    pub selection: Option<(usize, usize)>,
    pub clipboard: Option<Arc<Vec<f32>>>,
    pub amplify_factor: f32,
    pub waveform_visible: bool,
    pub list_width: f32,
    pub waveform_height: f32,
    pub cursor_pos: Option<usize>,
    pub zoom: f32,
    pub scroll_offset: f32,
    pub last_sample_index: usize,
    pub selected_samples: Vec<usize>,
    /// Loop-marker drag in progress (typed UI state, P3).
    pub dragging_marker: Option<usize>,
    /// Region-selection drag in progress (typed UI state, P3).
    pub selecting: bool,
}

impl Default for SampleEditor {
    fn default() -> Self {
        SampleEditor {
            selection: None,
            clipboard: None,
            amplify_factor: 1.0,
            waveform_visible: true,
            list_width: 200.0,
            waveform_height: 150.0,
            cursor_pos: None,
            zoom: 0.0,
            scroll_offset: 0.0,
            last_sample_index: 0,
            selected_samples: Vec::new(),
            dragging_marker: None,
            selecting: false,
        }
    }
}

impl SampleEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &Module,
        selected_sample: &mut usize,
        theme: &TrackerTheme,
        playback_state: &Arc<AtomicPlaybackState>,
    ) -> Option<SampleEditEvent> {
        crate::ui::sample_editor::draw_sample_editor(
            ui,
            module,
            selected_sample,
            theme,
            playback_state,
            self,
        )
    }
}
