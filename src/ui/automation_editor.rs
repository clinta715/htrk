use eframe::egui;

use crate::audio::plugins::ParamInfo;
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
    pub generator_open: bool,
}

impl Default for AutomationEditorState {
    fn default() -> Self {
        AutomationEditorState {
            selected_track_id: None,
            scroll_offset: 0.0,
            drag: LaneDragState::None,
            selected_order: 0,
            add_channel: 0,
            generator_open: false,
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
    pub generator_points: Option<(u32, Vec<AutomationPoint>)>,
}

pub fn draw_automation_editor(
    ui: &mut egui::Ui,
    module: &mut Module,
    state: &mut AutomationEditorState,
    theme: &TrackerTheme,
    get_send_bus_params: impl Fn(usize) -> Vec<ParamInfo>,
    get_instrument_params: impl Fn(u8) -> Vec<ParamInfo>,
) -> AutomationEditorResponse {
    let mut resp = AutomationEditorResponse {
        track_added: None,
        track_removed: None,
        track_toggled: None,
        point_changed: None,
        point_removed: None,
        interp_changed: None,
        generator_points: None,
    };

    let sidebar_width = 200.0;

    ui.horizontal_top(|ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(sidebar_width);
            ui.set_max_width(sidebar_width);
            ui.vertical(|ui| {
                super::style::section_header(ui, "Automation Tracks", theme);
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
                ui.label(egui::RichText::new("+ Add Track").size(super::style::FONT_BODY).color(theme.fg_instrument));

                ui.add_space(4.0);
                ui.label(egui::RichText::new("Per-Channel:").size(super::style::FONT_CAPTION).color(theme.fg_dim));
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
                ui.label(egui::RichText::new("Global:").size(super::style::FONT_CAPTION).color(theme.fg_dim));
                for target in AutomationTarget::all_global() {
                    if ui.small_button(target.label()).clicked() {
                        resp.track_added = Some((target, None));
                    }
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Plugin Params:").size(super::style::FONT_CAPTION).color(theme.fg_dim));

                // Send-bus plugin params.
                for si in 0..4 {
                    let params = get_send_bus_params(si);
                    if params.is_empty() {
                        continue;
                    }
                    let bus_letter = char::from(b'A' + si as u8);
                    ui.collapsing(
                        egui::RichText::new(format!("Send Bus {}", bus_letter))
                            .size(super::style::FONT_CAPTION),
                        |ui| {
                            for (host_index, p) in params.iter().enumerate() {
                                if !p.is_automatable {
                                    continue;
                                }
                                let host_index = host_index as u32;
                                let btn = egui::Button::new(
                                    egui::RichText::new(&p.name)
                                        .color(theme.fg_instrument),
                                );
                                if ui.add(btn).clicked() {
                                    resp.track_added = Some((
                                        AutomationTarget::PluginParam {
                                            send_bus: si as u8,
                                            host_index,
                                            param_id: p.id,
                                        },
                                        None,
                                    ));
                                }
                            }
                        },
                    );
                }

                // Instrument plugin params.
                let inst_count = module.instruments.len();
                for inst_idx in 0..inst_count {
                    if inst_idx == 0 {
                        continue;
                    }
                    let params = get_instrument_params(inst_idx as u8);
                    if params.is_empty() {
                        continue;
                    }
                    ui.collapsing(
                        egui::RichText::new(format!("Instrument {:02X}", inst_idx))
                            .size(super::style::FONT_CAPTION),
                        |ui| {
                            for (host_index, p) in params.iter().enumerate() {
                                if !p.is_automatable {
                                    continue;
                                }
                                let host_index = host_index as u32;
                                let btn = egui::Button::new(
                                    egui::RichText::new(&p.name)
                                        .color(theme.fg_instrument),
                                );
                                if ui.add(btn).clicked() {
                                    resp.track_added = Some((
                                        AutomationTarget::InstrumentPluginParam {
                                            instrument: inst_idx as u8,
                                            host_index,
                                            param_id: p.id,
                                        },
                                        None,
                                    ));
                                }
                            }
                        },
                    );
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
        ui.add_space(16.0);
        if ui.button("Generate").clicked() {
            state.generator_open = true;
        }
    });

    let num_rows = module.patterns.first().map_or(64, |p| p.num_rows);
    let lane_width = ui.available_width().max(200.0);

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(lane_width, 300.0),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, theme.bg_default);

    // Vertical grid lines for row alignment (X axis = rows)
    let row_step = if num_rows <= 64 { 1 } else if num_rows <= 128 { 2 } else { 4 };
    for row_idx in (0..num_rows).step_by(row_step) {
        let x = rect.left() + (row_idx as f32 / num_rows as f32) * rect.width();
        let is_beat_divider = row_idx % 16 == 0;
        let is_beat = row_idx % 4 == 0;
        let stroke = if is_beat_divider {
            egui::Stroke::new(0.8, theme.grid_line)
        } else if is_beat {
            egui::Stroke::new(0.5, theme.grid_line)
        } else {
            egui::Stroke::new(0.15, theme.grid_line_minor)
        };
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            stroke,
        );
    }

    // Row number labels along top edge
    for row_idx in (0..num_rows).step_by(4) {
        let x = rect.left() + (row_idx as f32 / num_rows as f32) * rect.width();
        painter.text(
            egui::pos2(x, rect.top() + 1.0),
            egui::Align2::CENTER_TOP,
            format!("{}", row_idx),
            egui::FontId::monospace(7.0),
            theme.fg_dim,
        );
    }

    // Horizontal value reference lines
    for val_pct in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let y = rect.bottom() - val_pct * rect.height();
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(0.3, theme.grid_line_minor),
        );
        // Hex value label on left
        let hex_val = (val_pct * 255.0).round() as u8;
        painter.text(
            egui::pos2(rect.left() + 2.0, y),
            egui::Align2::LEFT_CENTER,
            format!("{:02X}", hex_val),
            egui::FontId::monospace(7.0),
            theme.fg_dim,
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

                        painter.circle_filled(egui::pos2(px, py), point_radius, theme.automation_point);
                        painter.circle_stroke(egui::pos2(px, py), point_radius, egui::Stroke::new(1.0, theme.panel_border));

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

    if state.generator_open {
        let num_rows = module.patterns.first().map_or(64, |p| p.num_rows) as u16;
        if let Some(points) = draw_automation_generator_popup(
            ui.ctx(),
            &track.label(),
            num_rows,
            track.default_interp,
            &mut state.generator_open,
            theme,
        ) {
            resp.generator_points = Some((track.id, points));
        }
    }
}

fn draw_automation_generator_popup(
    ctx: &egui::Context,
    track_label: &str,
    num_rows: u16,
    default_interp: InterpolationMode,
    open: &mut bool,
    theme: &TrackerTheme,
) -> Option<Vec<AutomationPoint>> {
    let mut result = None;
    let mut should_close = false;

    egui::Window::new("Generate Automation Points")
        .id(egui::Id::new("auto_generator"))
        .open(open)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Track:").color(theme.fg_dim));
                ui.strong(egui::RichText::new(track_label).color(theme.fg_text));
            });

            ui.add_space(4.0);

            let shape_id = ui.make_persistent_id("auto_gen_shape");
            let mut shape_idx = ui.data(|d| d.get_temp::<usize>(shape_id).unwrap_or(0));
            let length_id = ui.make_persistent_id("auto_gen_length");
            let mut length = ui.data(|d| d.get_temp::<u16>(length_id).unwrap_or(64));
            let cycles_id = ui.make_persistent_id("auto_gen_cycles");
            let mut cycles = ui.data(|d| d.get_temp::<f32>(cycles_id).unwrap_or(1.0));
            let depth_id = ui.make_persistent_id("auto_gen_depth");
            let mut depth = ui.data(|d| d.get_temp::<f32>(depth_id).unwrap_or(0.75));
            let offset_id = ui.make_persistent_id("auto_gen_offset");
            let mut offset = ui.data(|d| d.get_temp::<f32>(offset_id).unwrap_or(0.5));
            let duty_id = ui.make_persistent_id("auto_gen_duty");
            let mut duty = ui.data(|d| d.get_temp::<f32>(duty_id).unwrap_or(50.0));

            super::style::section_header(ui, "Shape", theme);
            let shapes = ["Sine", "Square", "Triangle", "Saw Up", "Saw Down", "Pulse", "Random"];
            egui::ComboBox::from_id_salt("auto_gen_shape_combo")
                .selected_text(shapes[shape_idx])
                .show_ui(ui, |ui| {
                    for (i, name) in shapes.iter().enumerate() {
                        if ui.selectable_label(shape_idx == i, *name).clicked() {
                            shape_idx = i;
                        }
                    }
                });

            ui.add_space(4.0);
            super::style::section_header(ui, "Parameters", theme);
            ui.add_space(2.0);
            egui::Grid::new("auto_gen_grid").show(ui, |ui| {
                ui.label(egui::RichText::new("Rows:").color(theme.fg_dim));
                ui.add(egui::Slider::new(&mut length, 8..=256));
                ui.end_row();

                ui.label(egui::RichText::new("Cycles:").color(theme.fg_dim));
                ui.add(egui::Slider::new(&mut cycles, 0.25..=64.0).step_by(0.25));
                ui.end_row();

                ui.label(egui::RichText::new("Depth:").color(theme.fg_dim));
                ui.add(egui::Slider::new(&mut depth, 0.0..=0.5));
                ui.end_row();

                ui.label(egui::RichText::new("Offset:").color(theme.fg_dim));
                ui.add(egui::Slider::new(&mut offset, 0.0..=1.0));
                ui.end_row();

                if shape_idx == 5 {
                    ui.label(egui::RichText::new("Duty %:").color(theme.fg_dim));
                    ui.add(egui::Slider::new(&mut duty, 5.0..=95.0).step_by(1.0));
                    ui.end_row();
                }
            });

            let half_depth = depth / 2.0;
            if offset - half_depth < 0.0 {
                offset = half_depth;
            }
            if offset + half_depth > 1.0 {
                offset = 1.0 - half_depth;
            }

            let shape = match shape_idx {
                0 => crate::sequencer::envelope_generator::GeneratorShape::Sine,
                1 => crate::sequencer::envelope_generator::GeneratorShape::Square,
                2 => crate::sequencer::envelope_generator::GeneratorShape::Triangle,
                3 => crate::sequencer::envelope_generator::GeneratorShape::SawUp,
                4 => crate::sequencer::envelope_generator::GeneratorShape::SawDown,
                5 => crate::sequencer::envelope_generator::GeneratorShape::Pulse,
                _ => crate::sequencer::envelope_generator::GeneratorShape::Random,
            };

            let raw = crate::sequencer::envelope_generator::generate_values(shape, length, cycles, depth, offset, duty);

            // preview
            ui.add_space(4.0);
            super::style::section_header(ui, "Preview", theme);
            ui.add_space(2.0);
            let (preview_rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().min(400.0), 80.0),
                egui::Sense::hover(),
            );
            if !raw.is_empty() {
                let max_pos = raw.last().map(|p| p.0).unwrap_or(64) as f32;
                let painter = ui.painter_at(preview_rect);
                let to_screen = |(pos, val): &(u16, f32)| {
                    let x = preview_rect.left() + (*pos as f32 / max_pos) * preview_rect.width();
                    let y = preview_rect.bottom() - val * preview_rect.height();
                    egui::pos2(x, y)
                };
                let mut fill_points: Vec<egui::Pos2> = Vec::new();
                fill_points.push(egui::pos2(preview_rect.left(), preview_rect.bottom()));
                for p in &raw {
                    fill_points.push(to_screen(p));
                }
                fill_points.push(egui::pos2(preview_rect.right(), preview_rect.bottom()));
                painter.add(egui::Shape::convex_polygon(
                    fill_points,
                    theme.bg_highlight.gamma_multiply(0.3),
                    egui::Stroke::NONE,
                ));
                if raw.len() > 1 {
                    let line_pts: Vec<egui::Pos2> = raw.iter().map(to_screen).collect();
                    painter.add(egui::Shape::line(line_pts, egui::Stroke::new(1.5, theme.fg_instrument)));
                }
                for p in &raw {
                    painter.circle_filled(to_screen(p), 2.0, theme.fg_instrument);
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    let points: Vec<AutomationPoint> = raw.into_iter().map(|(pos, val)| {
                        AutomationPoint {
                            order: 0,
                            row: pos.min(num_rows.saturating_sub(1)),
                            value: val,
                            interp_to_next: default_interp,
                        }
                    }).collect();
                    if !points.is_empty() {
                        result = Some(points);
                    }
                    should_close = true;
                }
                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });

            ui.data_mut(|d| {
                d.insert_temp(shape_id, shape_idx);
                d.insert_temp(length_id, length);
                d.insert_temp(cycles_id, cycles);
                d.insert_temp(depth_id, depth);
                d.insert_temp(offset_id, offset);
                d.insert_temp(duty_id, duty);
            });
        });

    if should_close {
        *open = false;
    }

    result
}