use eframe::egui;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::Module;
use crate::ui::style::FONT_BODY;
use crate::ui::TrackerTheme;

const INLINE_PALETTE_HEIGHT: f32 = 120.0;

pub fn draw_inline_sample_palette(
    ui: &mut egui::Ui,
    module: &Module,
    paint_sample: &mut u8,
    playback_state: &AtomicPlaybackState,
    theme: &TrackerTheme,
    reset_scroll: bool,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Samples").font(egui::FontId::proportional(11.0))
            );
        });

        egui::ScrollArea::vertical()
            .id_salt("inline_sample_palette_scroll")
            .max_height(INLINE_PALETTE_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if reset_scroll {
                    let (_, resp) = ui.allocate_exact_size(egui::vec2(1.0, 1.0), egui::Sense::hover());
                    ui.scroll_to_rect(resp.rect, Some(egui::Align::TOP));
                }
                let mut any_clicked = false;
                for i in 0..module.samples.len().min(1000) {
                    let sample = &module.samples[i];
                    let is_selected = *paint_sample == i as u8;
                    let has_data = !sample.data.is_empty();

                    let bg = if is_selected {
                        theme.bg_playback
                    } else if has_data {
                        theme.panel_border
                    } else {
                        theme.panel_bg
                    };

                    let label_text = if sample.name.is_empty() && !has_data {
                        format!("{:02X}: ---", i)
                    } else {
                        format!("{:02X}: {}", i, sample.name)
                    };

                    let response = ui.horizontal(|ui| {
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 16.0),
                            egui::Sense::click_and_drag(),
                        );
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 2.0, bg);

                        let positions = if has_data {
                            playback_state.sample_positions_for(i)
                        } else {
                            Vec::new()
                        };

                        if has_data {
                            draw_waveform_thumbnail(&painter, rect, &sample.data, is_selected, &positions, theme);
                        }

                        if !positions.is_empty() {
                            painter.rect_filled(
                                egui::Rect::from_min_max(
                                    egui::pos2(rect.left(), rect.top()),
                                    egui::pos2(rect.left() + 2.0, rect.bottom()),
                                ),
                                0.0,
                                theme.playback_position_line,
                            );
                        }

                        let text_color = if is_selected {
                            egui::Color32::WHITE
                        } else if has_data {
                            theme.fg_text
                        } else {
                            theme.fg_dim
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
                                theme.fg_volume,
                            );
                        }

                        eguidev::track_response_full(
                            format!("inst.palette.row.{}", i),
                            &resp,
                            eguidev::WidgetMeta {
                                role: eguidev::WidgetRole::Label,
                                label: Some(label_text.clone()),
                                value: Some(eguidev::WidgetValue::Text(label_text.clone())),
                                visible: ui.is_visible() && ui.is_rect_visible(resp.rect),
                                ..Default::default()
                            },
                        );

                        // Drag source for sample-map drop target
                        if resp.drag_started() {
                            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("sample_drag_payload"), i as u8));
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

