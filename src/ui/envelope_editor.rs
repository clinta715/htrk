use eframe::egui;
use crate::edit::EnvelopeType;
use crate::sequencer::instrument::Envelope;

pub struct EnvelopeResponse {
    pub event: Option<EnvelopeEditEvent>,
    pub hovered_point: Option<usize>,
}

pub enum EnvelopeEditEvent {
    PointMoved(usize, u16, u8),
    PointAdded(u16, u8),
    PointRemoved(usize),
}

pub fn draw_envelope_editor(
    ui: &mut egui::Ui,
    envelope: &Envelope,
    env_type: EnvelopeType,
    playback_positions: &[f32],
) -> EnvelopeResponse {
    let mut event = None;
    let mut hovered_point = None;

    let (line_color, fill_color, label) = match env_type {
        EnvelopeType::Volume => (
            egui::Color32::from_rgb(80, 220, 80),
            egui::Color32::from_rgba_premultiplied(40, 140, 40, 40),
            "Volume",
        ),
        EnvelopeType::Panning => (
            egui::Color32::from_rgb(60, 180, 255),
            egui::Color32::from_rgba_premultiplied(30, 90, 180, 40),
            "Panning",
        ),
        EnvelopeType::Pitch => (
            egui::Color32::from_rgb(255, 180, 60),
            egui::Color32::from_rgba_premultiplied(180, 100, 30, 40),
            "Pitch",
        ),
        EnvelopeType::Filter => (
            egui::Color32::from_rgb(200, 100, 255),
            egui::Color32::from_rgba_premultiplied(120, 50, 160, 40),
            "Filter",
        ),
    };

    let desired_size = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 28));

    if envelope.points.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("No {} envelope points", label),
            egui::FontId::proportional(14.0),
            egui::Color32::GRAY,
        );
        return EnvelopeResponse { event: None, hovered_point: None };
    }

    let max_tick = envelope.points.last().map(|p| p.tick).unwrap_or(1).max(100);
    let max_val = 64.0;

    let to_screen = |tick: u16, val: u8| {
        let x = rect.left() + (tick as f32 / max_tick as f32) * rect.width();
        let y = rect.bottom() - (val as f32 / max_val) * rect.height();
        egui::pos2(x, y)
    };

    let from_screen = |pos: egui::Pos2| {
        let tick = ((pos.x - rect.left()) / rect.width() * max_tick as f32).clamp(0.0, max_tick as f32) as u16;
        let val = ((rect.bottom() - pos.y) / rect.height() * max_val).clamp(0.0, max_val) as u8;
        (tick, val)
    };

    // Draw grid
    for i in 0..=4 {
        let y = rect.bottom() - (i as f32 / 4.0) * rect.height();
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
        );
    }
    for i in 0..=5 {
        let x = rect.left() + (i as f32 / 5.0) * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
        );
    }

    // Build polyline points for fill and lines
    let mut line_pts: Vec<egui::Pos2> = Vec::new();
    for p in envelope.points.iter() {
        line_pts.push(to_screen(p.tick, p.value));
    }

    // Draw filled area under the curve
    if line_pts.len() >= 2 {
        let mut fill_pts = Vec::new();
        fill_pts.push(egui::pos2(line_pts[0].x, rect.bottom()));
        fill_pts.extend_from_slice(&line_pts);
        fill_pts.push(egui::pos2(line_pts.last().unwrap().x, rect.bottom()));
        painter.add(egui::Shape::convex_polygon(fill_pts, fill_color, egui::Stroke::NONE));
    }

    // Draw lines between points
    for i in 0..envelope.points.len().saturating_sub(1) {
        let p1 = envelope.points[i];
        let p2 = envelope.points[i + 1];
        let (x1, y1) = (to_screen(p1.tick, p1.value).x, to_screen(p1.tick, p1.value).y);
        let (x2, y2) = (to_screen(p2.tick, p2.value).x, to_screen(p2.tick, p2.value).y);
        painter.line_segment(
            [egui::pos2(x1, y1), egui::pos2(x2, y2)],
            egui::Stroke::new(2.5, line_color),
        );
    }

    for &pos in playback_positions {
        let x = rect.left() + (pos / max_tick as f32) * rect.width();
        if x < rect.left() || x > rect.right() { continue; }

        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.5, egui::Color32::from_rgba_premultiplied(255, 255, 100, 140)),
        );

        if envelope.points.len() >= 2 {
            let mut interp_val = envelope.points[0].value;
            for i in 0..envelope.points.len().saturating_sub(1) {
                let p1 = envelope.points[i];
                let p2 = envelope.points[i + 1];
                if pos >= p1.tick as f32 && pos <= p2.tick as f32 {
                    let t = if p2.tick > p1.tick {
                        (pos - p1.tick as f32) / (p2.tick - p1.tick) as f32
                    } else {
                        0.0
                    };
                    interp_val = (p1.value as f32 + (p2.value as f32 - p1.value as f32) * t).round() as u8;
                    break;
                }
            }
            let dot_pos = to_screen(pos.max(0.0) as u16, interp_val);
            painter.circle_filled(dot_pos, 4.0, egui::Color32::from_rgba_premultiplied(255, 255, 100, 220));
        }
    }

    // Determine hovered point
    let mouse_pos = response.hover_pos();
    if let Some(mpos) = mouse_pos {
        for (i, p) in envelope.points.iter().enumerate() {
            let center = to_screen(p.tick, p.value);
            let dist = (mpos - center).length();
            if dist < 8.0 {
                hovered_point = Some(i);
                break;
            }
        }
    }

    // Draw loop markers (dashed cyan lines)
    if envelope.flags.loop_ {
        if let Some(ls) = envelope.loop_start {
            if ls < envelope.points.len() {
                let lx = to_screen(envelope.points[ls].tick, 0).x;
                painter.line_segment(
                    [egui::pos2(lx, rect.top()), egui::pos2(lx, rect.bottom())],
                    egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(0, 200, 255, 120)),
                );
                painter.circle_stroke(egui::pos2(lx, rect.top() + 8.0), 4.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 200, 255)));
            }
        }
        if let Some(le) = envelope.loop_end {
            if le < envelope.points.len() {
                let lx = to_screen(envelope.points[le].tick, 0).x;
                painter.line_segment(
                    [egui::pos2(lx, rect.top()), egui::pos2(lx, rect.bottom())],
                    egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(0, 200, 255, 120)),
                );
                painter.circle_stroke(egui::pos2(lx, rect.bottom() - 8.0), 4.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 200, 255)));
            }
        }
    }

    // Draw points and handle interaction
    for (i, p) in envelope.points.iter().enumerate() {
        let center = to_screen(p.tick, p.value);
        let is_hovered = hovered_point == Some(i);
        let is_sustain = envelope.sustain_point == Some(i);

        let color = if is_hovered {
            egui::Color32::WHITE
        } else if is_sustain {
            egui::Color32::from_rgb(0, 255, 128)
        } else {
            line_color
        };

        // Draw label near point
        if is_hovered || is_sustain {
            let label_pos = egui::pos2(center.x + 10.0, center.y - 10.0).clamp(rect.left_top() + egui::vec2(4.0, 4.0), rect.right_bottom() - egui::vec2(50.0, 4.0));
            painter.text(
                label_pos,
                egui::Align2::LEFT_TOP,
                format!("{}:{}", p.tick, p.value),
                egui::FontId::monospace(10.0),
                color,
            );
        }

        if is_sustain {
            // Diamond shape for sustain point
            let d = 5.0;
            let pts = vec![
                egui::pos2(center.x, center.y - d),
                egui::pos2(center.x + d, center.y),
                egui::pos2(center.x, center.y + d),
                egui::pos2(center.x - d, center.y),
            ];
            painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
        } else {
            painter.circle_filled(center, 4.0, color);
        }

        // Drag to move points
        if response.dragged_by(egui::PointerButton::Primary) && is_hovered {
            let (new_tick, new_val) = from_screen(ui.input(|i| i.pointer.hover_pos().unwrap_or(center)));
            event = Some(EnvelopeEditEvent::PointMoved(i, new_tick, new_val));
        }
    }

    // Right-click to delete point
    if response.clicked_by(egui::PointerButton::Secondary) {
        if let Some(idx) = hovered_point {
            event = Some(EnvelopeEditEvent::PointRemoved(idx));
        }
    }

    // Double-click to add point
    if response.double_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (tick, val) = from_screen(pos);
            event = Some(EnvelopeEditEvent::PointAdded(tick, val));
        }
    }

    EnvelopeResponse { event, hovered_point }
}
