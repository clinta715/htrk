use eframe::egui;

use crate::audio::playback_state::AtomicPlaybackState;

use super::pattern_grid::{self, CursorPosition, SubColumn};
use super::theme::TrackerTheme;

const MIN_BLOCK_W: f32 = 95.0;
const MAX_BLOCK_H: f32 = 52.0;
const BLOCK_SPACING_X: f32 = 3.0;
const BLOCK_SPACING_Y: f32 = 2.0;
const METER_H: f32 = 10.0;
const METER_PAD_X: f32 = 6.0;
const BLOCK_CORNER_RADIUS: f32 = 3.0;
const SPLITTER_HEIGHT: f32 = 4.0;

const INFO_FOOTER_H: f32 = 18.0;
const SPLIT_MIN: f32 = 0.15;
const SPLIT_MAX: f32 = 0.85;

pub fn draw_playback_view(
    ui: &mut egui::Ui,
    playback_state: &AtomicPlaybackState,
    _command_sender: &mut Option<crate::audio::engine::CommandSender>,
    theme: &TrackerTheme,
    num_channels: usize,
    pattern: Option<&crate::sequencer::pattern::Pattern>,
    module: Option<&crate::sequencer::module::Module>,
    scroll_row: usize,
    scroll_channel: usize,
    metrics: pattern_grid::GridMetrics,
    col_vis: pattern_grid::ColumnVisibility,
    highlight_minor: u8,
    highlight_major: u8,
    sample_length_bg: bool,
    playback_row: Option<usize>,
    playback_tick: Option<u8>,
    playback_speed: u8,
    split: &mut f32,
    zoom: &mut u8,
) -> usize {
    draw_zoom_toolbar(ui, theme, zoom);

    let mut visible_rows = pattern_grid::VISIBLE_ROWS;

    if let Some(pattern) = pattern {
        let avail_h = ui.available_height();
        let grid_h = (avail_h * *split).max(80.0).min(avail_h - 60.0);

        let grid_rect = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(ui.available_width(), grid_h),
        );
        let mut grid_ui = ui.new_child(egui::UiBuilder::new().max_rect(grid_rect).layout(*ui.layout()));

        let dummy_cursor = CursorPosition {
            row: usize::MAX,
            channel: 0,
            sub_column: SubColumn::Note,
        };
        let grid_resp = pattern_grid::draw_pattern_grid(
            &mut grid_ui,
            pattern,
            &dummy_cursor,
            None,
            playback_row,
            playback_tick,
            playback_speed,
            scroll_row,
            scroll_channel,
            num_channels,
            metrics,
            theme,
            highlight_minor,
            highlight_major,
            sample_length_bg,
            col_vis,
            module,
            &[],
        );
        visible_rows = grid_resp.visible_rows;
        let used_h = grid_ui.min_rect().height();
        ui.allocate_space(egui::vec2(0.0, used_h));

        draw_splitter(ui, avail_h, split);
    }

    ui.separator();

    let avail_w = ui.available_width();
    let cols = (avail_w / MIN_BLOCK_W).floor() as usize;
    let cols = cols.max(1).min(num_channels);
    let block_w = (avail_w - (cols - 1) as f32 * BLOCK_SPACING_X) / cols as f32;
    let block_h = (block_w * 0.45).min(MAX_BLOCK_H);

    let start_pos = ui.cursor().min;
    for ch in 0..num_channels {
        let col = ch % cols;
        let row = ch / cols;
        let x = start_pos.x + col as f32 * (block_w + BLOCK_SPACING_X);
        let y = start_pos.y + row as f32 * (block_h + BLOCK_SPACING_Y);

        let block_rect = egui::Rect::from_min_size(
            egui::pos2(x, y),
            egui::vec2(block_w, block_h),
        );

        let painter = ui.painter_at(block_rect);
        painter.rect_filled(block_rect, BLOCK_CORNER_RADIUS, egui::Color32::from_rgb(16, 16, 18));
        painter.rect_stroke(block_rect, BLOCK_CORNER_RADIUS, egui::Stroke::new(0.5, egui::Color32::from_rgb(30, 30, 35)), egui::StrokeKind::Outside);

        let mut block_ui = ui.new_child(
            egui::UiBuilder::new().max_rect(block_rect.shrink2(egui::vec2(3.0, 2.0))),
        );

        block_ui.label(
            egui::RichText::new(format!("Ch {:02}", ch + 1))
                .size(10.0)
                .color(theme.bg_highlight),
        );
        let note_str = playback_state.channel_note_str(ch);
        let instr_str = playback_state.channel_instrument_str(ch);
        block_ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(note_str)
                    .size(10.0)
                    .monospace()
                    .color(theme.fg_note),
            );
            ui.label(" ");
            ui.label(
                egui::RichText::new(instr_str)
                    .size(10.0)
                    .monospace()
                    .color(theme.fg_instrument),
            );
        });

        let peak = playback_state.channel_peak(ch);
        let meter_w = (block_w - METER_PAD_X * 2.0).max(8.0);
        let (r, g, b) = peak_color(peak);
        let fill = (peak * meter_w).min(meter_w);
        let meter_pos = block_ui.cursor().min;
        block_ui.painter().rect_filled(
            egui::Rect::from_min_size(meter_pos, egui::vec2(meter_w, METER_H)),
            2.0,
            egui::Color32::from_gray(40),
        );
        block_ui.painter().rect_filled(
            egui::Rect::from_min_size(meter_pos, egui::vec2(fill, METER_H)),
            2.0,
            egui::Color32::from_rgb(r, g, b),
        );
        block_ui.allocate_space(egui::vec2(meter_w, METER_H));
    }

    let total_rows = (num_channels + cols - 1) / cols;
    let used_h = total_rows as f32 * (block_h + BLOCK_SPACING_Y) - BLOCK_SPACING_Y;
    ui.allocate_space(egui::vec2(0.0, used_h));

    ui.separator();

    draw_info_footer(ui, playback_state, theme, num_channels, module, cols, block_w);

    visible_rows
}