pub(crate) fn draw_waveform_thumbnail(
    painter: &egui::Painter,
    rect: egui::Rect,
    data: &std::sync::Arc<Vec<f32>>,
    is_selected: bool,
    playback_positions: &[f64],
    theme: &TrackerTheme,
) {
    let thumb_left = rect.right() - 70.0;
    let thumb_width = 64.0;
    let thumb_height = rect.height() - 4.0;
    let mid_y = rect.center().y;
    let half_h = thumb_height / 2.0;

    let is_playing = !playback_positions.is_empty();
    let color = if is_playing {
        theme.sample_thumb_playing
    } else if is_selected {
        theme.sample_thumb_selected
    } else {
        theme.sample_thumb_default
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

    for &pos in playback_positions {
        let x_pos = thumb_left + (pos as f32 / len as f32) * thumb_width;
        if x_pos >= thumb_left && x_pos <= thumb_left + thumb_width {
            painter.line_segment(
                [egui::pos2(x_pos, rect.top()), egui::pos2(x_pos, rect.bottom())],
                egui::Stroke::new(1.0, theme.playback_position_line),
            );
        }
    }
}

pub fn draw_sample_browser_popup(
    ctx: &egui::Context,
    module: &Module,
    paint_sample: u8,
    open: &mut bool,
    playback_state: &AtomicPlaybackState,
    theme: &TrackerTheme,
) -> Option<u8> {
    let mut result = None;
    let mut should_close = false;

    egui::Window::new("Select Sample")
        .id(egui::Id::new("sample_browser"))
        .open(open)
        .resizable(true)
        .default_size(egui::vec2(280.0, 350.0))
        .min_size(egui::vec2(180.0, 150.0))
        .show(ctx, |ui| {
            let filter_id = ui.make_persistent_id("sample_browser_filter");
            let mut filter = ui.data(|d| d.get_temp::<String>(filter_id).unwrap_or_default());

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Search:").size(FONT_BODY).color(theme.fg_dim));
                let resp = ui.add(egui::TextEdit::singleline(&mut filter).desired_width(ui.available_width()));
                if resp.changed() {
                    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
                }
            });
            ui.add_space(2.0);

            let filter_lower = filter.to_lowercase();

            egui::ScrollArea::vertical()
                .id_salt("sample_browser_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for i in 0..module.samples.len().min(1000) {
                        let sample = &module.samples[i];
                        if !filter_lower.is_empty()
                            && !sample.name.to_lowercase().contains(&filter_lower)
                        {
                            continue;
                        }
                        let has_data = !sample.data.is_empty();
                        let is_selected = paint_sample == i as u8;

                        let bg = if is_selected {
                            theme.bg_selected
                        } else if has_data {
                            theme.bg_highlight.gamma_multiply(0.3)
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 18.0),
                            egui::Sense::click(),
                        );

                        let painter = ui.painter_at(rect);
                        if bg != egui::Color32::TRANSPARENT {
                            painter.rect_filled(rect, 2.0, bg);
                        }

                        if resp.hovered() && !is_selected {
                            painter.rect_filled(rect, 2.0, theme.bg_highlight.gamma_multiply(0.2));
                        }

                        let text_color = if is_selected {
                            egui::Color32::WHITE
                        } else if has_data {
                            theme.fg_text
                        } else {
                            theme.fg_dim
                        };

                        let name = if sample.name.is_empty() {
                            format!("{:02X}: ---", i)
                        } else {
                            format!("{:02X}: {}", i, sample.name)
                        };

                        painter.text(
                            egui::pos2(rect.left() + 4.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &name,
                            egui::FontId::monospace(11.0),
                            text_color,
                        );

                        if has_data {
                            let thumb_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.right() - 64.0, rect.top() + 2.0),
                                egui::pos2(rect.right() - 4.0, rect.bottom() - 2.0),
                            );
                            painter.rect_filled(thumb_rect, 1.0, theme.meter_bg);
                            draw_waveform_thumbnail_browser(&painter, thumb_rect, &sample.data, is_selected, &playback_state.sample_positions_for(i), theme);
                        }

                        if resp.clicked() {
                            result = Some(i as u8);
                            should_close = true;
                        }

                        eguidev::track_response_full(
                            format!("inst.browser.row.{}", i),
                            &resp,
                            eguidev::WidgetMeta {
                                role: eguidev::WidgetRole::Button,
                                label: Some(name),
                                visible: ui.is_visible() && ui.is_rect_visible(rect),
                                ..Default::default()
                            },
                        );
                    }
                });

            if !filter.is_empty() {
                ui.add_space(2.0);
                if ui.button("Clear Search").clicked() {
                    filter.clear();
                    ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
                }
            }
        });

    if should_close {
        *open = false;
    }

    result
}

fn draw_waveform_thumbnail_browser(
    painter: &egui::Painter,
    rect: egui::Rect,
    data: &std::sync::Arc<Vec<f32>>,
    is_selected: bool,
    playback_positions: &[f64],
    theme: &TrackerTheme,
) {
    let mid_y = rect.center().y;
    let half_h = (rect.height() - 4.0) / 2.0;
    let width = rect.width() - 4.0;
    let left = rect.left() + 2.0;

    let is_playing = !playback_positions.is_empty();
    let color = if is_playing {
        theme.sample_thumb_playing
    } else if is_selected {
        theme.sample_thumb_selected
    } else {
        theme.sample_thumb_default
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

    for &pos in playback_positions {
        let x_pos = left + (pos as f32 / len as f32) * width;
        if x_pos >= left && x_pos <= left + width {
            painter.line_segment(
                [egui::pos2(x_pos, rect.top()), egui::pos2(x_pos, rect.bottom())],
                egui::Stroke::new(1.0, theme.playback_position_line),
            );
        }
    }
}
