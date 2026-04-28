use eframe::egui;

use crate::audio::playback_state::AtomicPlaybackState;

use super::theme::TrackerTheme;

pub fn draw_oscilloscope(
    ui: &mut egui::Ui,
    playback_state: &AtomicPlaybackState,
    theme: &TrackerTheme,
) {
    let height = 64.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(10, 10, 10));

    let mid_y = rect.top() + height / 2.0;

    painter.line_segment(
        [egui::pos2(rect.left(), mid_y), egui::pos2(rect.right(), mid_y)],
        egui::Stroke::new(0.5, egui::Color32::from_rgb(40, 40, 40)),
    );

    for frac in &[0.25, 0.75] {
        let y = rect.top() + height * frac;
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(0.5, egui::Color32::from_rgb(30, 30, 30)),
        );
    }

    let (left, right) = playback_state.read_scope();
    if left.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "~ idle ~",
            egui::FontId::monospace(11.0),
            egui::Color32::from_rgb(60, 60, 60),
        );
        return;
    }

    let width = rect.width();
    let samples_per_pixel = left.len() as f32 / width;

    draw_channel_waveform(&painter, rect, &left, mid_y, height, samples_per_pixel, width, egui::Color32::from_rgb(0, 180, 80));
    draw_channel_waveform(&painter, rect, &right, mid_y, height, samples_per_pixel, width, egui::Color32::from_rgb(0, 120, 200));
}

fn draw_channel_waveform(
    painter: &egui::Painter,
    rect: egui::Rect,
    data: &[f32],
    mid_y: f32,
    height: f32,
    samples_per_pixel: f32,
    width: f32,
    color: egui::Color32,
) {
    let half_h = height / 2.0;
    for x in 0..(width as usize) {
        let start = (x as f32 * samples_per_pixel) as usize;
        let end = ((x + 1) as f32 * samples_per_pixel) as usize;
        let end = end.min(data.len());
        if start >= data.len() { break; }

        let mut min = 1.0f32;
        let mut max = -1.0f32;
        for i in start..end {
            let v = data[i];
            if v < min { min = v; }
            if v > max { max = v; }
        }

        let x_pos = rect.left() + x as f32;
        painter.line_segment(
            [
                egui::pos2(x_pos, mid_y - max * half_h),
                egui::pos2(x_pos, mid_y - min * half_h),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
}
