use eframe::egui;
use eguidev::DevUiExt;

use crate::audio::commands::AudioCommand;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::player::PlayMode;

use super::theme::TrackerTheme;

pub struct TransportResponse {
    pub play_clicked: bool,
    pub stop_clicked: bool,
    pub pause_clicked: bool,
    pub play_from_clicked: bool,
    pub prev_pattern_clicked: bool,
    pub next_pattern_clicked: bool,
    pub bpm_changed: Option<u16>,
    pub speed_changed: Option<u8>,
    pub volume_changed: Option<f32>,
}

pub fn draw_transport(
    ui: &mut egui::Ui,
    playback_state: &AtomicPlaybackState,
    command_sender: &mut Option<crate::audio::engine::CommandSender>,
    theme: &TrackerTheme,
) -> TransportResponse {
    let mut resp = TransportResponse {
        play_clicked: false,
        stop_clicked: false,
        pause_clicked: false,
        play_from_clicked: false,
        prev_pattern_clicked: false,
        next_pattern_clicked: false,
        bpm_changed: None,
        speed_changed: None,
        volume_changed: None,
    };

    let playing = playback_state.playing.load(std::sync::atomic::Ordering::Relaxed);
    let bpm = playback_state.bpm.load(std::sync::atomic::Ordering::Relaxed);
    let speed = playback_state.speed.load(std::sync::atomic::Ordering::Relaxed);
    let order = playback_state.current_order.load(std::sync::atomic::Ordering::Relaxed);
    let row = playback_state.current_row.load(std::sync::atomic::Ordering::Relaxed);
    let pattern = playback_state.current_pattern.load(std::sync::atomic::Ordering::Relaxed);
    let play_mode = playback_state.play_mode();

    ui.horizontal(|ui| {
        ui.visuals_mut().widgets.inactive.bg_fill = theme.transport_bg;
        ui.visuals_mut().widgets.inactive.fg_stroke.color = theme.transport_fg;

        let play_label = if playing { "|| Pause" } else { "> Play" };
        let play_color = if playing { theme.transport_active } else { theme.transport_fg };
        let play_text = egui::RichText::new(play_label).color(play_color);
        let play_resp = ui.dev_button("transport.play", play_text);
        if play_resp.clicked() {
            if let Some(ref mut sender) = command_sender {
                if playing {
                    sender.send(AudioCommand::Pause);
                    resp.pause_clicked = true;
                } else {
                    sender.send(AudioCommand::Play);
                    resp.play_clicked = true;
                }
            }
        }

        if ui.dev_button("transport.stop", "[ ] Stop").clicked() {
            if let Some(ref mut sender) = command_sender {
                sender.send(AudioCommand::Stop);
            }
            resp.stop_clicked = true;
        }

        if ui.dev_button("transport.prev_pattern", "[<<]").clicked() {
            resp.prev_pattern_clicked = true;
        }

        if ui.dev_button("transport.next_pattern", "[>>]").clicked() {
            resp.next_pattern_clicked = true;
        }

        if ui.dev_button("transport.play_from", ">| Play From").clicked() {
            if let Some(ref mut sender) = command_sender {
                sender.send(AudioCommand::PlayFrom { order, row });
            }
            resp.play_from_clicked = true;
        }

        ui.dev_separator("transport.sep1");

        let is_pattern_mode = play_mode == PlayMode::Pattern;
        let pat_label = if is_pattern_mode { "[Pat]" } else { " Pat " };
        if ui.dev_button("transport.mode.pattern", pat_label).clicked() {
            if let Some(ref mut sender) = command_sender {
                sender.send(AudioCommand::SetPlayMode(PlayMode::Pattern));
            }
        }

        let is_song_mode = play_mode == PlayMode::Order;
        let song_label = if is_song_mode { "[Song]" } else { " Song " };
        if ui.dev_button("transport.mode.song", song_label).clicked() {
            if let Some(ref mut sender) = command_sender {
                sender.send(AudioCommand::SetPlayMode(PlayMode::Order));
            }
        }

        let is_loop = play_mode == PlayMode::Loop;
        let loop_label = if is_loop { "[Loop]" } else { " Loop " };
        if ui.dev_button("transport.mode.loop", loop_label).clicked() {
            if let Some(ref mut sender) = command_sender {
                if is_loop {
                    sender.send(AudioCommand::SetPlayMode(PlayMode::Once));
                } else {
                    sender.send(AudioCommand::SetPlayMode(PlayMode::Loop));
                }
            }
        }

        ui.separator();

        ui.label(
            egui::RichText::new(format!("Ord:{:03}", order))
                .font(egui::FontId::monospace(12.0))
                .color(theme.transport_fg),
        );
        ui.label(
            egui::RichText::new(format!("Pat:{:03}", pattern))
                .font(egui::FontId::monospace(12.0))
                .color(theme.transport_fg),
        );
        ui.label(
            egui::RichText::new(format!("Row:{:03}", row))
                .font(egui::FontId::monospace(12.0))
                .color(theme.transport_fg),
        );

        ui.dev_separator("transport.sep2");

        ui.dev_label("transport.bpm_label", egui::RichText::new("BPM:")
            .font(egui::FontId::monospace(12.0))
            .color(theme.transport_fg));
        let mut bpm_val = bpm as i32;
        let bpm_resp = ui.dev_drag_value_i32_range("transport.bpm", &mut bpm_val, 32..=255);
        if bpm_resp.changed() {
            resp.bpm_changed = Some(bpm_val as u16);
            if let Some(ref mut sender) = command_sender {
                sender.send(AudioCommand::SetBPM(bpm_val as u16));
            }
        }

        ui.dev_label("transport.speed_label", egui::RichText::new("Spd:")
            .font(egui::FontId::monospace(12.0))
            .color(theme.transport_fg));
        let mut speed_val = speed as i32;
        let speed_resp = ui.dev_drag_value_i32_range("transport.speed", &mut speed_val, 1..=255);
        if speed_resp.changed() {
            resp.speed_changed = Some(speed_val as u8);
            if let Some(ref mut sender) = command_sender {
                sender.send(AudioCommand::SetSpeed(speed_val as u8));
            }
        }

        ui.dev_separator("transport.sep3");

        ui.dev_label("transport.vol_label", egui::RichText::new("Vol:")
            .font(egui::FontId::monospace(12.0))
            .color(theme.transport_fg));
        let mut vol_val = playback_state.master_volume();
        let vol_resp = ui.dev_slider("transport.volume", &mut vol_val, 0.0..=1.0);
        if vol_resp.changed() {
            resp.volume_changed = Some(vol_val);
            if let Some(ref mut sender) = command_sender {
                sender.send(AudioCommand::SetMasterVolume(vol_val));
            }
        }

        ui.separator();

        let voices = playback_state.active_voices.load(std::sync::atomic::Ordering::Relaxed);
        ui.label(
            egui::RichText::new(format!("Voices:{}", voices))
                .font(egui::FontId::monospace(12.0))
                .color(theme.transport_fg),
        );

        ui.separator();

        let (peak_l, peak_r) = playback_state.master_peak();
        let meter_height = 14.0;
        let meter_width = 60.0;

        ui.vertical(|ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("L").font(egui::FontId::monospace(9.0)).color(theme.transport_fg));
                draw_vu_bar(ui, peak_l, meter_width, meter_height);
                ui.label(egui::RichText::new("R").font(egui::FontId::monospace(9.0)).color(theme.transport_fg));
                draw_vu_bar(ui, peak_r, meter_width, meter_height);
            });
        });
    });

    resp
}

fn draw_vu_bar(ui: &mut egui::Ui, level: f32, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(20, 20, 20));
    painter.rect_stroke(rect, 1.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 60)), egui::StrokeKind::Outside);

    let fill_width = (level.clamp(0.0, 1.0) * width).min(width);
    if fill_width > 0.0 {
        let fill_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top()),
            egui::pos2(rect.left() + fill_width, rect.bottom()),
        );
        let color = if level < 0.6 {
            egui::Color32::from_rgb(0, 180, 0)
        } else if level < 0.85 {
            egui::Color32::from_rgb(200, 200, 0)
        } else {
            egui::Color32::from_rgb(220, 40, 40)
        };
        painter.rect_filled(fill_rect, 1.0, color);
    }
}
