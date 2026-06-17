use eframe::egui;
use std::sync::Arc;

use crate::ui::TrackerTheme;

pub enum WaveformEvent {
    LoopStartChanged(usize),
    LoopEndChanged(usize),
}

pub fn draw_waveform(
    ui: &mut egui::Ui,
    data: &Arc<Vec<f32>>,
    loop_start: usize,
    loop_end: usize,
    has_loop: bool,
    selection: &mut Option<(usize, usize)>,
    sample_index: usize,
    playback_positions: &[f64],
    theme: &TrackerTheme,
    cursor_pos: &mut Option<usize>,
    zoom: &mut f32,
    scroll_offset: &mut f32,
) -> Option<WaveformEvent> {
    let mut event = None;
    let desired_size = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::drag());

    if data.is_empty() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No data",
            egui::FontId::proportional(14.0),
            ui.visuals().text_color(),
        );
        *cursor_pos = None;
        return None;
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 20));

    let len = data.len();
    let width = rect.width();
    let height = rect.height();
    let middle_y = rect.top() + height / 2.0;

    // Compute visible range based on zoom
    // zoom == 0 or >= len means fit-to-view (see entire sample)
    *zoom = zoom.clamp(0.0, len as f32);
    let visible_samples = if *zoom <= 0.0 || *zoom >= len as f32 {
        *zoom = 0.0;
        len
    } else {
        *zoom as usize
    };

    *scroll_offset = scroll_offset.clamp(0.0, 1.0);
    let max_scroll_offset = if visible_samples >= len { 0.0 } else { 1.0 };
    if *scroll_offset > max_scroll_offset {
        *scroll_offset = max_scroll_offset;
    }
    let start_sample = if visible_samples >= len {
        0
    } else {
        (*scroll_offset * (len - visible_samples) as f32) as usize
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
        *zoom = (old_visible * factor).clamp(8.0, len as f32);
        // Keep cursor position stable when zooming with mouse
        if let Some(hover_pos) = response.hover_pos() {
            let hover_ratio = (hover_pos.x - rect.left()) / width;
            let cursor_sample = start_sample + (hover_ratio * visible_samples as f32) as usize;
            let new_visible = *zoom as usize;
            let new_scroll = cursor_sample as f32 - hover_ratio * new_visible as f32;
            let max_s = (len - new_visible) as f32;
            *scroll_offset = if max_s > 0.0 { (new_scroll / max_s).clamp(0.0, 1.0) } else { 0.0 };
        }
    }

    // Track cursor position on hover
    *cursor_pos = response.hover_pos().map(|pos| {
        x_to_sample(pos.x)
    });

    // Draw center line
    painter.line_segment(
        [egui::pos2(rect.left(), middle_y), egui::pos2(rect.right(), middle_y)],
        egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
    );

    // Draw selection highlight
    if let Some((ref sel_start, ref sel_end)) = *selection {
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
                egui::Color32::from_rgba_premultiplied(40, 80, 200, 60),
            );
        }
    }

    // Draw waveform: min/max per pixel column
    let pixels_available = width as usize;
    for x in 0..pixels_available {
        let idx_start = start_sample + (x as f32 / pixels_available as f32 * visible_samples as f32) as usize;
        let idx_end = start_sample + ((x + 1) as f32 / pixels_available as f32 * visible_samples as f32) as usize;
        let idx_end = idx_end.min(end_sample);

        if idx_start >= end_sample { break; }

        let mut min = 1.0f32;
        let mut max = -1.0f32;

        for i in idx_start..idx_end {
            let val = data[i];
            if val < min { min = val; }
            if val > max { max = val; }
        }

        let x_pos = rect.left() + x as f32;
        painter.line_segment(
            [
                egui::pos2(x_pos, middle_y - max * (height / 2.0)),
                egui::pos2(x_pos, middle_y - min * (height / 2.0)),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 200, 0)),
        );
    }

    // Draw playback position lines
    for &pos in playback_positions {
        if pos >= start_sample as f64 && pos <= end_sample as f64 {
            let x_pos = sample_to_x(pos as usize);
            if x_pos >= rect.left() && x_pos <= rect.right() {
                painter.line_segment(
                    [egui::pos2(x_pos, rect.top()), egui::pos2(x_pos, rect.bottom())],
                    egui::Stroke::new(2.0, theme.playback_position_line),
                );
            }
        }
    }

    // Interaction: loop markers and region selection
    if has_loop {
        let start_x = sample_to_x(loop_start.min(end_sample).max(start_sample));
        let end_x = sample_to_x(loop_end.min(end_sample).max(start_sample));

        let marker_id = ui.make_persistent_id(format!("waveform_marker_drag_{}", sample_index));
        let sel_id = ui.make_persistent_id(format!("waveform_sel_drag_{}", sample_index));
        let mut dragging_marker = ui.data_mut(|d| d.get_temp::<Option<usize>>(marker_id).flatten());
        let mut selecting = ui.data_mut(|d| d.get_temp::<bool>(sel_id)).unwrap_or(false);

        if response.drag_started() {
            let mouse_pos = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);
            let dist_start = (mouse_pos.x - start_x).abs();
            let dist_end = (mouse_pos.x - end_x).abs();

            // Check for shift+drag for scrolling
            if ui.input(|i| i.modifiers.shift) {
                // Shift+drag = scroll, not select
                dragging_marker = None;
                selecting = false;
            } else if dist_start < 10.0 || dist_end < 10.0 {
                dragging_marker = if dist_start < dist_end { Some(0) } else { Some(1) };
                selecting = false;
            } else {
                let pos = x_to_sample(mouse_pos.x);
                *selection = Some((pos, pos));
                selecting = true;
            }
            ui.data_mut(|d| d.insert_temp(marker_id, dragging_marker));
            ui.data_mut(|d| d.insert_temp(sel_id, selecting));
        }

        if response.dragged() {
            if let Some(marker) = dragging_marker {
                let mouse_pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(egui::Pos2::ZERO);
                let new_pos = x_to_sample(mouse_pos.x);
                if marker == 0 && new_pos < loop_end {
                    event = Some(WaveformEvent::LoopStartChanged(new_pos));
                } else if marker == 1 && new_pos > loop_start {
                    event = Some(WaveformEvent::LoopEndChanged(new_pos));
                }
            } else if selecting {
                let mouse_pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(egui::Pos2::ZERO);
                let pos = x_to_sample(mouse_pos.x);
                if let Some((start, _)) = *selection {
                    *selection = Some((start, pos));
                }
            }
        }

        if response.drag_stopped() {
            ui.data_mut(|d| d.insert_temp::<Option<usize>>(marker_id, None));
            ui.data_mut(|d| d.insert_temp::<bool>(sel_id, false));
            if let Some((ref s, ref e)) = *selection {
                if s == e { *selection = None; }
                else { *selection = Some(((*s).min(*e), (*s).max(*e))); }
            }
        }

        // Only draw loop markers if they're in view
        if loop_start >= start_sample && loop_start <= end_sample {
            painter.line_segment(
                [egui::pos2(start_x, rect.top()), egui::pos2(start_x, rect.bottom())],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 255, 0)),
            );
            painter.circle_filled(egui::pos2(start_x, rect.top() + 10.0), 5.0, egui::Color32::YELLOW);
        }
        if loop_end >= start_sample && loop_end <= end_sample {
            painter.line_segment(
                [egui::pos2(end_x, rect.top()), egui::pos2(end_x, rect.bottom())],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 255, 0)),
            );
            painter.circle_filled(egui::pos2(end_x, rect.bottom() - 10.0), 5.0, egui::Color32::YELLOW);
        }
    } else {
        let sel_id = ui.make_persistent_id(format!("waveform_sel_drag_{}", sample_index));
        let mut selecting = ui.data_mut(|d| d.get_temp::<bool>(sel_id)).unwrap_or(false);

        if response.drag_started() {
            let mouse_pos = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);
            if ui.input(|i| i.modifiers.shift) {
                // Shift+drag = scroll, don't start selection
                selecting = false;
            } else {
                let pos = x_to_sample(mouse_pos.x);
                *selection = Some((pos, pos));
                selecting = true;
            }
            ui.data_mut(|d| d.insert_temp(sel_id, selecting));
        }

        if response.dragged() && selecting {
            let mouse_pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(egui::Pos2::ZERO);
            let pos = x_to_sample(mouse_pos.x);
            if let Some((start, _)) = *selection {
                *selection = Some((start, pos));
            }
        }

        if response.drag_stopped() {
            ui.data_mut(|d| d.insert_temp::<bool>(sel_id, false));
            if let Some((ref s, ref e)) = *selection {
                if s == e { *selection = None; }
                else { *selection = Some(((*s).min(*e), (*s).max(*e))); }
            }
        }
    }

    event
}