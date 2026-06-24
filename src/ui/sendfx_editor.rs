use eframe::egui;
use crate::audio::engine::CommandSender;
use crate::audio::commands::AudioCommand;
use crate::audio::plugins::{EditorMode, HostedPluginHandle};
use crate::sequencer::effect::SendEffectType;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::ui::sendfx_panel::EframeHwnd;

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
    plugin_handles: &mut [Option<Box<dyn HostedPluginHandle>>; NUM_SEND_BUSES],
    eframe_hwnd: Option<EframeHwnd>,
) {
    // X-close poll: if any plugin's editor HWND is no longer visible
    // (user X-closed the window externally), call close_editor to keep
    // our state in sync with reality.
    for si in 0..NUM_SEND_BUSES {
        if let Some(ref mut handle) = plugin_handles[si] {
            if handle.is_editor_open() {
                if !is_editor_hwnd_visible(handle.as_ref()) {
                    handle.close_editor();
                }
            }
        }
    }

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
                                    Some(EditorMode::Embedded) => "Embedded",
                                    None => "",
                                };
                                ui.label(
                                    egui::RichText::new(mode_label)
                                        .weak()
                                        .size(10.0),
                                );
                            } else {
                                // Default: "Edit..." (floating). The user can
                                // hold Alt or use the menu for embedded mode.
                                if ui.button("Edit...").clicked() {
                                    if let Some(ref mut handle) = plugin_handles[si] {
                                        if let Err(e) = handle.open_editor(
                                            EditorMode::Floating,
                                            None,
                                        ) {
                                            eprintln!("[plugin] open editor failed: {e}");
                                        }
                                    }
                                }
                                if eframe_hwnd.is_some() {
                                    if ui.button("Edit (in htrk)").clicked() {
                                        if let Some(ref mut handle) = plugin_handles[si] {
                                            // The HWND is stored as a usize token;
                                            // convert back to *mut c_void for the
                                            // trait method.
                                            let parent: *mut std::ffi::c_void =
                                                eframe_hwnd.unwrap() as *mut _;
                                            if let Err(e) = handle.open_editor(
                                                EditorMode::Embedded,
                                                Some(parent),
                                            ) {
                                                eprintln!("[plugin] open editor (embedded) failed: {e}");
                                            }
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
                            // Close the editor first, then remove the processor.
                            if let Some(ref mut handle) = plugin_handles[si] {
                                handle.close_editor();
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

                // ── Plugin Parameters (one Slider per param) ──
                // When a CLAP plugin is loaded on this bus, list its
                // exposed parameters and let the user set their values
                // with a Slider. The slider's `set_parameter` call is
                // queued to the audio-thread param ring; the plugin
                // sees the new value on the next process() call.
                if let Some(ref handle) = plugin_handles[si] {
                    let params = handle.parameter_info();
                    if !params.is_empty() {
                        ui.collapsing(format!("Parameters ({})", params.len()), |ui| {
                            for (idx, p) in params.iter().enumerate() {
                                let mut value = handle.get_parameter(p.id);
                                let min = p.min;
                                let max = p.max;
                                let range = if (max - min).abs() > f32::EPSILON {
                                    (min, max)
                                } else {
                                    (0.0, 1.0)
                                };
                                let label = if p.is_automatable {
                                    format!("{}  (A)", p.name)
                                } else {
                                    p.name.clone()
                                };
                                if ui
                                    .add(egui::Slider::new(&mut value, range.0..=range.1)
                                        .text(&label))
                                    .changed()
                                {
                                    if let Some(ref handle) = plugin_handles[si] {
                                        handle.set_parameter(p.id, value);
                                        // Also send a SetSendPluginParam command so
                                        // the engine's send_buses[si].plugin is
                                        // updated (it should be the same instance
                                        // pointed to by the handle's param ring,
                                        // but this keeps the audio-thread ring
                                        // in sync if the processor is swapped).
                                        if let Some(ref mut sender) = command_sender {
                                            sender.send(AudioCommand::SetSendPluginParam {
                                                send_index: si,
                                                param_id: p.id,
                                                value,
                                            });
                                        }
                                    }
                                    let _ = idx; // suppress unused
                                }
                            }
                        });
                    }
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
    //
    // Below the bus cards, render a dedicated row for any plugin that is
    // currently open in Embedded mode. The plugin's child HWND is already
    // parented to our (invisible) host window, which is in turn a WS_CHILD
    // of the eframe main window. We just need to make sure the host window
    // is sized to fill the egui rect.
    draw_embedded_editor_panels(ui, plugin_handles);
}

#[cfg(windows)]
fn is_editor_hwnd_visible(handle: &dyn HostedPluginHandle) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    let Some(hwnd) = handle.editor_hwnd() else {
        // No editor HWND = probably floating mode. Always "visible" (the
        // plugin manages its own window). Don't auto-close.
        return true;
    };
    unsafe { IsWindowVisible(hwnd) != 0 }
}

#[cfg(not(windows))]
fn is_editor_hwnd_visible(_handle: &dyn HostedPluginHandle) -> bool {
    // Non-Windows: can't query the OS; assume visible. The plugin
    // manages its own window in floating mode.
    true
}

/// Render the embedded editor panel row. For each send bus whose plugin is
/// currently open in Embedded mode, draw an egui frame whose rect will be
/// the target for the plugin's child HWND. We use the same trick as
/// hdaw2: query the plugin's preferred initial size, and resize the host
/// window to match the egui rect.
#[cfg(windows)]
fn draw_embedded_editor_panels(
    ui: &mut egui::Ui,
    plugin_handles: &mut [Option<Box<dyn HostedPluginHandle>>; NUM_SEND_BUSES],
) {
    // Find which buses are in Embedded mode.
    let embedded_indices: Vec<usize> = plugin_handles
        .iter()
        .enumerate()
        .filter_map(|(si, h)| {
            let h = h.as_ref()?;
            if h.is_editor_open() && h.editor_mode() == Some(EditorMode::Embedded) {
                Some(si)
            } else {
                None
            }
        })
        .collect();

    if embedded_indices.is_empty() {
        return;
    }

    ui.add_space(8.0);
    ui.separator();
    ui.label(
        egui::RichText::new("Embedded Plugin Editors")
            .strong()
            .size(13.0),
    );

    for si in embedded_indices {
        if let Some(ref mut handle) = plugin_handles[si] {
            let bus_letter = char::from(b'A' + si as u8);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_height(280.0);
                ui.label(format!("Send Bus {} (embedded editor)", bus_letter));
                // Reserve space for the plugin's child HWND. The actual
                // sizing happens in app.rs after the UI pass — here we
                // just draw the rect so egui allocates the space.
                let rect = ui.available_rect_before_wrap();
                let width = rect.width().max(100.0) as i32;
                let height = rect.height().max(100.0) as i32;
                if let Some(hwnd) = handle.editor_hwnd() {
                    // SAFETY: hwnd is a valid HWND owned by the plugin handle.
                    unsafe {
                        use windows_sys::Win32::UI::WindowsAndMessaging::MoveWindow;
                        MoveWindow(hwnd, 0, 0, width, height, 1);
                    }
                }
                ui.allocate_space(egui::vec2(width as f32, height as f32));
            });
        }
    }
}

#[cfg(not(windows))]
fn draw_embedded_editor_panels(
    _ui: &mut egui::Ui,
    _plugin_handles: &mut [Option<Box<dyn HostedPluginHandle>>; NUM_SEND_BUSES],
) {
    // No-op on non-Windows.
}
