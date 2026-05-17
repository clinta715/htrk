use eframe::egui;

use crate::audio::playback_state::AtomicPlaybackState;

use super::oscilloscope;
use super::theme::TrackerTheme;
use super::transport;

pub fn draw_playback_view(
    ui: &mut egui::Ui,
    playback_state: &AtomicPlaybackState,
    command_sender: &mut Option<crate::audio::engine::CommandSender>,
    theme: &TrackerTheme,
    num_channels: usize,
) {
    let _transport_resp = transport::draw_transport(ui, playback_state, command_sender, theme);
    ui.separator();

    let avail = ui.available_width();
    let cols = ((avail - 4.0) / 112.0).floor() as usize;
    let cols = cols.max(1).min(num_channels);

    egui::Grid::new("playback_channel_grid")
        .striped(true)
        .spacing(egui::vec2(2.0, 1.0))
        .min_col_width(110.0)
        .show(ui, |ui| {
            let mut ch_in_row = 0;
            for ch in 0..num_channels {
                if ch_in_row >= cols {
                    ui.end_row();
                    ch_in_row = 0;
                }

                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Ch {:02}", ch + 1))
                            .size(11.0)
                            .color(theme.bg_highlight),
                    );
                    let note_str = playback_state.channel_note_str(ch);
                    let instr_str = playback_state.channel_instrument_str(ch);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(note_str)
                                .size(12.0)
                                .monospace()
                                .color(theme.fg_note),
                        );
                        ui.label(" ");
                        ui.label(
                            egui::RichText::new(instr_str)
                                .size(12.0)
                                .monospace()
                                .color(theme.fg_instrument),
                        );
                    });

                    let peak = playback_state.channel_peak(ch);
                    let meter_w = 100.0;
                    let meter_h = 6.0;
                    let (r, g, b) = peak_color(peak);
                    let fill = (peak * meter_w).min(meter_w);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            ui.next_widget_position(),
                            egui::vec2(meter_w, meter_h),
                        ),
                        1.0,
                        egui::Color32::from_gray(40),
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            ui.next_widget_position(),
                            egui::vec2(fill, meter_h),
                        ),
                        1.0,
                        egui::Color32::from_rgb(r, g, b),
                    );
                    ui.allocate_space(egui::vec2(meter_w, meter_h));
                });
                ch_in_row += 1;
            }
        });

    ui.separator();

    oscilloscope::draw_oscilloscope(ui, playback_state, theme, num_channels);

    ui.separator();

    draw_spectrum_view(ui, playback_state, theme, num_channels);
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

fn draw_spectrum_view(
    ui: &mut egui::Ui,
    playback_state: &AtomicPlaybackState,
    theme: &TrackerTheme,
    num_channels: usize,
) {
    ui.label(
        egui::RichText::new("Channel Spectrum")
            .size(12.0)
            .color(theme.bg_highlight),
    );

    let avail = ui.available_width();
    let bar_count = 16.min(num_channels);
    let bar_w = (avail - 4.0) / bar_count as f32;
    let bar_h_max = 60.0;

    egui::Grid::new("spectrum_grid")
        .spacing(egui::vec2(2.0, 1.0))
        .show(ui, |ui| {
            for ch in 0..bar_count {
                let (left, _right) = playback_state.read_channel_scope(ch);
                let energy = if left.is_empty() {
                    0.0
                } else {
                    let sum: f32 = left.iter().map(|&s| {
                        let normalized = (s as f32) / (u32::MAX as f32);
                        normalized
                    }).sum();
                    let avg = sum / left.len() as f32;
                    (avg * 4.0).min(1.0)
                };

                let x = ui.next_widget_position().x;
                let y = ui.next_widget_position().y;
                let fill_h = (energy * bar_h_max).max(1.0);

                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, y + bar_h_max - fill_h),
                        egui::vec2(bar_w - 2.0, fill_h),
                    ),
                    1.0,
                    egui::Color32::from_rgb(60, 180, 100),
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(bar_w - 2.0, bar_h_max),
                    ),
                    1.0,
                    egui::Color32::from_gray(20),
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, y + bar_h_max - fill_h),
                        egui::vec2(bar_w - 2.0, fill_h),
                    ),
                    1.0,
                    egui::Color32::from_rgb(80, 220, 120),
                );

                ui.label(
                    egui::RichText::new(format!("{}", ch + 1))
                        .size(8.0)
                        .color(egui::Color32::from_gray(100)),
                );
                ui.allocate_space(egui::vec2(bar_w, bar_h_max + 12.0));
            }
        });
}
