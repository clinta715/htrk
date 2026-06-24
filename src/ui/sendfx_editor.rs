use eframe::egui;
use crate::audio::engine::CommandSender;
use crate::audio::commands::AudioCommand;
use crate::sequencer::effect::SendEffectType;
use crate::sequencer::effect::NUM_SEND_BUSES;

fn param_label(effect: SendEffectType, index: u32) -> &'static str {
    match effect {
        SendEffectType::None => "",
        SendEffectType::Delay => match index {
            0 => "Delay (beat)",
            1 => "Feedback",
            2 => "Damping",
            3 => "Tempo Sync",
            _ => "",
        },
        SendEffectType::Reverb => match index {
            0 => "Decay",
            1 => "Damping",
            2 => "Size",
            3 => "Width",
            _ => "",
        },
        SendEffectType::Chorus => match index {
            0 => "Rate",
            1 => "Depth",
            2 => "Feedback",
            3 => "Delay (ms)",
            _ => "",
        },
        SendEffectType::Flanger => match index {
            0 => "Rate",
            1 => "Depth",
            2 => "Feedback",
            3 => "Delay (ms)",
            _ => "",
        },
        SendEffectType::Phaser => match index {
            0 => "Rate",
            1 => "Depth",
            2 => "Feedback",
            3 => "Stages",
            _ => "",
        },
    }
}

fn send_bus_label(effect: SendEffectType) -> &'static str {
    effect.name()
}

pub fn draw_sendfx_view(
    ui: &mut egui::Ui,
    command_sender: &mut Option<CommandSender>,
    send_bus_types: &mut [SendEffectType; NUM_SEND_BUSES],
    send_bus_params: &mut [[f32; 5]; NUM_SEND_BUSES],
    send_pre_fader: &mut [bool; NUM_SEND_BUSES],
    plugin_names: &mut [Option<String>; NUM_SEND_BUSES],
    plugin_browser_open_for: &mut Option<usize>,
) {
    ui.horizontal(|ui| {
        for si in 0..NUM_SEND_BUSES {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_width(200.0);
                let bus_letter = char::from(b'A' + si as u8);
                let bus_label = format!("Send Bus {} ({})", bus_letter, send_bus_label(send_bus_types[si]));
                ui.label(egui::RichText::new(&bus_label).strong().size(14.0));

                ui.horizontal(|ui| {
                    ui.label("Type:");
                    let type_name = send_bus_types[si].name();
                    egui::ComboBox::from_id_salt(("bus_type", si))
                        .selected_text(type_name)
                        .show_ui(ui, |ui| {
                            let variants = [
                                SendEffectType::None,
                                SendEffectType::Delay,
                                SendEffectType::Reverb,
                                SendEffectType::Chorus,
                                SendEffectType::Flanger,
                                SendEffectType::Phaser,
                            ];
                            for &vt in &variants {
                                if ui.selectable_label(send_bus_types[si] == vt, vt.name()).clicked() {
                                    send_bus_types[si] = vt;
                                    send_bus_params[si] = [0.0; 5]; // Reset params for new effect type
                                    if let Some(ref mut sender) = command_sender {
                                        sender.send(AudioCommand::SetSendEffectType {
                                            send_index: si,
                                            effect_type: vt,
                                        });
                                    }
                                }
                            }
                        });
                });

                // ── Plugin slot ──
                let plugin_name = plugin_names[si].as_deref().unwrap_or("").to_string();
                if !plugin_name.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 255, 100),
                        format!("Plugin: {plugin_name}")
                    );
                } else {
                    ui.label(egui::RichText::new("(built-in effect)").weak().size(11.0));
                }
                ui.horizontal(|ui| {
                    if ui.button("Plugin...").clicked() {
                        *plugin_browser_open_for = Some(si);
                    }
                    if !plugin_name.is_empty() {
                        if ui.button("Remove").clicked() {
                            if let Some(ref mut sender) = command_sender {
                                sender.send(AudioCommand::SetSendPlugin {
                                    send_index: si,
                                    processor: None,
                                });
                            }
                            plugin_names[si] = None;
                        }
                    }
                });

                ui.separator();

                let params = &mut send_bus_params[si];

                let mut rl = params[0];
                ui.add(egui::Slider::new(&mut rl, 0.0..=1.0).text("Return"));
                if (rl - params[0]).abs() > 0.005 {
                    params[0] = rl;
                    if let Some(ref mut sender) = command_sender {
                        sender.send(AudioCommand::SetSendReturnLevel { send_index: si, level: rl });
                    }
                }

                let mut pf = send_pre_fader[si];
                ui.checkbox(&mut pf, "Pre-Fader");
                if pf != send_pre_fader[si] {
                    send_pre_fader[si] = pf;
                    if let Some(ref mut sender) = command_sender {
                        sender.send(AudioCommand::SetSendPreFader { send_index: si, pre_fader: pf });
                    }
                }

                if send_bus_types[si] == SendEffectType::None {
                    ui.label("(no effect)");
                } else {
                    for pi in 0u32..4 {
                        let label = param_label(send_bus_types[si], pi);
                        if label.is_empty() { continue; }

                        let param_value = &mut params[1 + pi as usize];

                        if send_bus_types[si] == SendEffectType::Delay && pi == 3 {
                            // Tempo Sync: checkbox
                            let mut ts = *param_value > 0.5;
                            ui.checkbox(&mut ts, label);
                            let new_val = if ts { 1.0 } else { 0.0 };
                            if (new_val - *param_value).abs() > 0.01 {
                                *param_value = new_val;
                                if let Some(ref mut sender) = command_sender {
                                    sender.send(AudioCommand::SetSendFxParam {
                                        send_index: si, param: pi, value: new_val,
                                    });
                                }
                            }
                        } else if send_bus_types[si] == SendEffectType::Delay && pi == 0 {
                            // Delay beats: wider range
                            let mut val = *param_value;
                            ui.add(egui::Slider::new(&mut val, 0.0625..=8.0).text(label));
                            if (val - *param_value).abs() > 0.005 {
                                *param_value = val;
                                if let Some(ref mut sender) = command_sender {
                                    sender.send(AudioCommand::SetSendFxParam {
                                        send_index: si, param: pi, value: val,
                                    });
                                }
                            }
                        } else if send_bus_types[si] == SendEffectType::Phaser && pi == 3 {
                            // Stages: integer range
                            let mut val = *param_value;
                            ui.add(egui::Slider::new(&mut val, 2.0..=12.0).text(label));
                            if (val - *param_value).abs() > 0.01 {
                                *param_value = val;
                                if let Some(ref mut sender) = command_sender {
                                    sender.send(AudioCommand::SetSendFxParam {
                                        send_index: si, param: pi, value: val,
                                    });
                                }
                            }
                        } else {
                            // Generic 0..1 slider
                            let mut val = *param_value;
                            ui.add(egui::Slider::new(&mut val, 0.0..=1.0).text(label));
                            if (val - *param_value).abs() > 0.005 {
                                *param_value = val;
                                if let Some(ref mut sender) = command_sender {
                                    sender.send(AudioCommand::SetSendFxParam {
                                        send_index: si, param: pi, value: val,
                                    });
                                }
                            }
                        }
                    }
                }
            });
        }
    });
}
