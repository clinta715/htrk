use std::sync::Arc;
use eframe::egui;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::module::Module;
use crate::ui::theme::TrackerTheme;
use crate::ui::sample_editor::SampleEditEvent;

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
