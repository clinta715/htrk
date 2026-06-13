use eframe::egui;
use std::sync::Arc;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::Module;
use crate::ui::TrackerTheme;

pub enum SampleEditEvent {
    NameChanged(String),
    VolumeChanged(u8),
    PanningChanged(u8),
    GlobalVolumeChanged(u8),
    LoopTypeChanged(crate::sequencer::sample::LoopType),
    LoopStartChanged(usize),
    LoopEndChanged(usize),
    RelativeNoteChanged(i8),
    FineTuneChanged(i8),
    Normalize,
    Reverse,
    CutRegion(usize, usize),
    CopyRegion(usize, usize),
    PasteRegion(usize),
    CropRegion(usize, usize),
    Amplify(f32),
    SilenceRegion(usize, usize),
    TrimSilence,
    SetLoopFromSelection(usize, usize),
    ExportSample(usize),
    ImportSample,
}

pub fn draw_sample_editor(
    ui: &mut egui::Ui,
    module: &Module,
    selected_sample: &mut usize,
    _theme: &TrackerTheme,
    selection: &mut Option<(usize, usize)>,
    clipboard: &mut Option<Arc<Vec<f32>>>,
    amplify_factor: &mut f32,
    playback_state: &AtomicPlaybackState,
) -> Option<SampleEditEvent> {
    let mut event = None;

    ui.horizontal(|ui| {
        // Sample List
        ui.vertical(|ui| {
            ui.set_width(220.0);
            ui.set_height(ui.available_height());

            ui.horizontal(|ui| {
                ui.heading("Samples");
                if ui.button("<<").clicked() && *selected_sample > 1 {
                    *selected_sample -= 1;
                    *selection = None;
                }
                if ui.button(">>").clicked() && *selected_sample + 1 < module.samples.len() {
                    *selected_sample += 1;
                    *selection = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Open...").clicked() {
                        event = Some(SampleEditEvent::ImportSample);
                    }
                });
            });

            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let num_samples = module.samples.len();
                    if num_samples <= 1 {
                        ui.label("No samples");
                    } else {
                        for i in 1..num_samples {
                            let is_selected = i == *selected_sample;
                            let sample = &module.samples[i];
                            let has_data = !sample.data.is_empty();
                            let has_name = !sample.name.is_empty();

                            let name = if has_name { sample.name.as_str() } else { "" };

                            let len_str = if sample.data.len() >= 1_048_576 {
                                format!("{:.1}MB", sample.data.len() as f64 / 1_048_576.0)
                            } else if sample.data.len() >= 1024 {
                                format!("{:.1}KB", sample.data.len() as f64 / 1024.0)
                            } else if sample.data.len() > 0 {
                                format!("{}B", sample.data.len())
                            } else {
                                String::new()
                            };

                            let loop_info = match sample.loop_type {
                                crate::sequencer::sample::LoopType::None => "",
                                crate::sequencer::sample::LoopType::Forward => " [F]",
                                crate::sequencer::sample::LoopType::PingPong => " [PP]",
                                crate::sequencer::sample::LoopType::Backward => " [B]",
                            };

                            let detail = if has_data {
                                format!(" | {} | {}Hz{}", len_str, sample.sample_rate, loop_info)
                            } else {
                                String::new()
                            };

                            let is_playing = !playback_state.sample_positions_for(i).is_empty();

                            let play_dot = if is_playing { "  " } else { "  " };
                            let label = format!("{}{:02X}: {}{}", play_dot, i, name, detail);

                            let bg = if is_selected {
                                egui::Color32::from_rgb(60, 60, 120)
                            } else if is_playing {
                                egui::Color32::from_rgb(30, 30, 40)
                            } else if has_data {
                                egui::Color32::from_rgb(24, 24, 32)
                            } else {
                                egui::Color32::from_rgb(16, 16, 24)
                            };

                            let fg = if is_selected {
                                egui::Color32::from_rgb(255, 255, 255)
                            } else if is_playing {
                                egui::Color32::from_rgb(220, 220, 240)
                            } else if has_data || has_name {
                                egui::Color32::from_rgb(200, 200, 220)
                            } else {
                                egui::Color32::from_rgb(80, 80, 100)
                            };

                            let response = ui.add_sized(
                                [ui.available_width(), 16.0],
                                egui::Label::new(
                                    egui::RichText::new(label)
                                        .font(egui::FontId::monospace(12.0))
                                        .color(fg)
                                        .background_color(bg),
                                )
                                .sense(egui::Sense::click()),
                            );

                            if response.clicked() {
                                *selected_sample = i;
                                *selection = None;
                            }
                            if is_playing {
                                let painter = ui.painter_at(response.rect);
                                painter.circle_filled(
                                    egui::pos2(response.rect.left() + 8.0, response.rect.center().y),
                                    3.0,
                                    _theme.playback_position_line,
                                );
                            }
                            if has_data && i > 0 {
                                response.context_menu(|ui| {
                                    if ui.button("Export Sample...").clicked() {
                                        event = Some(SampleEditEvent::ExportSample(i));
                                        ui.close();
                                    }
                                });
                            }
                        }
                    }
                });
        });

        ui.separator();

        // Sample Editor Main Area
        if let Some(sample) = module.samples.get(*selected_sample) {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if ui.button("<< Prev").clicked() && *selected_sample > 1 {
                        *selected_sample -= 1;
                        *selection = None;
                    }
                    ui.heading(format!("Sample {:02X}:", *selected_sample));
                    let mut name = sample.name.clone();
                    if ui.text_edit_singleline(&mut name).changed() {
                        event = Some(SampleEditEvent::NameChanged(name));
                    }
                    if ui.button("Next >>").clicked() && *selected_sample + 1 < module.samples.len() {
                        *selected_sample += 1;
                        *selection = None;
                    }
                });
                
                // Waveform display
                ui.group(|ui| {
                    ui.set_min_height(200.0);
                    let playback_positions = playback_state.sample_positions_for(*selected_sample);
                    if let Some(w_event) = crate::ui::waveform::draw_waveform(
                        ui,
                        &sample.data,
                        sample.loop_start,
                        sample.loop_end,
                        sample.loop_type != crate::sequencer::sample::LoopType::None,
                        selection,
                        *selected_sample,
                        &playback_positions,
                        _theme,
                    ) {
                        match w_event {
                            crate::ui::waveform::WaveformEvent::LoopStartChanged(pos) => {
                                event = Some(SampleEditEvent::LoopStartChanged(pos));
                            }
                            crate::ui::waveform::WaveformEvent::LoopEndChanged(pos) => {
                                event = Some(SampleEditEvent::LoopEndChanged(pos));
                            }
                        }
                    }
                });

                ui.horizontal(|ui| {
                    // Properties Left Column
                    ui.vertical(|ui| {
                        ui.group(|ui| {
                            ui.set_width(200.0);
                            ui.heading("Playback");
                            egui::Grid::new(format!("sample_playback_{}", *selected_sample)).show(ui, |ui| {
                                ui.label("Default Vol:");
                                let mut vol = sample.default_volume;
                                if ui.add(egui::Slider::new(&mut vol, 0..=64)).changed() {
                                    event = Some(SampleEditEvent::VolumeChanged(vol));
                                }
                                ui.end_row();

                                ui.label("Global Vol:");
                                let mut gvol = sample.global_volume;
                                if ui.add(egui::Slider::new(&mut gvol, 0..=64)).changed() {
                                    event = Some(SampleEditEvent::GlobalVolumeChanged(gvol));
                                }
                                ui.end_row();

                                ui.label("Panning:");
                                let mut pan = sample.default_panning;
                                if ui.add(egui::Slider::new(&mut pan, 0..=64)).changed() {
                                    event = Some(SampleEditEvent::PanningChanged(pan));
                                }
                                ui.end_row();
                            });
                        });
                    });

                    // Properties Middle Column
                    ui.vertical(|ui| {
                        ui.group(|ui| {
                            ui.set_width(200.0);
                            ui.heading("Tuning");
                            egui::Grid::new(format!("sample_tuning_{}", *selected_sample)).show(ui, |ui| {
                                ui.label("Relative Note:");
                                let mut rel = sample.relative_note;
                                if ui.add(egui::DragValue::new(&mut rel).range(-96..=95)).changed() {
                                    event = Some(SampleEditEvent::RelativeNoteChanged(rel));
                                }
                                ui.end_row();

                                ui.label("Fine Tune:");
                                let mut fine = sample.fine_tune;
                                if ui.add(egui::DragValue::new(&mut fine).range(-128..=127)).changed() {
                                    event = Some(SampleEditEvent::FineTuneChanged(fine));
                                }
                                ui.end_row();

                                ui.label("C5 Speed:");
                                ui.label(format!("{} Hz", sample.sample_rate));
                                ui.end_row();
                            });
                        });
                    });

                    // Properties Right Column (Loops)
                    ui.vertical(|ui| {
                        ui.group(|ui| {
                            ui.set_width(250.0);
                            ui.heading("Loop");
                            egui::Grid::new(format!("sample_loop_{}", *selected_sample)).show(ui, |ui| {
                                ui.label("Type:");
                                ui.horizontal(|ui| {
                                    if ui.selectable_label(sample.loop_type == crate::sequencer::sample::LoopType::None, "Off").clicked() {
                                        event = Some(SampleEditEvent::LoopTypeChanged(crate::sequencer::sample::LoopType::None));
                                    }
                                    if ui.selectable_label(sample.loop_type == crate::sequencer::sample::LoopType::Forward, "Forward").clicked() {
                                        event = Some(SampleEditEvent::LoopTypeChanged(crate::sequencer::sample::LoopType::Forward));
                                    }
                                    if ui.selectable_label(sample.loop_type == crate::sequencer::sample::LoopType::PingPong, "Ping-Pong").clicked() {
                                        event = Some(SampleEditEvent::LoopTypeChanged(crate::sequencer::sample::LoopType::PingPong));
                                    }
                                });
                                ui.end_row();

                                ui.label("Start:");
                                let mut start = sample.loop_start;
                                if ui.add(egui::DragValue::new(&mut start).range(0..=sample.data.len())).changed() {
                                    event = Some(SampleEditEvent::LoopStartChanged(start));
                                }
                                ui.end_row();

                                ui.label("End:");
                                let mut end = sample.loop_end;
                                if ui.add(egui::DragValue::new(&mut end).range(0..=sample.data.len())).changed() {
                                    event = Some(SampleEditEvent::LoopEndChanged(end));
                                }
                                ui.end_row();
                            });
                        });
                    });
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let has_sel = selection.is_some();
                    let has_clip = clipboard.is_some();
                    let len = sample.data.len();

                    if ui.add_enabled(has_sel, egui::Button::new("Cut")).clicked() {
                        if let Some((s, e)) = *selection {
                            event = Some(SampleEditEvent::CutRegion(s.min(e), s.max(e)));
                            *selection = None;
                        }
                    }
                    if ui.add_enabled(has_sel, egui::Button::new("Copy")).clicked() {
                        if let Some((s, e)) = *selection {
                            event = Some(SampleEditEvent::CopyRegion(s.min(e), s.max(e)));
                        }
                    }
                    if ui.add_enabled(has_clip, egui::Button::new("Paste")).clicked() {
                        let pos = selection.map(|(s, e)| s.min(e)).unwrap_or(0);
                        event = Some(SampleEditEvent::PasteRegion(pos));
                    }
                    ui.separator();
                    if ui.add_enabled(has_sel, egui::Button::new("Crop")).clicked() {
                        if let Some((s, e)) = *selection {
                            event = Some(SampleEditEvent::CropRegion(s.min(e), s.max(e)));
                            *selection = None;
                        }
                    }
                    ui.label("Amp:");
                    ui.add(egui::DragValue::new(amplify_factor).speed(0.05).range(0.0..=10.0));
                    if ui.button("Apply").clicked() && *amplify_factor != 1.0 {
                        event = Some(SampleEditEvent::Amplify(*amplify_factor));
                    }
                    if ui.add_enabled(has_sel, egui::Button::new("Silence")).clicked() {
                        if let Some((s, e)) = *selection {
                            event = Some(SampleEditEvent::SilenceRegion(s.min(e), s.max(e)));
                            *selection = None;
                        }
                    }
                    if ui.add_enabled(has_sel, egui::Button::new("Set Loop")).clicked() {
                        if let Some((s, e)) = *selection {
                            event = Some(SampleEditEvent::SetLoopFromSelection(s.min(e), s.max(e)));
                            *selection = None;
                        }
                    }
                    ui.separator();
                    if len > 0 && ui.button("Trim").clicked() {
                        event = Some(SampleEditEvent::TrimSilence);
                    }
                    if ui.button("Normalize").clicked() {
                        event = Some(SampleEditEvent::Normalize);
                    }
                    if ui.button("Reverse").clicked() {
                        event = Some(SampleEditEvent::Reverse);
                    }
                });
            });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("No sample selected. Click on a sample in the list to the left.");
            });
        }
    });

    event
}
