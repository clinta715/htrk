use eframe::egui;

use crate::sequencer::automation::{
    AutomationPoint, AutomationTarget, AutomationTrack, InterpolationMode,
};
use crate::sequencer::Module;
use super::theme::TrackerTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneDragState {
    None,
    Moving { track_id: u32, point_idx: usize },
}

pub struct AutomationEditorState {
    pub selected_track_id: Option<u32>,
    pub scroll_offset: f32,
    pub drag: LaneDragState,
    pub selected_order: u16,
    pub add_channel: usize,
}

impl Default for AutomationEditorState {
    fn default() -> Self {
        AutomationEditorState {
            selected_track_id: None,
            scroll_offset: 0.0,
            drag: LaneDragState::None,
            selected_order: 0,
            add_channel: 0,
        }
    }
}

pub struct AutomationEditorResponse {
    pub track_added: Option<(AutomationTarget, Option<usize>)>,
    pub track_removed: Option<u32>,
    pub track_toggled: Option<u32>,
    pub point_changed: Option<(u32, AutomationPoint)>,
    pub point_removed: Option<(u32, u16, u16)>,
    pub interp_changed: Option<(u32, InterpolationMode)>,
}

pub fn draw_automation_editor(
    ui: &mut egui::Ui,
    module: &mut Module,
    state: &mut AutomationEditorState,
    theme: &TrackerTheme,
) -> AutomationEditorResponse {
    let mut resp = AutomationEditorResponse {
        track_added: None,
        track_removed: None,
        track_toggled: None,
        point_changed: None,
        point_removed: None,
        interp_changed: None,
    };

    let sidebar_width = 200.0;

    ui.horizontal_top(|ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(sidebar_width);
            ui.set_max_width(sidebar_width);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Automation Tracks").strong().size(13.0));
                ui.add_space(4.0);

                let track_ids: Vec<u32> = module.automation_tracks.iter().map(|t| t.id).collect();
                for &tid in &track_ids {
                    let track_idx = module.automation_tracks.iter().position(|t| t.id == tid);
                    let track = match track_idx {
                        Some(i) => &module.automation_tracks[i],
                        None => continue,
                    };
                    let label = track.label();
                    let is_selected = state.selected_track_id == Some(tid);
                    let fg = if is_selected {
                        theme.order_selected
                    } else {
                        theme.fg_note
                    };
                    let bg = if is_selected {
                        theme.bg_selected
                    } else {
                        theme.status_bg
                    };

                    ui.horizontal(|ui| {
                        let enabled = track.enabled;
                        let cb_size = 14.0;
                        let (cb_rect, cb_resp) = ui.allocate_exact_size(
                            egui::vec2(cb_size, cb_size),
                            egui::Sense::click(),
                        );
                        let cb_icon = if enabled { "\u{2713}" } else { " " };
                        let cb_color = if enabled { theme.vu_green } else { theme.fg_dim };
                        ui.painter().text(
                            cb_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            cb_icon,
                            egui::FontId::monospace(11.0),
                            cb_color,
                        );
                        if cb_resp.clicked() {
                            resp.track_toggled = Some(tid);
                        }

                        let tk_resp = ui.allocate_response(
                            egui::vec2(ui.available_width(), 20.0),
                            egui::Sense::click(),
                        );
                        ui.painter().rect_filled(tk_resp.rect, 2.0, bg);
                        ui.painter().text(
                            egui::pos2(tk_resp.rect.left() + 4.0, tk_resp.rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &label,
                            egui::FontId::monospace(11.0),
                            fg,
                        );
                        if tk_resp.clicked() {
                            state.selected_track_id = Some(tid);
                        }

                        let del_size = 18.0;
                        let del_rect = egui::Rect::from_min_size(
                            egui::pos2(tk_resp.rect.right() - del_size, tk_resp.rect.top()),
                            egui::vec2(del_size, 20.0),
                        );
                        let del_id = ui.id().with("auto_del").with(tid);
                        let del_resp = ui.interact(del_rect, del_id, egui::Sense::click());
                        ui.painter().text(
                            del_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "\u{00d7}",
                            egui::FontId::monospace(13.0),
                            theme.vu_red,
                        );
                        if del_resp.clicked() {
                            resp.track_removed = Some(tid);
                        }
                    });
                }

                ui.add_space(8.0);
                ui.label(egui::RichText::new("+ Add Track").size(11.0).color(theme.fg_instrument));

                ui.add_space(4.0);
                ui.label(egui::RichText::new("Per-Channel:").size(10.0).color(theme.fg_dim));
                ui.horizontal(|ui| {
                    ui.label("Ch:");
                    let max_ch = if module.channel_volume.is_empty() { 0 } else { module.channel_volume.len().saturating_sub(1) };
                    ui.add(egui::DragValue::new(&mut state.add_channel).range(0..=max_ch).speed(1.0));
                });
                for target in AutomationTarget::all_per_channel() {
                    if ui.small_button(target.label()).clicked() {
                        resp.track_added = Some((target, Some(state.add_channel)));
                    }
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Global:").size(10.0).color(theme.fg_dim));
                for target in AutomationTarget::all_global() {
                    if ui.small_button(target.label()).clicked() {
                        resp.track_added = Some((target, None));
                    }
                }
            });
        });

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(400.0, 300.0));
            ui.vertical(|ui| {
                let selected_track = state.selected_track_id.and_then(|tid| {
                    module.automation_tracks.iter().position(|t| t.id == tid)
                });

                match selected_track {
                    Some(idx) => {
                        let track = &module.automation_tracks[idx];
                        draw_lane_editor(ui, track, state, theme, module, &mut resp);
                    }
                    None => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(80.0);
                            ui.label(egui::RichText::new("Select a track from the sidebar").size(14.0).color(theme.fg_dim));
                            ui.label(egui::RichText::new("or click '+ Add Track' to create one").size(12.0).color(theme.fg_dimmer));
                        });
                    }
                }
            });
        });
    });

    resp
}

