use eframe::egui;
use crate::audio::engine::CommandSender;
use crate::audio::commands::AudioCommand;
use crate::audio::plugins::{EditorMode, HostedPluginHandle};
use crate::sequencer::effect::SendEffectType;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::ui::sendfx_panel::EframeHwnd;
use crate::ui::style::{FONT_CAPTION, SP_XS};

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

/// Draw the hosted plugin's exposed parameters as a compact, multi-column
/// slider panel — a "procedural GUI" for plugins whose native editor is
/// unavailable or undesirable. The column count adapts to the available
/// width (wide panels like the instrument editor get several columns;
/// narrow ones like a send-bus slot collapse to one), a filter box narrows
/// large param sets, and the grid is bounded by a scroll area so a 50+
/// param plugin doesn't take over the whole window. Labels are truncated
/// with the full name shown on hover.
///
/// The handle's `set_parameter` is called directly (queued into the
/// plugin's SPSC param ring for the next `process()`), and `on_change` is
/// invoked with `(param_id, value)` so the caller can dispatch the matching
/// `AudioCommand` (`SetSendPluginParam` / `SetInstrumentPluginParam`).
pub(crate) fn draw_plugin_parameter_sliders(
    ui: &mut egui::Ui,
    handle: &dyn HostedPluginHandle,
    mut on_change: impl FnMut(u32, f32),
) {
    let params = handle.parameter_info();
    if params.is_empty() {
        return;
    }
    ui.collapsing(format!("Parameters ({})", params.len()), |ui| {
        // Quick filter — invaluable for plugins with dozens of params.
        let filter_id = ui.make_persistent_id("plugin_param_filter");
        let mut filter = ui.data(|d| d.get_temp::<String>(filter_id).unwrap_or_default());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Filter:")
                    .size(FONT_CAPTION)
                    .color(ui.visuals().weak_text_color()),
            );
            let resp = ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("parameter name…")
                    .desired_width(ui.available_width()),
            );
            if resp.changed() {
                ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            }
        });
        ui.add_space(SP_XS);

        let filter_lower = filter.to_lowercase();
        let visible: Vec<_> = params
            .iter()
            .filter(|p| {
                filter_lower.is_empty() || p.name.to_lowercase().contains(&filter_lower)
            })
            .collect();
        if visible.is_empty() {
            ui.label(egui::RichText::new(format!("No parameters match \"{}\".", filter)).weak());
            return;
        }

        // Column count adapts to width: ~220px per slider is comfortable.
        // Use the floor so columns always fit within `avail`. The old
        // `col_w = (avail / cols).max(target_col_w)` could make total
        // width exceed `avail`, pushing the last column off-screen.
        // Use horizontal_wrapped with fixed-width sliders so items naturally
        // fill rows and wrap — no Grid column-width distribution issues.
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    // Each slider gets a fixed ~340px (label + level value).
                    // Once the row is full, wrapped layout starts the next row.
                    let slider_w = 340.0;
                    for p in &visible {
                        let mut value = handle.get_parameter(p.id);
                        let range = if (p.max - p.min).abs() > f32::EPSILON {
                            (p.min, p.max)
                        } else {
                            (0.0, 1.0)
                        };
                        let full = if p.is_automatable {
                            format!("{}  (A)", p.name)
                        } else {
                            p.name.clone()
                        };
                        let label = truncate_label(&full, 22);
                        let resp = ui.add_sized(
                            egui::vec2(slider_w, ui.spacing().interact_size.y),
                            egui::Slider::new(&mut value, range.0..=range.1)
                                .text(&label)
                                .clamping(egui::SliderClamping::Always),
                        );
                        if resp.changed() {
                            handle.set_parameter(p.id, value);
                            on_change(p.id, value);
                        }
                        resp.on_hover_text(&full);
                    }
                });
            });
    });
}

/// Truncate a label to `max` chars, appending an ellipsis if shortened.
fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

