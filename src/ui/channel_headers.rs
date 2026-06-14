use eframe::egui;

use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::automation::AutomationTarget;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::ui::pattern_grid::GridMetrics;

use super::theme::TrackerTheme;

pub struct ChannelHeadersResponse {
    pub toggle_mute: Option<usize>,
    pub toggle_solo: Option<usize>,
    pub rename_channel: Option<(usize, String)>,
    pub send_changed: Option<(usize, usize, f32)>,
    pub automation_target_changed: Option<(usize, Option<AutomationTarget>)>,
}

pub struct ChannelRenameState {
    pub editing_channel: Option<usize>,
    pub edit_buffer: String,
}

impl Default for ChannelRenameState {
    fn default() -> Self {
        ChannelRenameState {
            editing_channel: None,
            edit_buffer: String::new(),
        }
    }
}

pub fn draw_channel_headers(
    ui: &mut egui::Ui,
    num_channels: usize,
    scroll_channel: usize,
    visible_channels: usize,
    muted_channels: &[bool],
    solo_channels: &[bool],
    channel_names: &[String],
    channel_panning: &[u8],
    send_levels: &[[f32; NUM_SEND_BUSES]],
    rename_state: &mut ChannelRenameState,
    theme: &TrackerTheme,
    playback_state: &AtomicPlaybackState,
    metrics: GridMetrics,
    automation_targets: &[Option<AutomationTarget>],
    note_on_flash: &[bool; 64],
) -> ChannelHeadersResponse {
    let mut resp = ChannelHeadersResponse {
        toggle_mute: None,
        toggle_solo: None,
        rename_channel: None,
        send_changed: None,
        automation_target_changed: None,
    };

    let send_bar_h = 3.0;
    let send_bar_gap = 1.0;
    let vu_bar_height = 4.0;
    let pan_bar_height = 3.0;
    let auto_row_h = metrics.font_size * 1.4;

    let button_h = metrics.font_size * 1.2;
    let btn_area_h = button_h + 4.0;
    let send_area_h = (NUM_SEND_BUSES.min(4) as f32) * (send_bar_h + send_bar_gap);
    let header_height = btn_area_h + vu_bar_height + pan_bar_height + send_area_h + auto_row_h + 6.0;

    let total_width = metrics.row_num_width + visible_channels as f32 * metrics.channel_width;
    let (full_rect, _) = ui.allocate_exact_size(
        egui::vec2(total_width, header_height),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(full_rect);

    let last_ch = (scroll_channel + visible_channels).min(num_channels);

    for ch in scroll_channel..last_ch {
        let display_ch = ch - scroll_channel;
        let ch_x = full_rect.left() + metrics.row_num_width + display_ch as f32 * metrics.channel_width;
        let ch_w = metrics.channel_width - 2.0;
        let ch_rect = egui::Rect::from_min_size(
            egui::pos2(ch_x, full_rect.top()),
            egui::vec2(ch_w, header_height),
        );

        painter.rect_filled(ch_rect, 0.0, theme.channel_header_bg);

        if note_on_flash.get(ch).copied().unwrap_or(false) {
            let flash_id = ui.id().with("note_flash").with(ch);
            let flash_val = ui.ctx().animate_bool_with_time(flash_id, true, 0.3);
            if flash_val > 0.01 {
                let flash_rect = egui::Rect::from_min_size(
                    egui::pos2(ch_rect.left(), ch_rect.top()),
                    egui::vec2(ch_rect.width(), 2.0),
                );
                let flash_alpha = (flash_val * 220.0) as u8;
                painter.rect_filled(flash_rect, 0.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, flash_alpha));
            }
        } else {
            let flash_id = ui.id().with("note_flash").with(ch);
            ui.ctx().animate_bool_with_time(flash_id, false, 0.3);
        }

        let muted = muted_channels.get(ch).copied().unwrap_or(false);
        let solo = solo_channels.get(ch).copied().unwrap_or(false);

        let mut y = ch_rect.top() + 2.0;

        let btn_font = egui::FontId::monospace(metrics.font_size * 0.8);
        let mute_color = if muted { theme.channel_muted } else { theme.channel_header_fg };
        let mute_label = if muted { "M" } else { "m" };
        let mute_rect = egui::Rect::from_min_size(
            egui::pos2(ch_x + 2.0, y),
            egui::vec2(metrics.char_width * 1.6, button_h),
        );
        let mute_id = ui.id().with("mute").with(ch);
        let mute_resp = ui.interact(mute_rect, mute_id, egui::Sense::click());
        let mute_fill = if muted || mute_resp.hovered() {
            egui::Color32::from_rgb(60, 30, 30)
        } else {
            theme.status_bg
        };
        painter.rect_filled(mute_rect, 2.0, mute_fill);
        painter.text(
            mute_rect.center(),
            egui::Align2::CENTER_CENTER,
            mute_label,
            btn_font.clone(),
            mute_color,
        );
        if mute_resp.clicked() {
            resp.toggle_mute = Some(ch);
        }

        let solo_color = if solo { theme.channel_solo } else { theme.channel_header_fg };
        let solo_label = if solo { "S" } else { "s" };
        let solo_rect = egui::Rect::from_min_size(
            egui::pos2(mute_rect.right() + 2.0, y),
            egui::vec2(metrics.char_width * 1.6, button_h),
        );
        let solo_id = ui.id().with("solo").with(ch);
        let solo_resp = ui.interact(solo_rect, solo_id, egui::Sense::click());
        let solo_fill = if solo || solo_resp.hovered() {
            egui::Color32::from_rgb(30, 60, 30)
        } else {
            theme.status_bg
        };
        painter.rect_filled(solo_rect, 2.0, solo_fill);
        painter.text(
            solo_rect.center(),
            egui::Align2::CENTER_CENTER,
            solo_label,
            btn_font.clone(),
            solo_color,
        );
        if solo_resp.clicked() {
            resp.toggle_solo = Some(ch);
        }

        let is_editing = rename_state.editing_channel == Some(ch);
        let ch_name = channel_names.get(ch).cloned().unwrap_or_else(|| format!("Ch{}", ch + 1));

        if is_editing {
            let name_x = solo_rect.right() + 2.0;
            let name_w = (ch_rect.right() - name_x).max(10.0);
            let name_rect = egui::Rect::from_min_size(
                egui::pos2(name_x, y),
                egui::vec2(name_w, button_h),
            );
            let edit_id = ui.id().with("rename").with(ch);
            let mut edit_buf = rename_state.edit_buffer.clone();
            let text_edit = egui::TextEdit::singleline(&mut edit_buf)
                .font(egui::FontId::monospace(metrics.font_size * 0.8))
                .desired_width(name_w)
                .interactive(true)
                .id(edit_id);
            let mut edit_ui = ui.new_child(egui::UiBuilder::new().max_rect(name_rect));
            let te_resp = edit_ui.add(text_edit);
            te_resp.request_focus();
            rename_state.edit_buffer = edit_buf;
            if te_resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if !rename_state.edit_buffer.is_empty() {
                    resp.rename_channel = Some((ch, rename_state.edit_buffer.clone()));
                }
                rename_state.editing_channel = None;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                rename_state.editing_channel = None;
            }
        } else {
            let name_x = solo_rect.right() + 2.0;
            let name_w = (ch_rect.right() - name_x).max(10.0);
            let name_rect = egui::Rect::from_min_size(
                egui::pos2(name_x, y),
                egui::vec2(name_w, button_h),
            );
            painter.text(
                egui::pos2(name_rect.left() + 1.0, name_rect.center().y),
                egui::Align2::LEFT_CENTER,
                &ch_name,
                btn_font.clone(),
                theme.channel_header_fg,
            );
            let name_id = ui.id().with("name").with(ch);
            let name_resp = ui.interact(name_rect, name_id, egui::Sense::click());
            if name_resp.double_clicked() {
                rename_state.editing_channel = Some(ch);
                rename_state.edit_buffer = ch_name;
            }
        }

        y = ch_rect.top() + btn_area_h;

        let vu_x = ch_x + 2.0;
        let vu_w = ch_w - 4.0;
        let vu_rect = egui::Rect::from_min_size(
            egui::pos2(vu_x, y),
            egui::vec2(vu_w, vu_bar_height),
        );
        painter.rect_filled(vu_rect, 0.0, theme.meter_bg);
        let peak = playback_state.channel_peak(ch);
        let fill_w = (peak.clamp(0.0, 1.0) * vu_w).min(vu_w);
        if fill_w > 0.0 {
            let fill_rect = egui::Rect::from_min_max(
                egui::pos2(vu_rect.left(), vu_rect.top()),
                egui::pos2(vu_rect.left() + fill_w, vu_rect.bottom()),
            );
            let color = if peak < 0.6 {
                theme.vu_green
            } else if peak < 0.85 {
                theme.vu_yellow
            } else {
                theme.vu_red
            };
            painter.rect_filled(fill_rect, 0.0, color);
        }
        y += vu_bar_height;

        let pan_val = channel_panning.get(ch).copied().unwrap_or(32);
        let pan_rect = egui::Rect::from_min_size(
            egui::pos2(vu_x, y),
            egui::vec2(vu_w, pan_bar_height),
        );
        painter.rect_filled(pan_rect, 0.0, theme.meter_bg);
        let center_x = pan_rect.center().x;
        let dot_x = pan_rect.left() + (pan_val as f32 / 64.0) * pan_rect.width();
        painter.line_segment(
            [egui::pos2(center_x, pan_rect.top()), egui::pos2(center_x, pan_rect.bottom())],
            egui::Stroke::new(0.5, theme.grid_line),
        );
        let dot_r = 2.0;
        let dot_color = if pan_val < 20 {
            theme.pan_left
        } else if pan_val > 44 {
            theme.pan_right
        } else {
            theme.pan_center
        };
        painter.circle_filled(egui::pos2(dot_x, pan_rect.center().y), dot_r, dot_color);
        y += pan_bar_height + 2.0;

        let bar_colors = theme.send_bus_colors;
        for si in 0..NUM_SEND_BUSES.min(4) {
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(vu_x, y),
                egui::vec2(vu_w, send_bar_h),
            );
            painter.rect_filled(bar_rect, 0.0, theme.meter_bg);

            if let Some(lvl) = send_levels.get(ch).map(|sl| sl[si]) {
                if lvl > 0.0 {
                    let fill_w = (lvl * vu_w).min(vu_w);
                    let fill_rect = egui::Rect::from_min_max(
                        egui::pos2(bar_rect.left(), bar_rect.top()),
                        egui::pos2(bar_rect.left() + fill_w, bar_rect.bottom()),
                    );
                    painter.rect_filled(fill_rect, 0.0, bar_colors[si % bar_colors.len()]);
                }
            }

            let bar_id = ui.id().with("send").with(ch).with(si);
            let bar_resp = ui.interact(bar_rect, bar_id, egui::Sense::click_and_drag());
            if bar_resp.dragged() {
                if let Some(pos) = bar_resp.interact_pointer_pos() {
                    let rel_x = (pos.x - bar_rect.left()).clamp(0.0, vu_w);
                    let new_level = (rel_x / vu_w).clamp(0.0, 1.0);
                    resp.send_changed = Some((ch, si, new_level));
                }
            }

            y += send_bar_h + send_bar_gap;
        }

        let auto_targets = AutomationTarget::all_per_channel();
        let current_auto = automation_targets.get(ch).copied().flatten();
        let auto_label = match &current_auto {
            Some(t) => t.label(),
            None => "fx",
        };
        let auto_rect = egui::Rect::from_min_size(
            egui::pos2(vu_x, y),
            egui::vec2(vu_w, auto_row_h),
        );
        let auto_id = ui.id().with("auto").with(ch);
        let auto_resp = ui.interact(auto_rect, auto_id, egui::Sense::click());
        let auto_fill = if current_auto.is_some() {
            egui::Color32::from_rgb(40, 50, 70)
        } else if auto_resp.hovered() {
            theme.status_bg
        } else {
            theme.status_bg
        };
        painter.rect_filled(auto_rect, 2.0, auto_fill);
        let auto_font = egui::FontId::monospace(metrics.font_size * 0.7);
        let auto_color = if current_auto.is_some() {
            theme.automation_value_text
        } else {
            theme.fg_note_empty
        };
        painter.text(
            auto_rect.center(),
            egui::Align2::CENTER_CENTER,
            auto_label,
            auto_font,
            auto_color,
        );
        if auto_resp.clicked() {
            let next = match current_auto {
                None => Some(auto_targets[0]),
                Some(ref t) => {
                    let idx = auto_targets.iter().position(|a| *a == *t);
                    match idx {
                        Some(i) if i + 1 < auto_targets.len() => Some(auto_targets[i + 1]),
                        _ => None,
                    }
                }
            };
            resp.automation_target_changed = Some((ch, next));
        }
        if auto_resp.secondary_clicked() {
            let prev = match current_auto {
                None => Some(auto_targets[auto_targets.len() - 1]),
                Some(ref t) => {
                    let idx = auto_targets.iter().position(|a| *a == *t);
                    match idx {
                        Some(0) => None,
                        Some(i) => Some(auto_targets[i - 1]),
                        None => Some(auto_targets[0]),
                    }
                }
            };
            resp.automation_target_changed = Some((ch, prev));
        }
    }

    resp
}
