use std::sync::Arc;
use eframe::egui;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::CommandSender;
use crate::sequencer::module::Module;
use crate::sequencer::pattern::Pattern;
use crate::app_config::SpacingMode;
use crate::ui::pattern_grid::{GridMetrics, ColumnVisibility};
use crate::ui::theme::TrackerTheme;

pub struct PlaybackView {
    pub scroll_row: usize,
    pub scroll_channel: usize,
    pub split: f32,
    pub zoom: u8,
    pub last_visible_rows: usize,
}

impl Default for PlaybackView {
    fn default() -> Self {
        PlaybackView {
            scroll_row: 0,
            scroll_channel: 0,
            split: 0.35,
            zoom: 10,
            last_visible_rows: 0,
        }
    }
}

impl PlaybackView {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        playback_state: &Arc<AtomicPlaybackState>,
        command_sender: &mut Option<CommandSender>,
        theme: &TrackerTheme,
        num_channels: usize,
        pattern: Option<&Pattern>,
        module: Option<&Module>,
        config_highlight_minor: u8,
        config_highlight_major: u8,
        sample_length_bg: bool,
        col_vis: ColumnVisibility,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
        spacing_mode: SpacingMode,
    ) {
        let metrics = GridMetrics::new(
            self.zoom as f32,
            spacing_mode,
            col_vis,
        );

        let visible_rows = crate::ui::playback_view::draw_playback_view(
            ui,
            playback_state,
            command_sender,
            theme,
            num_channels,
            pattern,
            module,
            self.scroll_row,
            self.scroll_channel,
            metrics,
            col_vis,
            config_highlight_minor,
            config_highlight_major,
            sample_length_bg,
            playback_row,
            playback_tick,
            playback_speed,
            &mut self.split,
            &mut self.zoom,
        );

        self.last_visible_rows = visible_rows;

        if let Some(row) = playback_row {
            if row < self.scroll_row {
                self.scroll_row = row;
            }
            if self.last_visible_rows > 0
                && row >= self.scroll_row + self.last_visible_rows
            {
                self.scroll_row = row - self.last_visible_rows + 1;
            }
        }
    }
}