fn draw_zoom_toolbar(ui: &mut egui::Ui, theme: &TrackerTheme, zoom: &mut u8) {
    ui.horizontal(|ui| {
        if ui.add_sized(
            egui::vec2(20.0, 16.0),
            egui::Button::new(egui::RichText::new("–").size(10.0).color(theme.transport_fg)),
        ).clicked() {
            *zoom = zoom.saturating_sub(2).max(8);
        }
        ui.label(
            egui::RichText::new(format!("{}", zoom))
                .size(10.0)
                .monospace()
                .color(theme.transport_fg),
        );
        if ui.add_sized(
            egui::vec2(20.0, 16.0),
            egui::Button::new(egui::RichText::new("+").size(10.0).color(theme.transport_fg)),
        ).clicked() {
            *zoom = (*zoom + 2).min(24);
        }
    });
}

fn draw_splitter(ui: &mut egui::Ui, avail_h: f32, split: &mut f32) {
    let separator_rect = egui::Rect::from_min_size(
        egui::pos2(ui.cursor().min.x, ui.cursor().min.y),
        egui::vec2(ui.available_width(), SPLITTER_HEIGHT),
    );

    let response = ui.allocate_rect(separator_rect, egui::Sense::click_and_drag());
    let painter = ui.painter_at(separator_rect);
    painter.rect_filled(separator_rect, 2.0, egui::Color32::from_rgb(40, 40, 45));
    painter.rect_stroke(separator_rect, 2.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(55, 55, 60)), egui::StrokeKind::Outside);

    if response.dragged_by(egui::PointerButton::Primary) {
        let dy = response.drag_delta().y;
        *split = (*split + dy / avail_h).clamp(SPLIT_MIN, SPLIT_MAX);
    }
}

fn info_fill_color(frac: f32) -> egui::Color32 {
    if frac < 0.5 {
        egui::Color32::from_rgb(60, 160, 80)
    } else if frac < 0.8 {
        egui::Color32::from_rgb(180, 160, 40)
    } else {
        egui::Color32::from_rgb(200, 60, 60)
    }
}

