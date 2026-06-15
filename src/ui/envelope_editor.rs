use eframe::egui;
use crate::edit::EnvelopeType;
use crate::sequencer::instrument::Envelope;
use crate::ui::TrackerTheme;

pub struct EnvelopeResponse {
    pub event: Option<EnvelopeEditEvent>,
    pub hovered_point: Option<usize>,
}

pub enum EnvelopeEditEvent {
    PointMoved(usize, u16, u8),
    PointAdded(u16, u8),
    PointRemoved(usize),
}

fn gradient_fill_polygon(painter: &egui::Painter, pts: &[egui::Pos2], top_color: egui::Color32, bottom_color: egui::Color32) {
    if pts.len() < 3 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    let min_y = pts.iter().map(|p| p.y).fold(f32::MAX, f32::min);
    let max_y = pts.iter().map(|p| p.y).fold(f32::MIN, f32::max);
    let range = (max_y - min_y).max(1.0);

    for &p in pts {
        let t = ((p.y - min_y) / range).clamp(0.0, 1.0);
        let c = egui::Color32::from_rgba_premultiplied(
            (top_color[0] as f32 * (1.0 - t) + bottom_color[0] as f32 * t) as u8,
            (top_color[1] as f32 * (1.0 - t) + bottom_color[1] as f32 * t) as u8,
            (top_color[2] as f32 * (1.0 - t) + bottom_color[2] as f32 * t) as u8,
            (top_color[3] as f32 * (1.0 - t) + bottom_color[3] as f32 * t) as u8,
        );
        mesh.colored_vertex(p, c);
    }
    for i in 1..(pts.len() as u32 - 1) {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
}

pub fn draw_envelope_editor(
    ui: &mut egui::Ui,
    envelope: &Envelope,
    env_type: EnvelopeType,
    playback_positions: &[f32],
    theme: &TrackerTheme,
) -> EnvelopeResponse {
    let mut event = None;
    let mut hovered_point = None;

    let (line_color, fill_color, label) = match env_type {
        EnvelopeType::Volume => (theme.envelope_colors[0].0, theme.envelope_colors[0].1, "Volume"),
        EnvelopeType::Panning => (theme.envelope_colors[1].0, theme.envelope_colors[1].1, "Panning"),
        EnvelopeType::Pitch => (theme.envelope_colors[2].0, theme.envelope_colors[2].1, "Pitch"),
        EnvelopeType::Filter => (theme.envelope_colors[3].0, theme.envelope_colors[3].1, "Filter"),
    };

    let desired_size = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

    let painter = ui.painter_at(rect);
    #[allow(deprecated)]
    let rounding = egui::Rounding::same(4);
    painter.rect(rect, rounding, theme.panel_bg, egui::Stroke::new(1.0, theme.panel_border), egui::epaint::StrokeKind::Inside);

    if envelope.points.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("No {} envelope points", label),
            egui::FontId::proportional(14.0),
            theme.fg_dim,
        );
        return EnvelopeResponse { event: None, hovered_point: None };
    }

    let max_tick = envelope.points.last().map(|p| p.tick).unwrap_or(1).max(100);
    let max_val = 64.0;

    let inner = rect.shrink(2.0);

    let to_screen = |tick: u16, val: u8| {
        let x = inner.left() + (tick as f32 / max_tick as f32) * inner.width();
        let y = inner.bottom() - (val as f32 / max_val) * inner.height();
        egui::pos2(x, y)
    };

    let from_screen = |pos: egui::Pos2| {
        let tick = ((pos.x - inner.left()) / inner.width() * max_tick as f32).clamp(0.0, max_tick as f32) as u16;
        let val = ((inner.bottom() - pos.y) / inner.height() * max_val).clamp(0.0, max_val) as u8;
        (tick, val)
    };

    // Draw grid (subtle)
    let grid_col = theme.grid_line.gamma_multiply(0.6);
    let grid_minor_col = theme.grid_line_minor.gamma_multiply(0.6);
    for i in 0..=4 {
        let y = inner.bottom() - (i as f32 / 4.0) * inner.height();
        painter.line_segment(
            [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
            egui::Stroke::new(1.0, grid_col),
        );
    }
    for i in 0..=5 {
        let x = inner.left() + (i as f32 / 5.0) * inner.width();
        painter.line_segment(
            [egui::pos2(x, inner.top()), egui::pos2(x, inner.bottom())],
            egui::Stroke::new(1.0, grid_minor_col),
        );
    }

    // Build polyline points
    let mut line_pts: Vec<egui::Pos2> = Vec::new();
    for p in envelope.points.iter() {
        line_pts.push(to_screen(p.tick, p.value));
    }

    // Gradient fill under curve
    if line_pts.len() >= 2 {
        let mut fill_pts = Vec::new();
        fill_pts.push(egui::pos2(line_pts[0].x, inner.bottom()));
        fill_pts.extend_from_slice(&line_pts);
        fill_pts.push(egui::pos2(line_pts.last().unwrap().x, inner.bottom()));
        let fill_start = line_color.linear_multiply(0.25);
        let fill_end = fill_color.gamma_multiply(0.5);
        gradient_fill_polygon(&painter, &fill_pts, fill_start, fill_end);
    }

    // Subtle bottom shadow for depth
    {
        let shadow_top = inner.bottom() - inner.height() * 0.4;
        let shadow_rect = egui::Rect::from_min_max(
            egui::pos2(inner.left(), shadow_top),
            egui::pos2(inner.right(), inner.bottom()),
        );
        let mut shadow_mesh = egui::Mesh::default();
        let trans = egui::Color32::TRANSPARENT;
        let dark = egui::Color32::from_black_alpha(16);
        shadow_mesh.colored_vertex(shadow_rect.left_top(), trans);
        shadow_mesh.colored_vertex(shadow_rect.right_top(), trans);
        shadow_mesh.colored_vertex(shadow_rect.right_bottom(), dark);
        shadow_mesh.colored_vertex(shadow_rect.left_bottom(), dark);
        shadow_mesh.add_triangle(0, 1, 2);
        shadow_mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(shadow_mesh));
    }

    // Draw envelope line
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

    // Playback position
    for &pos in playback_positions {
        let x = inner.left() + (pos / max_tick as f32) * inner.width();
        if x < inner.left() || x > inner.right() { continue; }

        painter.line_segment(
            [egui::pos2(x, inner.top()), egui::pos2(x, inner.bottom())],
            egui::Stroke::new(1.5, theme.playback_position_line),
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
            painter.circle_filled(dot_pos, 4.0, theme.playback_position_dot);
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

    // Draw loop markers
    if envelope.flags.loop_ {
        if let Some(ls) = envelope.loop_start {
            if ls < envelope.points.len() {
                let lx = to_screen(envelope.points[ls].tick, 0).x;
                painter.line_segment(
                    [egui::pos2(lx, inner.top()), egui::pos2(lx, inner.bottom())],
                    egui::Stroke::new(1.0, theme.loop_marker),
                );
                painter.circle_stroke(egui::pos2(lx, inner.top() + 8.0), 4.0, egui::Stroke::new(1.5, theme.loop_marker));
            }
        }
        if let Some(le) = envelope.loop_end {
            if le < envelope.points.len() {
                let lx = to_screen(envelope.points[le].tick, 0).x;
                painter.line_segment(
                    [egui::pos2(lx, inner.top()), egui::pos2(lx, inner.bottom())],
                    egui::Stroke::new(1.0, theme.loop_marker),
                );
                painter.circle_stroke(egui::pos2(lx, inner.bottom() - 8.0), 4.0, egui::Stroke::new(1.5, theme.loop_marker));
            }
        }
    }

    // Draw points and handle interaction
    for (i, p) in envelope.points.iter().enumerate() {
        let center = to_screen(p.tick, p.value);
        let is_hovered = hovered_point == Some(i);
        let is_sustain = envelope.sustain_point == Some(i);

        let color = if is_hovered {
            theme.fg_text
        } else if is_sustain {
            theme.fg_volume
        } else {
            line_color
        };

        // Hover glow
        if is_hovered {
            painter.circle_filled(center, 10.0, color.linear_multiply(0.15));
        }

        // Label
        if is_hovered || is_sustain {
            let label_pos = egui::pos2(center.x + 10.0, center.y - 10.0).clamp(
                inner.left_top() + egui::vec2(4.0, 4.0),
                inner.right_bottom() - egui::vec2(50.0, 4.0),
            );
            painter.text(
                label_pos,
                egui::Align2::LEFT_TOP,
                format!("{}:{}", p.tick, p.value),
                egui::FontId::monospace(10.0),
                color,
            );
        }

        if is_sustain {
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

        // Drag
        if response.dragged_by(egui::PointerButton::Primary) && is_hovered {
            let (new_tick, new_val) = from_screen(ui.input(|i| i.pointer.hover_pos().unwrap_or(center)));
            event = Some(EnvelopeEditEvent::PointMoved(i, new_tick, new_val));
        }
    }

    // Right-click delete
    if response.clicked_by(egui::PointerButton::Secondary) {
        if let Some(idx) = hovered_point {
            event = Some(EnvelopeEditEvent::PointRemoved(idx));
        }
    }

    // Double-click add
    if response.double_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let (tick, val) = from_screen(pos);
            event = Some(EnvelopeEditEvent::PointAdded(tick, val));
        }
    }

    EnvelopeResponse { event, hovered_point }
}
