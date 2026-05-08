use eframe::egui;
use crate::sequencer::Module;

const INLINE_PALETTE_HEIGHT: f32 = 80.0;

pub fn draw_inline_sample_palette(
    ui: &mut egui::Ui,
    module: &Module,
    paint_sample: &mut u8,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Samples").font(egui::FontId::proportional(11.0))
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("Selected: {:02X}", paint_sample))
                        .font(egui::FontId::monospace(10.0))
                        .color(egui::Color32::from_rgb(180, 220, 180)),
                );
            });
        });

        egui::ScrollArea::vertical()
            .max_height(INLINE_PALETTE_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut any_clicked = false;
                for i in 0..module.samples.len().min(100) {
                    let sample = &module.samples[i];
                    let is_selected = *paint_sample == i as u8;
                    let has_data = !sample.data.is_empty();

                    let bg = if is_selected {
                        egui::Color32::from_rgb(40, 70, 40)
                    } else if has_data {
                        egui::Color32::from_rgb(30, 30, 35)
                    } else {
                        egui::Color32::from_rgb(18, 18, 20)
                    };

                    let label_text = if sample.name.is_empty() && !has_data {
                        format!("{:02X}: ---", i)
                    } else {
                        format!("{:02X}: {}", i, sample.name)
                    };

                    let response = ui.horizontal(|ui| {
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 16.0),
                            egui::Sense::click(),
                        );
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 2.0, bg);

                        if has_data {
                            draw_waveform_thumbnail(&painter, rect, &sample.data, is_selected);
                        }

                        let text_color = if is_selected {
                            egui::Color32::WHITE
                        } else if has_data {
                            egui::Color32::from_rgb(170, 170, 180)
                        } else {
                            egui::Color32::from_rgb(80, 80, 85)
                        };

                        painter.text(
                            egui::pos2(rect.left() + 4.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &label_text,
                            egui::FontId::monospace(10.0),
                            text_color,
                        );

                        if is_selected {
                            painter.rect_filled(
                                egui::Rect::from_min_max(
                                    egui::pos2(rect.left(), rect.top()),
                                    egui::pos2(rect.left() + 2.0, rect.bottom()),
                                ),
                                0.0,
                                egui::Color32::from_rgb(100, 220, 100),
                            );
                        }

                        resp
                    });

                    if response.inner.clicked() && !any_clicked {
                        *paint_sample = i as u8;
                        any_clicked = true;
                    }
                }
            });
    });
}

fn draw_waveform_thumbnail(
    painter: &egui::Painter,
    rect: egui::Rect,
    data: &std::sync::Arc<Vec<f32>>,
    is_selected: bool,
) {
    let thumb_left = rect.right() - 70.0;
    let thumb_width = 64.0;
    let thumb_height = rect.height() - 4.0;
    let mid_y = rect.center().y;
    let half_h = thumb_height / 2.0;

    let color = if is_selected {
        egui::Color32::from_rgb(100, 200, 120)
    } else {
        egui::Color32::from_rgb(60, 100, 70)
    };

    let len = data.len();
    if len == 0 {
        return;
    }
    let samples_per_pixel = len as f32 / thumb_width;
    for x in 0..(thumb_width as usize) {
        let start = (x as f32 * samples_per_pixel) as usize;
        let end = ((x + 1) as f32 * samples_per_pixel) as usize;
        let end = end.min(len);
        if start >= len {
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

        let x_pos = thumb_left + x as f32;
        painter.line_segment(
            [
                egui::pos2(x_pos, mid_y - max * half_h),
                egui::pos2(x_pos, mid_y - min * half_h),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
}

pub struct SampleBrowserResult {
    pub selected_sample: u8,
    pub open: bool,
}

pub fn draw_sample_browser_popup(
    ctx: &egui::Context,
    module: &Module,
    paint_sample: u8,
    open: &mut bool,
) -> Option<u8> {
    let mut result = None;

    let window_name = "sample_browser";

    egui::Window::new("Sample Browser")
        .id(egui::Id::new(window_name))
        .open(open)
        .resizable(true)
        .default_size(egui::vec2(360.0, 400.0))
        .min_size(egui::vec2(200.0, 200.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for i in 0..module.samples.len().min(100) {
                            let sample = &module.samples[i];
                            let has_data = !sample.data.is_empty();
                            let is_selected = paint_sample == i as u8;

                            let bg = if is_selected {
                                egui::Color32::from_rgb(35, 60, 35)
                            } else if has_data {
                                egui::Color32::from_rgb(25, 25, 30)
                            } else {
                                egui::Color32::TRANSPARENT
                            };

                    let frame = egui::Frame::NONE
                        .fill(bg)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::same(6))
                        .outer_margin(egui::Margin::symmetric(0, 2));

                            frame.show(ui, |ui| {
                                let resp = ui.allocate_response(
                                    ui.available_size(),
                                    egui::Sense::click(),
                                );

                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(140.0);
                                        let name = if sample.name.is_empty() {
                                            format!("{:02X}: (unnamed)", i)
                                        } else {
                                            format!("{:02X}: {}", i, sample.name)
                                        };
                                        ui.label(
                                            egui::RichText::new(name)
                                                .font(egui::FontId::monospace(11.0))
                                                .color(if is_selected {
                                                    egui::Color32::from_rgb(140, 230, 140)
                                                } else if has_data {
                                                    egui::Color32::WHITE
                                                } else {
                                                    egui::Color32::from_rgb(100, 100, 100)
                                                }),
                                        );

                                        if has_data {
                                            let dur_secs = sample.data.len() as f64 / sample.sample_rate as f64;
                                            let info = format!(
                                                "{}Hz | {} smpl | {:.1}s",
                                                sample.sample_rate,
                                                sample.data.len(),
                                                dur_secs,
                                            );
                                            ui.label(
                                                egui::RichText::new(info)
                                                    .font(egui::FontId::monospace(9.0))
                                                    .color(egui::Color32::from_rgb(120, 120, 130)),
                                            );
                                        }
                                    });

                                    if has_data {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width().max(80.0), 28.0),
                                            egui::Sense::hover(),
                                        );
                                        let painter = ui.painter_at(rect);
                                        painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(15, 15, 18));
                                        draw_waveform_thumbnail_browser(&painter, rect, &sample.data, is_selected);
                                    }
                                });

                                if resp.clicked() {
                                    result = Some(i as u8);
                                }
                            });
                        }
                    });
            });
        });

    result
}

fn draw_waveform_thumbnail_browser(
    painter: &egui::Painter,
    rect: egui::Rect,
    data: &std::sync::Arc<Vec<f32>>,
    is_selected: bool,
) {
    let mid_y = rect.center().y;
    let half_h = (rect.height() - 4.0) / 2.0;
    let width = rect.width() - 4.0;
    let left = rect.left() + 2.0;

    let color = if is_selected {
        egui::Color32::from_rgb(90, 190, 110)
    } else {
        egui::Color32::from_rgb(50, 90, 60)
    };

    let len = data.len();
    if len == 0 {
        return;
    }
    let samples_per_pixel = len as f32 / width;
    for x in 0..(width as usize) {
        let start = (x as f32 * samples_per_pixel) as usize;
        let end = ((x + 1) as f32 * samples_per_pixel) as usize;
        let end = end.min(len);
        if start >= len {
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

        let x_pos = left + x as f32;
        painter.line_segment(
            [
                egui::pos2(x_pos, mid_y - max * half_h),
                egui::pos2(x_pos, mid_y - min * half_h),
            ],
            egui::Stroke::new(1.0, color),
        );
    }
}
