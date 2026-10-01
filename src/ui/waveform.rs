use eframe::egui;
use std::sync::Arc;

use crate::ui::TrackerTheme;
use crate::ui::sample_editor_panel::SampleEditor;

pub enum WaveformEvent {
    LoopStartChanged(usize),
    LoopEndChanged(usize),
    CutSelection,
    CopySelection,
    PasteAtCursor,
    CropToSelection,
    SilenceSelection,
    Normalize,
    Reverse,
    SetLoopFromSelection,
    TrimSilence,
    FadeInSelection,
    FadeOutSelection,
    ZoomToSelection,
    ZoomFit,
}

pub fn draw_waveform(
    ui: &mut egui::Ui,
    data: &Arc<Vec<f32>>,
    loop_start: usize,
    loop_end: usize,
    has_loop: bool,
    _sample_index: usize,
    playback_positions: &[f64],
    theme: &TrackerTheme,
    editor: &mut SampleEditor,
    clipboard_available: bool,
) -> Option<WaveformEvent> {
    let mut event = None;
    let desired_size = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(
        desired_size,
        egui::Sense::drag().union(egui::Sense::click()),
    );

    if data.is_empty() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No data",
            egui::FontId::proportional(14.0),
            ui.visuals().text_color(),
        );
        editor.cursor_pos = None;
        return None;
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.meter_bg);

    let len = data.len();
    let width = rect.width();
    let height = rect.height();
    let middle_y = rect.top() + height / 2.0;

    // Compute visible range based on zoom
    // zoom == 0 or >= len means fit-to-view (see entire sample)
    editor.zoom = editor.zoom.clamp(0.0, len as f32);
    let visible_samples = if editor.zoom <= 0.0 || editor.zoom >= len as f32 {
        editor.zoom = 0.0;
        len
    } else {
        editor.zoom as usize
    };

    editor.scroll_offset = editor.scroll_offset.clamp(0.0, 1.0);
    let max_scroll_offset = if visible_samples >= len { 0.0 } else { 1.0 };
    if editor.scroll_offset > max_scroll_offset {
        editor.scroll_offset = max_scroll_offset;
    }
    let start_sample = if visible_samples >= len {
        0
    } else {
        (editor.scroll_offset * (len - visible_samples) as f32) as usize
    };
    let end_sample = (start_sample + visible_samples).min(len);

    // Helper: convert sample index to x position
    let sample_to_x = |idx: usize| -> f32 {
        rect.left() + ((idx - start_sample) as f32 / visible_samples as f32) * width
    };
    let x_to_sample = |x: f32| -> usize {
        let ratio = ((x - rect.left()) / width).clamp(0.0, 1.0);
        (start_sample + (ratio * visible_samples as f32) as usize).min(len - 1)
    };

    // Mouse wheel zoom
    let scroll_delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);
    if scroll_delta != 0.0 && response.hovered() {
        let old_visible = visible_samples as f32;
        let factor = 1.0 - scroll_delta * 0.003;
        editor.zoom = (old_visible * factor).clamp(8.0, len as f32);
        // Keep cursor position stable when zooming with mouse
        if let Some(hover_pos) = response.hover_pos() {
            let hover_ratio = (hover_pos.x - rect.left()) / width;
            let cursor_sample = start_sample + (hover_ratio * visible_samples as f32) as usize;
            let new_visible = editor.zoom as usize;
            let new_scroll = cursor_sample as f32 - hover_ratio * new_visible as f32;
            let max_s = (len - new_visible) as f32;
            editor.scroll_offset = if max_s > 0.0 {
                (new_scroll / max_s).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
    }

    // Track cursor position on hover
    editor.cursor_pos = response.hover_pos().map(|pos| x_to_sample(pos.x));

    // Draw center line
    painter.line_segment(
        [
            egui::pos2(rect.left(), middle_y),
            egui::pos2(rect.right(), middle_y),
        ],
        egui::Stroke::new(1.0_f32, theme.grid_line),
    );

    // Draw selection highlight
    if let Some((ref sel_start, ref sel_end)) = editor.selection {
        let s = (*sel_start).min(*sel_end).min(len);
        let e = (*sel_end).max(*sel_start).min(len);
        if s < e && s < end_sample && e > start_sample {
            let vis_s = s.max(start_sample);
            let vis_e = e.min(end_sample);
            let sel_left = sample_to_x(vis_s);
            let sel_right = sample_to_x(vis_e);
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(sel_left, rect.top()),
                    egui::pos2(sel_right, rect.bottom()),
                ),
                0.0,
                egui::Color32::from_rgba_premultiplied(
                    theme.fg_instrument.r(),
                    theme.fg_instrument.g(),
                    theme.fg_instrument.b(),
                    60,
                ),
            );
        }
    }

    // Draw waveform: min/max per pixel column
    let pixels_available = width as usize;
    for x in 0..pixels_available {
        let idx_start =
            start_sample + (x as f32 / pixels_available as f32 * visible_samples as f32) as usize;
        let idx_end = start_sample
            + ((x + 1) as f32 / pixels_available as f32 * visible_samples as f32) as usize;
        let idx_end = idx_end.min(end_sample);

        if idx_start >= end_sample {
            break;
        }

        let mut min = 1.0f32;
        let mut max = -1.0f32;

        for i in idx_start..idx_end {
            let val = data[i];
            if val < min {
                min = val;
            }
            if val > max {
                max = val;
            }
        }

        let x_pos = rect.left() + x as f32;
        painter.line_segment(
            [
                egui::pos2(x_pos, middle_y - max * (height / 2.0)),
                egui::pos2(x_pos, middle_y - min * (height / 2.0)),
            ],
            egui::Stroke::new(1.0_f32, theme.fg_volume),
        );
    }

    // Draw playback position lines
    for &pos in playback_positions {
        if pos >= start_sample as f64 && pos <= end_sample as f64 {
            let x_pos = sample_to_x(pos as usize);
            if x_pos >= rect.left() && x_pos <= rect.right() {
                painter.line_segment(
                    [
                        egui::pos2(x_pos, rect.top()),
                        egui::pos2(x_pos, rect.bottom()),
                    ],
                    egui::Stroke::new(2.0_f32, theme.playback_position_line),
                );
            }
        }
    }

    // Interaction: loop markers and region selection.
    // Drag state lives on `SampleEditor` (typed, P3) and resets on sample
    // switch at the `draw_sample_editor` call site.
    if has_loop {
        let start_x = sample_to_x(loop_start.min(end_sample).max(start_sample));
        let end_x = sample_to_x(loop_end.min(end_sample).max(start_sample));

        if response.drag_started() {
            let mouse_pos = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);
            let dist_start = (mouse_pos.x - start_x).abs();
            let dist_end = (mouse_pos.x - end_x).abs();

            // Check for shift+drag for scrolling
            if ui.input(|i| i.modifiers.shift) {
                // Shift+drag = scroll, not select
                editor.dragging_marker = None;
                editor.selecting = false;
            } else if dist_start < 10.0 || dist_end < 10.0 {
                editor.dragging_marker = if dist_start < dist_end {
                    Some(0)
                } else {
                    Some(1)
                };
                editor.selecting = false;
            } else {
                let pos = x_to_sample(mouse_pos.x);
                editor.selection = Some((pos, pos));
                editor.selecting = true;
            }
        }

        if response.dragged() {
            if let Some(marker) = editor.dragging_marker {
                let mouse_pos = ui
                    .input(|i| i.pointer.interact_pos())
                    .unwrap_or(egui::Pos2::ZERO);
                let new_pos = x_to_sample(mouse_pos.x);
                if marker == 0 && new_pos < loop_end {
                    event = Some(WaveformEvent::LoopStartChanged(new_pos));
                } else if marker == 1 && new_pos > loop_start {
                    event = Some(WaveformEvent::LoopEndChanged(new_pos));
                }
            } else if editor.selecting {
                let mouse_pos = ui
                    .input(|i| i.pointer.interact_pos())
                    .unwrap_or(egui::Pos2::ZERO);
                let pos = x_to_sample(mouse_pos.x);
                if let Some((start, _)) = editor.selection {
                    editor.selection = Some((start, pos));
                }
            }
        }

        if response.drag_stopped() {
            editor.dragging_marker = None;
            editor.selecting = false;
            if let Some((s, e)) = editor.selection {
                if s == e {
                    editor.selection = None;
                } else {
                    editor.selection = Some((s.min(e), s.max(e)));
                }
            }
        }

        // Only draw loop markers if they're in view
        if loop_start >= start_sample && loop_start <= end_sample {
            painter.line_segment(
                [
                    egui::pos2(start_x, rect.top()),
                    egui::pos2(start_x, rect.bottom()),
                ],
                egui::Stroke::new(2.0_f32, theme.cursor_outline),
            );
            painter.circle_filled(
                egui::pos2(start_x, rect.top() + 10.0),
                5.0,
                theme.cursor_outline,
            );
        }
        if loop_end >= start_sample && loop_end <= end_sample {
            painter.line_segment(
                [
                    egui::pos2(end_x, rect.top()),
                    egui::pos2(end_x, rect.bottom()),
                ],
                egui::Stroke::new(2.0_f32, theme.cursor_outline),
            );
            painter.circle_filled(
                egui::pos2(end_x, rect.bottom() - 10.0),
                5.0,
                theme.cursor_outline,
            );
        }
    } else {
        if response.drag_started() {
            let mouse_pos = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);
            if ui.input(|i| i.modifiers.shift) {
                // Shift+drag = scroll, don't start selection
                editor.selecting = false;
            } else {
                let pos = x_to_sample(mouse_pos.x);
                editor.selection = Some((pos, pos));
                editor.selecting = true;
            }
        }

        if response.dragged() && editor.selecting {
            let mouse_pos = ui
                .input(|i| i.pointer.interact_pos())
                .unwrap_or(egui::Pos2::ZERO);
            let pos = x_to_sample(mouse_pos.x);
            if let Some((start, _)) = editor.selection {
                editor.selection = Some((start, pos));
            }
        }

        if response.drag_stopped() {
            editor.selecting = false;
            if let Some((s, e)) = editor.selection {
                if s == e {
                    editor.selection = None;
                } else {
                    editor.selection = Some((s.min(e), s.max(e)));
                }
            }
        }
    }

    // Right-click context menu
    response.context_menu(|ui| {
        ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
        let has_sel = editor.selection.is_some();
        if ui.add_enabled(has_sel, egui::Button::new("Cut")).clicked() {
            event = Some(WaveformEvent::CutSelection);
            ui.close();
        }
        if ui.add_enabled(has_sel, egui::Button::new("Copy")).clicked() {
            event = Some(WaveformEvent::CopySelection);
            ui.close();
        }
        if ui
            .add_enabled(clipboard_available, egui::Button::new("Paste"))
            .clicked()
        {
            event = Some(WaveformEvent::PasteAtCursor);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_sel, egui::Button::new("Crop to Selection"))
            .clicked()
        {
            event = Some(WaveformEvent::CropToSelection);
            ui.close();
        }
        if ui
            .add_enabled(has_sel, egui::Button::new("Silence Selection"))
            .clicked()
        {
            event = Some(WaveformEvent::SilenceSelection);
            ui.close();
        }
        ui.separator();
        if ui.button("Normalize").clicked() {
            event = Some(WaveformEvent::Normalize);
            ui.close();
        }
        if ui.button("Reverse").clicked() {
            event = Some(WaveformEvent::Reverse);
            ui.close();
        }
        if ui.button("Trim Silence").clicked() {
            event = Some(WaveformEvent::TrimSilence);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_sel, egui::Button::new("Set Loop from Selection"))
            .clicked()
        {
            event = Some(WaveformEvent::SetLoopFromSelection);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_sel, egui::Button::new("Fade In"))
            .clicked()
        {
            event = Some(WaveformEvent::FadeInSelection);
            ui.close();
        }
        if ui
            .add_enabled(has_sel, egui::Button::new("Fade Out"))
            .clicked()
        {
            event = Some(WaveformEvent::FadeOutSelection);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_sel, egui::Button::new("Zoom to Selection"))
            .clicked()
        {
            event = Some(WaveformEvent::ZoomToSelection);
            ui.close();
        }
        if ui.button("Zoom Fit").clicked() {
            event = Some(WaveformEvent::ZoomFit);
            ui.close();
        }
    });

    event
}