fn draw_info_footer(
    ui: &mut egui::Ui,
    playback_state: &AtomicPlaybackState,
    _theme: &TrackerTheme,
    num_channels: usize,
    module: Option<&crate::sequencer::module::Module>,
    cols: usize,
    block_w: f32,
) {
    let start_pos = ui.cursor().min;
    for ch in 0..num_channels {
        let col = ch % cols;
        let row = ch / cols;
        let x = start_pos.x + col as f32 * (block_w + BLOCK_SPACING_X);
        let y = start_pos.y + row as f32 * (INFO_FOOTER_H + BLOCK_SPACING_Y);

        let cell_rect = egui::Rect::from_min_size(
            egui::pos2(x, y),
            egui::vec2(block_w, INFO_FOOTER_H),
        );

        let painter = ui.painter_at(cell_rect);
        painter.rect_filled(cell_rect, 2.0, egui::Color32::from_rgb(14, 14, 16));
        painter.rect_stroke(cell_rect, 2.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(28, 28, 32)), egui::StrokeKind::Outside);

        let inner = cell_rect.shrink2(egui::vec2(3.0, 2.0));

        let sample_idx = playback_state.channel_sample_index_val(ch);
        let pos = playback_state.channel_sample_position(ch);

        let label_str = match sample_idx {
            Some(idx) => format!("S{:02}", idx),
            None => "S--".to_string(),
        };
        painter.text(
            egui::pos2(inner.left(), inner.center().y),
            egui::Align2::LEFT_CENTER,
            &label_str,
            egui::FontId::monospace(8.0),
            egui::Color32::from_gray(110),
        );

        let label_w = 20.0;
        let bar_x = inner.left() + label_w;
        let bar_w = (inner.width() - label_w - 14.0).max(4.0);
        let bar_h = 5.0;
        let bar_y = inner.center().y - bar_h / 2.0;

        let progress = pos.zip(sample_idx).and_then(|(p, idx)| {
            let sample = module?.samples.get(idx as usize)?;
            let len = sample.data.len().max(1) as f64;
            Some((p / len).min(1.0) as f32)
        });

        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(bar_x, bar_y), egui::vec2(bar_w, bar_h)),
            1.0,
            egui::Color32::from_gray(30),
        );
        if let Some(frac) = progress {
            let fill_w = (frac * bar_w).max(1.0);
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(bar_x, bar_y), egui::vec2(fill_w, bar_h)),
                1.0,
                info_fill_color(frac),
            );
        }

        let env_pos = playback_state.channel_env_pos(0, ch);
        let env_x = inner.right() - 10.0;
        let env_rect = egui::Rect::from_min_size(
            egui::pos2(env_x, inner.top()),
            egui::vec2(6.0, inner.height()),
        );
        painter.rect_filled(env_rect, 1.0, egui::Color32::from_gray(25));
        if let Some(ep) = env_pos {
            let env_frac = (ep as f64 / 1.0).min(1.0) as f32;
            let env_fill_h = (env_frac * env_rect.height()).max(1.0);
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(env_rect.left(), env_rect.bottom() - env_fill_h),
                    egui::vec2(6.0, env_fill_h),
                ),
                1.0,
                egui::Color32::from_rgb(80, 120, 200),
            );
        }
    }

    let total_rows = (num_channels + cols - 1) / cols;
    let used_h = total_rows as f32 * (INFO_FOOTER_H + BLOCK_SPACING_Y) - BLOCK_SPACING_Y;
    ui.allocate_space(egui::vec2(0.0, used_h));
}

fn peak_color(peak: f32) -> (u8, u8, u8) {
    if peak < 0.5 {
        let t = peak / 0.5;
        ((t * 255.0) as u8, (255.0 * (1.0 - t * 0.3)) as u8, 40)
    } else if peak < 0.8 {
        let t = (peak - 0.5) / 0.3;
        (255, (255.0 * (1.0 - t * 0.7)) as u8, (40.0 * (1.0 - t)) as u8)
    } else {
        (255, (255.0 * (1.0 - (peak - 0.8) / 0.2)) as u8, 0)
    }
}
