use eframe::egui;

use crate::audio::playback_state::AtomicPlaybackState;

use super::theme::TrackerTheme;

const CELL_GAP: f32 = 2.0;
const MAX_SCOPE_COLS: usize = 8;
const MIN_CELL_WIDTH: f32 = 35.0;
const MAX_CELL_WIDTH: f32 = 150.0;

struct ScopeLayout {
    cols: usize,
    rows: usize,
    cell_width: f32,
    cell_height: f32,
}

fn compute_scope_layout(panel_width: f32, num_channels: usize) -> ScopeLayout {
    if num_channels == 0 {
        return ScopeLayout { cols: 0, rows: 0, cell_width: 0.0, cell_height: 0.0 };
    }
    let cols = if num_channels <= 4 {
        num_channels
    } else {
        let ideal = (panel_width / 60.0).floor() as usize;
        ideal.max(1).min(num_channels).min(MAX_SCOPE_COLS)
    };
    let total_gap = (cols - 1) as f32 * CELL_GAP;
    let cell_width = ((panel_width - total_gap) / cols as f32).clamp(MIN_CELL_WIDTH, MAX_CELL_WIDTH);
    let cell_height = cell_width * 0.53;
    let rows = (num_channels + cols - 1) / cols;
    ScopeLayout { cols, rows, cell_width, cell_height }
}

fn channel_color(ch: usize) -> egui::Color32 {
    let hue = (ch as f32 * 37.0) % 360.0;
    let (r, g, b) = hsv_to_rgb(hue / 360.0, 0.7, 0.85);
    egui::Color32::from_rgb(r, g, b)
}

fn channel_color_secondary(ch: usize) -> egui::Color32 {
    let hue = (ch as f32 * 37.0) % 360.0;
    let (r, g, b) = hsv_to_rgb(hue / 360.0, 0.5, 0.55);
    egui::Color32::from_rgb(r, g, b)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match (h * 6.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

pub fn compute_scope_height(panel_width: f32, num_channels: usize) -> f32 {
    if num_channels == 0 {
        return 0.0;
    }
    let layout = compute_scope_layout(panel_width, num_channels);
    layout.rows as f32 * layout.cell_height
}

pub fn draw_oscilloscope(
    ui: &mut egui::Ui,
    playback_state: &AtomicPlaybackState,
    _theme: &TrackerTheme,
    num_channels: usize,
) {
    if num_channels == 0 {
        return;
    }

    let panel_width = ui.available_width();
    let layout = compute_scope_layout(panel_width, num_channels);
    let total_height = layout.rows as f32 * layout.cell_height;

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(panel_width, total_height),
        egui::Sense::hover(),
    );

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(8, 8, 8));

    for ch in 0..num_channels {
        let col = ch % layout.cols;
        let row = ch / layout.cols;
        let cell_x = rect.left() + col as f32 * (layout.cell_width + CELL_GAP);
        let cell_y = rect.top() + row as f32 * (layout.cell_height + CELL_GAP);

        let cell_rect = egui::Rect::from_min_size(
            egui::pos2(cell_x, cell_y),
            egui::vec2(layout.cell_width, layout.cell_height),
        );

        let cell_painter = ui.painter_at(cell_rect);
        cell_painter.rect_filled(cell_rect, 1.0, egui::Color32::from_rgb(12, 12, 14));

        let mid_y = cell_rect.center().y;

        cell_painter.line_segment(
            [egui::pos2(cell_rect.left(), mid_y), egui::pos2(cell_rect.right(), mid_y)],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(35, 35, 40)),
        );

        let label = format!("{}", ch + 1);
        cell_painter.text(
            egui::pos2(cell_rect.left() + 2.0, cell_rect.top() + 1.0),
            egui::Align2::LEFT_TOP,
            &label,
            egui::FontId::monospace(7.0),
            egui::Color32::from_rgb(70, 70, 80),
        );

        let (left, right) = playback_state.read_channel_scope(ch);

        if left.is_empty() {
            continue;
        }

        let scope_left = cell_rect.left() + 2.0;
        let scope_top = cell_rect.top() + 10.0;
        let scope_width = cell_rect.width() - 4.0;
        let scope_height = cell_rect.height() - 12.0;
        let scope_mid_y = scope_top + scope_height / 2.0;
        let samples_per_pixel = left.len() as f32 / scope_width.max(1.0);

        draw_channel_waveform(
            &cell_painter,
            scope_left,
            scope_mid_y,
            scope_height,
            &left,
            samples_per_pixel,
            scope_width,
            channel_color(ch),
        );
        draw_channel_waveform(
            &cell_painter,
            scope_left,
            scope_mid_y,
            scope_height,
            &right,
            samples_per_pixel,
            scope_width,
            channel_color_secondary(ch),
        );
    }
}

fn draw_channel_waveform(
    painter: &egui::Painter,
    x_start: f32,
    mid_y: f32,
    height: f32,
    data: &[f32],
    samples_per_pixel: f32,
    width: f32,
    color: egui::Color32,
) {
    let half_h = height / 2.0;
    for x in 0..(width as usize) {
        let start = (x as f32 * samples_per_pixel) as usize;
        let end = ((x + 1) as f32 * samples_per_pixel) as usize;
        let end = end.min(data.len());
        if start >= data.len() {
            break;
        }

        let mut min = 1.0f32;
        let mut max = -1.0f32;
        for i in start..end {
            let v = data[i];
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
        }

        let x_pos = x_start + x as f32;
        painter.line_segment(
            [
                egui::pos2(x_pos, mid_y - max * half_h),
                egui::pos2(x_pos, mid_y - min * half_h),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
}
