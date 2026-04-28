use eframe::egui;
use std::sync::Arc;

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
        return None;
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 20));

    let len = data.len();
    let width = rect.width();
    let height = rect.height();
    let middle_y = rect.top() + height / 2.0;

    let points_per_pixel = (len as f32 / width).max(1.0);

    // Draw selection highlight first (behind waveform lines)
    if let Some((ref sel_start, ref sel_end)) = *selection {
        let s = (*sel_start).min(*sel_end).min(len);
        let e = (*sel_end).max(*sel_start).min(len);
        if s < e {
            let sel_left = rect.left() + (s as f32 / len as f32) * width;
            let sel_right = rect.left() + (e as f32 / len as f32) * width;
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

    // Simplistic rendering: just draw min/max for each pixel column
    for x in 0..(width as usize) {
        let start_idx = (x as f32 * points_per_pixel) as usize;
        let end_idx = ((x + 1) as f32 * points_per_pixel) as usize;
        let end_idx = end_idx.min(len);

        if start_idx >= len { break; }

        let mut min = 1.0;
        let mut max = -1.0;

        for i in start_idx..end_idx {
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

    // Loop markers and region selection
    if has_loop {
        let start_x = rect.left() + (loop_start as f32 / len as f32) * width;
        let end_x = rect.left() + (loop_end as f32 / len as f32) * width;

        let marker_id = ui.make_persistent_id("waveform_marker_drag");
        let sel_id = ui.make_persistent_id("waveform_sel_drag");
        let mut dragging_marker = ui.data_mut(|d| d.get_temp::<Option<usize>>(marker_id).flatten());
        let mut selecting = ui.data_mut(|d| d.get_temp::<bool>(sel_id)).unwrap_or(false);

        if response.drag_started() {
            let mouse_pos = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);
            let dist_start = (mouse_pos.x - start_x).abs();
            let dist_end = (mouse_pos.x - end_x).abs();

            if dist_start < 10.0 || dist_end < 10.0 {
                dragging_marker = if dist_start < dist_end { Some(0) } else { Some(1) };
                selecting = false;
            } else {
                let norm = ((mouse_pos.x - rect.left()) / width).clamp(0.0, 1.0);
                *selection = Some(((norm * len as f32) as usize, (norm * len as f32) as usize));
                selecting = true;
            }
            ui.data_mut(|d| d.insert_temp(marker_id, dragging_marker));
            ui.data_mut(|d| d.insert_temp(sel_id, selecting));
        }

        if response.dragged() {
            if let Some(marker) = dragging_marker {
                let mouse_pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(egui::Pos2::ZERO);
                let norm = ((mouse_pos.x - rect.left()) / width).clamp(0.0, 1.0);
                let new_pos = (norm * len as f32) as usize;
                if marker == 0 && new_pos < loop_end {
                    event = Some(WaveformEvent::LoopStartChanged(new_pos));
                } else if marker == 1 && new_pos > loop_start {
                    event = Some(WaveformEvent::LoopEndChanged(new_pos));
                }
            } else if selecting {
                let mouse_pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(egui::Pos2::ZERO);
                let norm = ((mouse_pos.x - rect.left()) / width).clamp(0.0, 1.0);
                let pos = (norm * len as f32) as usize;
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

        painter.line_segment(
            [egui::pos2(start_x, rect.top()), egui::pos2(start_x, rect.bottom())],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 255, 0)),
        );
        painter.line_segment(
            [egui::pos2(end_x, rect.top()), egui::pos2(end_x, rect.bottom())],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 255, 0)),
        );

        painter.circle_filled(egui::pos2(start_x, rect.top() + 10.0), 5.0, egui::Color32::YELLOW);
        painter.circle_filled(egui::pos2(end_x, rect.bottom() - 10.0), 5.0, egui::Color32::YELLOW);
    } else {
        let sel_id = ui.make_persistent_id("waveform_sel_drag");
        let mut selecting = ui.data_mut(|d| d.get_temp::<bool>(sel_id)).unwrap_or(false);

        if response.drag_started() {
            let mouse_pos = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);
            let norm = ((mouse_pos.x - rect.left()) / width).clamp(0.0, 1.0);
            let pos = (norm * len as f32) as usize;
            *selection = Some((pos, pos));
            selecting = true;
            ui.data_mut(|d| d.insert_temp(sel_id, selecting));
        }

        if response.dragged() && selecting {
            let mouse_pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(egui::Pos2::ZERO);
            let norm = ((mouse_pos.x - rect.left()) / width).clamp(0.0, 1.0);
            let pos = (norm * len as f32) as usize;
            if let Some((start, _)) = *selection {
                *selection = Some((start, pos));
            }
        }

        if response.drag_stopped() {
            selecting = false;
            ui.data_mut(|d| d.insert_temp::<bool>(sel_id, false));
            if let Some((ref s, ref e)) = *selection {
                if s == e { *selection = None; }
                else { *selection = Some(((*s).min(*e), (*s).max(*e))); }
            }
        }
    }

    painter.line_segment(
        [egui::pos2(rect.left(), middle_y), egui::pos2(rect.right(), middle_y)],
        egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
    );

    event
}
