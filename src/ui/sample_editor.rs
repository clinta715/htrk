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
    theme: &TrackerTheme,
    selection: &mut Option<(usize, usize)>,
    clipboard: &mut Option<Arc<Vec<f32>>>,
    amplify_factor: &mut f32,
    playback_state: &AtomicPlaybackState,
    sample_split: &mut f32,
) -> Option<SampleEditEvent> {
    let mut event = None;

    ui.horizontal(|ui| {
        let total_w = ui.available_width();
        let list_w = (total_w * *sample_split).max(120.0);

        // Sample List
        ui.vertical(|ui| {
            ui.set_width(list_w);
            ui.set_height(ui.available_height());

            ui.horizontal(|ui| {
                ui.heading("Samples");
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
                        let row_h = 18.0_f32;
                        let mono_font = egui::FontId::monospace(11.0);
                        let char_w = ui
                            .painter()
                            .layout("0".to_string(), mono_font.clone(), egui::Color32::WHITE, f32::INFINITY)
                            .size()
                            .x;
                        for i in 1..num_samples {
                            let is_selected = i == *selected_sample;
                            let sample = &module.samples[i];
                            let has_data = !sample.data.is_empty();
                            let has_name = !sample.name.is_empty();
                            let name = if has_name { sample.name.as_str() } else { "---" };

                            let len_str = if sample.data.len() >= 1_048_576 {
                                format!("{:.1}MB", sample.data.len() as f64 / 1_048_576.0)
                            } else if sample.data.len() >= 1024 {
                                format!("{:.1}KB", sample.data.len() as f64 / 1024.0)
                            } else if has_data {
                                format!("{}smp", sample.data.len())
                            } else {
                                String::new()
                            };

                            let loop_info = match sample.loop_type {
                                crate::sequencer::sample::LoopType::None => "",
                                crate::sequencer::sample::LoopType::Forward => " · fwd",
                                crate::sequencer::sample::LoopType::PingPong => " · pong",
                                crate::sequencer::sample::LoopType::Backward => " · bwd",
                            };

                            let detail = if has_data {
                                format!("{} · {}Hz{}", len_str, sample.sample_rate, loop_info)
                            } else {
                                String::new()
                            };

                            let positions = playback_state.sample_positions_for(i);
                            let is_playing = !positions.is_empty();

                            let bg = if is_selected {
                                theme.bg_selected
                            } else if is_playing {
                                theme.bg_playback
                            } else {
                                theme.bg_default
                            };
                            let fg = if is_selected {
                                theme.fg_note
                            } else if has_data || has_name {
                                theme.channel_header_fg
                            } else {
                                theme.fg_note_empty
                            };

                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), row_h),
                                egui::Sense::click(),
                            );
                            let painter = ui.painter_at(rect);
                            painter.rect_filled(rect, 3.0, bg);

                            let text_x = rect.left() + 10.0;
                            let primary = format!("{:02X}: {}", i, name);
                            let avail_w = (rect.right() - 76.0 - 8.0 - text_x).max(40.0);
                            let primary = if primary.chars().count() as f32 * char_w <= avail_w {
                                primary
                            } else {
                                let take = ((avail_w / char_w).floor() as usize).saturating_sub(1).max(1);
                                let trunc: String = primary.chars().take(take).collect();
                                format!("{trunc}\u{2026}")
                            };
                            let prim_w = painter
                                .layout(primary.clone(), mono_font.clone(), fg, f32::INFINITY)
                                .size()
                                .x;
                            painter.text(
                                egui::pos2(text_x, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                primary,
                                mono_font.clone(),
                                fg,
                            );

                            if has_data && !detail.is_empty() {
                                let dw = painter
                                    .layout(detail.clone(), egui::FontId::monospace(9.0), theme.fg_note_empty, f32::INFINITY)
                                    .size()
                                    .x;
                                let dx = text_x + prim_w + 8.0;
                                if dx + dw < rect.right() - 76.0 {
                                    painter.text(
                                        egui::pos2(dx, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        detail,
                                        egui::FontId::monospace(9.0),
                                        theme.fg_note_empty,
                                    );
                                }
                            }

                            if has_data {
                                crate::ui::sample_palette::draw_waveform_thumbnail(
                                    &painter,
                                    rect,
                                    &sample.data,
                                    is_selected,
                                    &positions,
                                    theme,
                                );
                            }

                            if is_selected || is_playing {
                                let bar = if is_selected { theme.fg_volume } else { theme.playback_position_line };
                                painter.rect_filled(
                                    egui::Rect::from_min_size(
                                        egui::pos2(rect.left(), rect.top()),
                                        egui::vec2(3.0, rect.height()),
                                    ),
                                    0.0,
                                    bar,
                                );
                            }

                            if response.clicked() {
                                *selected_sample = i;
                                *selection = None;
                            }
                            if has_data {
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

        crate::ui::draw_vertical_splitter(ui, total_w, sample_split, 0.15, 0.70, theme);

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
                        theme,
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
                    let col_w = ui.available_width() / 3.0;

                    // Properties Left Column
                    ui.vertical(|ui| {
                        ui.group(|ui| {
                            ui.set_min_width(col_w);
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
                            ui.set_min_width(col_w);
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
                            ui.set_min_width(col_w);
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

                    ui.label(egui::RichText::new("Clipboard:").size(10.0).color(theme.fg_dim));
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
                    ui.label(egui::RichText::new("Process:").size(10.0).color(theme.fg_dim));
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
                    ui.label(egui::RichText::new("Destructive:").size(10.0).color(theme.fg_dim));
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