fn draw_lane_editor(
    ui: &mut egui::Ui,
    track: &AutomationTrack,
    state: &mut AutomationEditorState,
    theme: &TrackerTheme,
    module: &Module,
    resp: &mut AutomationEditorResponse,
) {
    let label = track.label();
    ui.label(egui::RichText::new(&label).strong().size(14.0));

    ui.horizontal(|ui| {
        ui.label("Interp:");
        for mode in [InterpolationMode::Hold, InterpolationMode::Linear, InterpolationMode::Smooth, InterpolationMode::Exponential] {
            let name = match mode {
                InterpolationMode::Hold => "Hold",
                InterpolationMode::Linear => "Lin",
                InterpolationMode::Smooth => "Smooth",
                InterpolationMode::Exponential => "Exp",
            };
            if ui.selectable_label(track.default_interp == mode, name).clicked() {
                resp.interp_changed = Some((track.id, mode));
            }
        }
    });

    let num_rows = module.patterns.first().map_or(64, |p| p.num_rows);
    let row_height = 1.0;
    let total_height = num_rows as f32 * row_height;
    let lane_width = ui.available_width().max(200.0);

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(lane_width, 300.0),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, theme.bg_default);

    for row_idx in (0..num_rows).step_by(4) {
        let y = rect.top() + row_idx as f32 * row_height * (300.0 / total_height.max(1.0));
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(0.5, theme.grid_line),
        );
    }

    let order = state.selected_order;

    if track.points.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Click to add points",
            egui::FontId::monospace(12.0),
            theme.fg_note_empty,
        );
    } else {
        let point_radius = 4.0;
        let curve_color = theme.automation_curve;
        let dim_color = theme.automation_curve_dim;

        for i in 0..track.points.len() {
            let pt = &track.points[i];
            let px = rect.left() + (pt.row as f32 / num_rows as f32) * rect.width();
            let py = rect.bottom() - (pt.value * rect.height());

            if i + 1 < track.points.len() {
                let next = &track.points[i + 1];
                let next_px = rect.left() + (next.row as f32 / num_rows as f32) * rect.width();
                let next_py = rect.bottom() - (next.value * rect.height());

                match pt.interp_to_next {
                    InterpolationMode::Hold => {
                        painter.line_segment(
                            [egui::pos2(px, py), egui::pos2(next_px, py)],
                            egui::Stroke::new(1.5, dim_color),
                        );
                        painter.line_segment(
                            [egui::pos2(next_px, py), egui::pos2(next_px, next_py)],
                            egui::Stroke::new(1.5, dim_color),
                        );
                    }
                    InterpolationMode::Linear => {
                        painter.line_segment(
                            [egui::pos2(px, py), egui::pos2(next_px, next_py)],
                            egui::Stroke::new(1.5, curve_color),
                        );
                    }
                    InterpolationMode::Smooth | InterpolationMode::Exponential => {
                        let steps = ((next_px - px).abs() as usize).max(2);
                        let mut prev = egui::pos2(px, py);
                        for s in 1..=steps {
                            let t = s as f32 / steps as f32;
                            let v = match pt.interp_to_next {
                                InterpolationMode::Smooth => {
                                    pt.value + (next.value - pt.value) * (1.0 - (t * std::f32::consts::PI).cos()) / 2.0
                                }
                                InterpolationMode::Exponential => {
                                    if pt.value.abs() < 1e-6 {
                                        next.value * t
                                    } else {
                                        pt.value * (next.value / pt.value).powf(t)
                                    }
                                }
                                _ => pt.value + (next.value - pt.value) * t,
                            };
                            let sx = px + (next_px - px) * t;
                            let sy = rect.bottom() - v * rect.height();
                            let cur = egui::pos2(sx, sy);
                            painter.line_segment([prev, cur], egui::Stroke::new(1.5, curve_color));
                            prev = cur;
                        }
                    }
                }
            }

            painter.circle_filled(egui::pos2(px, py), point_radius, egui::Color32::WHITE);
            painter.circle_stroke(egui::pos2(px, py), point_radius, egui::Stroke::new(1.0, egui::Color32::BLACK));

            let hex_val = (pt.value * 255.0).round() as u8;
            painter.text(
                egui::pos2(px + point_radius + 2.0, py),
                egui::Align2::LEFT_CENTER,
                format!("{:02X}", hex_val),
                egui::FontId::monospace(9.0),
                theme.automation_value_text,
            );
        }
    }

    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let rel_x = (pos.x - rect.left()).clamp(0.0, rect.width());
            let rel_y = (rect.bottom() - pos.y).clamp(0.0, rect.height());
            let row = ((rel_x / rect.width()) * num_rows as f32).round() as u16;
            let value = (rel_y / rect.height()).clamp(0.0, 1.0);

            let clicked_existing = track.points.iter().position(|p| {
                (p.row as f32 - row as f32).abs() < 2.0
                && (p.value - value).abs() < 0.05
            });

            if response.secondary_clicked() {
                if track.points.iter().any(|p| p.order == order && p.row == row) {
                    resp.point_removed = Some((track.id, order, row));
                }
            } else if response.dragged() {
                if let LaneDragState::Moving { track_id, point_idx: _ } = state.drag {
                    if track_id == track.id {
                        resp.point_changed = Some((track.id, AutomationPoint {
                            order,
                            row,
                            value,
                            interp_to_next: track.default_interp,
                        }));
                    }
                } else if let Some(idx) = clicked_existing {
                    state.drag = LaneDragState::Moving { track_id: track.id, point_idx: idx };
                }
            } else {
                resp.point_changed = Some((track.id, AutomationPoint {
                    order,
                    row,
                    value,
                    interp_to_next: track.default_interp,
                }));
            }
        }
    } else if response.hovered() && response.secondary_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let rel_x = (pos.x - rect.left()).clamp(0.0, rect.width());
            let row = ((rel_x / rect.width()) * num_rows as f32).round() as u16;
            resp.point_removed = Some((track.id, order, row));
        }
    }

    if ui.input(|i| i.pointer.any_released()) {
        state.drag = LaneDragState::None;
    }
}