pub fn draw_sendfx_view(
    ui: &mut egui::Ui,
    command_sender: &mut Option<CommandSender>,
    send_bus_types: &mut [SendEffectType; NUM_SEND_BUSES],
    send_bus_params: &mut [[f32; 5]; NUM_SEND_BUSES],
    send_pre_fader: &mut [bool; NUM_SEND_BUSES],
    plugin_names: &mut [Option<String>; NUM_SEND_BUSES],
    plugin_browser_open_for: &mut Option<usize>,
    plugin_handles: &mut [Option<Box<dyn HostedPluginHandle>>; NUM_SEND_BUSES],
    eframe_hwnd: Option<EframeHwnd>,
    // Called when the user clicks "Remove" on a plugin. The first arg is
    // the send-bus index, the second is the plugin's saved state blob
    // (caller decides what to do with it — typically write into the
    // module's `send_bus_plugins[i].state` field).
    mut on_remove_plugin: impl FnMut(usize, Vec<u8>),
) {
    // X-close poll and embedded editor panel rendering now happen in
    // HtrkApp::tick_plugin_editors (called from the frame-level ui()
    // pass) so they work regardless of which tab the user is in.

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
                        let has_editor = plugin_handles[si]
                            .as_ref()
                            .map(|h| h.has_editor())
                            .unwrap_or(false);
                        let is_open = plugin_handles[si]
                            .as_ref()
                            .map(|h| h.is_editor_open())
                            .unwrap_or(false);
                        let current_mode = plugin_handles[si]
                            .as_ref()
                            .and_then(|h| h.editor_mode());

                        if has_editor {
                            if is_open {
                                if ui.button("Close").clicked() {
                                    if let Some(ref mut handle) = plugin_handles[si] {
                                        handle.close_editor();
                                    }
                                }
                                // Show the active mode as a small label
                                let mode_label = match current_mode {
                                    Some(EditorMode::Floating) => "Floating",
                                    None => "",
                                };
                                ui.label(
                                    egui::RichText::new(mode_label)
                                        .weak()
                                        .size(10.0),
                                );
                            } else {
                                // "Edit..." opens the plugin's own floating
                                // window. The in-app parameter sliders below
                                // (Parameters section) provide the procedural
                                // GUI; the old embedded native-GUI view was
                                // removed.
                                if ui.button("Edit...").clicked() {
                                    if let Some(ref mut handle) = plugin_handles[si] {
                                        if let Err(e) = handle.open_editor(
                                            EditorMode::Floating,
                                            eframe_hwnd.map(|h| h as *mut std::ffi::c_void),
                                        ) {
                                            eprintln!("[plugin] open editor failed: {e}");
                                        }
                                    }
                                }
                            }

                            // Editor error label (red, below the button row)
                            if let Some(err) = plugin_handles[si]
                                .as_ref()
                                .and_then(|h| h.last_editor_error())
                            {
                                ui.colored_label(
                                    egui::Color32::from_rgb(255, 100, 100),
                                    err,
                                );
                            }
                        }

                        if ui.button("Remove").clicked() {
                            // Close the editor first, then save the plugin
                            // state via the callback, then remove the processor.
                            if let Some(ref mut handle) = plugin_handles[si] {
                                handle.close_editor();
                                // Capture state before the handle is dropped.
                                if let Ok(state) = handle.save_state() {
                                    on_remove_plugin(si, state);
                                }
                            }
                            if let Some(ref mut sender) = command_sender {
                                sender.send(AudioCommand::SetSendPlugin {
                                    send_index: si,
                                    processor: None,
                                });
                            }
                            plugin_handles[si] = None;
                            plugin_names[si] = None;
                        }
                    }
                });

                // ── Plugin Parameters ──
                // Procedural parameter GUI (multi-column slider panel with
                // filter). Shared with the instrument editor via
                // draw_plugin_parameter_sliders. The slider queues to the
                // plugin's param ring; SetSendPluginParam mirrors the value
                // to the audio-thread processor.
                if let Some(ref handle) = plugin_handles[si] {
                    draw_plugin_parameter_sliders(
                        ui,
                        handle.as_ref(),
                        |param_id, value| {
                            if let Some(ref mut sender) = command_sender {
                                sender.send(AudioCommand::SetSendPluginParam {
                                    send_index: si,
                                    param_id,
                                    value,
                                });
                            }
                        },
                    );
                }

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

    // ── Embedded editor panel ──
    // Embedded editor panel rendering now happens in
    // HtrkApp::tick_plugin_editors (called from the frame-level ui()
    // pass) so editors are visible in any tab.
}
