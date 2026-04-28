use eframe::egui;

use crate::audio::playback_state::AtomicPlaybackState;

use super::theme::TrackerTheme;

pub struct ChannelHeadersResponse {
    pub toggle_mute: Option<usize>,
    pub toggle_solo: Option<usize>,
    pub rename_channel: Option<(usize, String)>,
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
    rename_state: &mut ChannelRenameState,
    theme: &TrackerTheme,
    playback_state: &AtomicPlaybackState,
) -> ChannelHeadersResponse {
    let mut resp = ChannelHeadersResponse {
        toggle_mute: None,
        toggle_solo: None,
        rename_channel: None,
    };

    let header_height = 34.0;
    let vu_bar_height = 4.0;
    let row_num_width = 32.0;
    let channel_width = 14.0 * 7.8;

    ui.horizontal(|ui| {
        ui.add_space(row_num_width);

        let last_ch = (scroll_channel + visible_channels).min(num_channels);
        for ch in scroll_channel..last_ch {
            let muted = muted_channels.get(ch).copied().unwrap_or(false);
            let solo = solo_channels.get(ch).copied().unwrap_or(false);
            let peak = playback_state.channel_peak(ch);

            ui.allocate_ui(egui::vec2(channel_width - 2.0, header_height), |ui| {
                ui.horizontal(|ui| {
                    ui.visuals_mut().widgets.inactive.bg_fill = theme.channel_header_bg;
                    ui.visuals_mut().widgets.inactive.fg_stroke.color = theme.channel_header_fg;

                    let mute_color = if muted { theme.channel_muted } else { theme.channel_header_fg };
                    let mute_label = if muted { "M" } else { "m" };
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new(mute_label)
                                .font(egui::FontId::monospace(10.0))
                                .color(mute_color),
                        ))
                        .clicked()
                    {
                        resp.toggle_mute = Some(ch);
                    }

                    let solo_color = if solo { theme.channel_solo } else { theme.channel_header_fg };
                    let solo_label = if solo { "S" } else { "s" };
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new(solo_label)
                                .font(egui::FontId::monospace(10.0))
                                .color(solo_color),
                        ))
                        .clicked()
                    {
                        resp.toggle_solo = Some(ch);
                    }

                    let ch_name = channel_names.get(ch).cloned().unwrap_or_else(|| format!("Ch{}", ch + 1));
                    let is_editing = rename_state.editing_channel == Some(ch);

                    if is_editing {
                        let edit_buf = rename_state.edit_buffer.clone();
                        let text_edit = egui::TextEdit::singleline(&mut rename_state.edit_buffer)
                            .font(egui::FontId::monospace(10.0))
                            .desired_width(40.0)
                            .interactive(true);
                        let te_resp = ui.add(text_edit);
                        te_resp.request_focus();
                        if te_resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if !edit_buf.is_empty() {
                                resp.rename_channel = Some((ch, edit_buf));
                            }
                            rename_state.editing_channel = None;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            rename_state.editing_channel = None;
                        }
                    } else {
                        let name_resp = ui.add(
                            egui::Label::new(
                                egui::RichText::new(&ch_name)
                                    .font(egui::FontId::monospace(10.0))
                                    .color(theme.channel_header_fg),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if name_resp.double_clicked() {
                            rename_state.editing_channel = Some(ch);
                            rename_state.edit_buffer = ch_name;
                        }
                    }
                });

                let vu_width = channel_width - 4.0;
                let (vu_rect, _) = ui.allocate_exact_size(egui::vec2(vu_width, vu_bar_height), egui::Sense::hover());
                let painter = ui.painter_at(vu_rect);
                painter.rect_filled(vu_rect, 0.0, egui::Color32::from_rgb(20, 20, 20));
                let fill_w = (peak.clamp(0.0, 1.0) * vu_width).min(vu_width);
                if fill_w > 0.0 {
                    let fill_rect = egui::Rect::from_min_max(
                        egui::pos2(vu_rect.left(), vu_rect.top()),
                        egui::pos2(vu_rect.left() + fill_w, vu_rect.bottom()),
                    );
                    let color = if peak < 0.6 {
                        egui::Color32::from_rgb(0, 160, 0)
                    } else if peak < 0.85 {
                        egui::Color32::from_rgb(180, 180, 0)
                    } else {
                        egui::Color32::from_rgb(200, 40, 40)
                    };
                    painter.rect_filled(fill_rect, 0.0, color);
                }
            });
        }
    });

    resp
}
