use eframe::egui;
use std::sync::Arc;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::Module;
use crate::ui::TrackerTheme;
use eguidev::DevUiExt;

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
                    if ui.dev_button("sample.import", "Open...").clicked() {
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
                            let primary_text: egui::WidgetText = egui::RichText::new(primary)
                                .font(mono_font.clone())
                                .color(fg)
                                .into();
                            let primary_galley = primary_text.into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                avail_w,
                                egui::FontSelection::Default,
                            );
                            let prim_w = primary_galley.size().x;
                            painter.galley(
                                egui::pos2(
                                    text_x,
                                    rect.center().y - primary_galley.size().y / 2.0,
                                ),
                                primary_galley.clone(),
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

                            eguidev::track_response_full(
                                format!("sample.row.{}", i),
                                &response,
                                eguidev::WidgetMeta {
                                    role: eguidev::WidgetRole::Label,
                                    label: Some(format!("{:02X}: {}", i, name)),
                                    value: Some(eguidev::WidgetValue::Text(name.to_string())),
                                    visible: ui.is_visible() && ui.is_rect_visible(response.rect),
                                    ..Default::default()
                                },
                            );
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
                let waveform_group = ui.group(|ui| {
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
                eguidev::track_response_full(
                    "sample.waveform",
                    &waveform_group.response,
                    eguidev::WidgetMeta {
                        role: eguidev::WidgetRole::Label,
                        label: Some("Waveform".to_string()),
                        visible: ui.is_visible() && ui.is_rect_visible(waveform_group.response.rect),
                        ..Default::default()
                    },
                );

                ui.horizontal(|ui| {
                    let col_w = ui.available_width() / 3.0;

                    // Properties Left Column
                    ui.vertical(|ui| {
                        ui.group(|ui| {
                            ui.set_min_width(col_w);
                            ui.heading("Playback");
                            egui::Grid::new(format!("sample_playback_{}", *selected_sample)).show(ui, |ui| {
                                ui.label("Default Vol:");
                                let mut vol = sample.default_volume as f32;
                                if ui.dev_slider("sample.prop.volume", &mut vol, 0.0..=64.0).changed() {
                                    event = Some(SampleEditEvent::VolumeChanged(vol as u8));
                                }
                                ui.end_row();

                                ui.label("Global Vol:");
                                let mut gvol = sample.global_volume as f32;
                                if ui.dev_slider("sample.prop.global_volume", &mut gvol, 0.0..=64.0).changed() {
                                    event = Some(SampleEditEvent::GlobalVolumeChanged(gvol as u8));
                                }
                                ui.end_row();

                                ui.label("Panning:");
                                let mut pan = sample.default_panning as f32;
                                if ui.dev_slider("sample.prop.panning", &mut pan, 0.0..=64.0).changed() {
                                    event = Some(SampleEditEvent::PanningChanged(pan as u8));
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
                                let mut rel = sample.relative_note as i32;
                                if ui.dev_drag_value_i32_range("sample.prop.relative_note", &mut rel, -96..=95).changed() {
                                    event = Some(SampleEditEvent::RelativeNoteChanged(rel as i8));
                                }
                                ui.end_row();

                                ui.label("Fine Tune:");
                                let mut fine = sample.fine_tune as i32;
                                if ui.dev_drag_value_i32_range("sample.prop.finetune", &mut fine, -128..=127).changed() {
                                    event = Some(SampleEditEvent::FineTuneChanged(fine as i8));
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
                                let mut start = sample.loop_start as i32;
                                if ui.dev_drag_value_i32_range("sample.prop.loop_start", &mut start, 0..=sample.data.len() as i32).changed() {
                                    event = Some(SampleEditEvent::LoopStartChanged(start as usize));
                                }
                                ui.end_row();

                                ui.label("End:");
                                let mut end = sample.loop_end as i32;
                                if ui.dev_drag_value_i32_range("sample.prop.loop_end", &mut end, 0..=sample.data.len() as i32).changed() {
                                    event = Some(SampleEditEvent::LoopEndChanged(end as usize));
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
                    {
                        let r = ui.add_enabled(has_sel, egui::Button::new("Cut"));
                        eguidev::track_response_full(
                            "sample.process.cut",
                            &r,
                            eguidev::WidgetMeta {
                                role: eguidev::WidgetRole::Button,
                                label: Some("Cut".to_string()),
                                visible: ui.is_visible() && ui.is_rect_visible(r.rect),
                                ..Default::default()
                            },
                        );
                        if r.clicked() {
                            if let Some((s, e)) = *selection {
                                event = Some(SampleEditEvent::CutRegion(s.min(e), s.max(e)));
                                *selection = None;
                            }
                        }
                    }
                    {
                        let r = ui.add_enabled(has_sel, egui::Button::new("Copy"));
                        eguidev::track_response_full(
                            "sample.process.copy",
                            &r,
                            eguidev::WidgetMeta {
                                role: eguidev::WidgetRole::Button,
                                label: Some("Copy".to_string()),
                                visible: ui.is_visible() && ui.is_rect_visible(r.rect),
                                ..Default::default()
                            },
                        );
                        if r.clicked() {
                            if let Some((s, e)) = *selection {
                                event = Some(SampleEditEvent::CopyRegion(s.min(e), s.max(e)));
                            }
                        }
                    }
                    {
                        let r = ui.add_enabled(has_clip, egui::Button::new("Paste"));
                        eguidev::track_response_full(
                            "sample.process.paste",
                            &r,
                            eguidev::WidgetMeta {
                                role: eguidev::WidgetRole::Button,
                                label: Some("Paste".to_string()),
                                visible: ui.is_visible() && ui.is_rect_visible(r.rect),
                                ..Default::default()
                            },
                        );
                        if r.clicked() {
                            let pos = selection.map(|(s, e)| s.min(e)).unwrap_or(0);
                            event = Some(SampleEditEvent::PasteRegion(pos));
                        }
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("Process:").size(10.0).color(theme.fg_dim));
                    {
                        let r = ui.add_enabled(has_sel, egui::Button::new("Crop"));
                        eguidev::track_response_full(
                            "sample.process.crop",
                            &r,
                            eguidev::WidgetMeta {
                                role: eguidev::WidgetRole::Button,
                                label: Some("Crop".to_string()),
                                visible: ui.is_visible() && ui.is_rect_visible(r.rect),
                                ..Default::default()
                            },
                        );
                        if r.clicked() {
                            if let Some((s, e)) = *selection {
                                event = Some(SampleEditEvent::CropRegion(s.min(e), s.max(e)));
                                *selection = None;
                            }
                        }
                    }
                    ui.label("Amp:");
                    ui.add(egui::DragValue::new(amplify_factor).speed(0.05).range(0.0..=10.0));
                    if ui.dev_button("sample.process.amplify", "Apply").clicked() && *amplify_factor != 1.0 {
                        event = Some(SampleEditEvent::Amplify(*amplify_factor));
                    }
                    {
                        let r = ui.add_enabled(has_sel, egui::Button::new("Silence"));
                        eguidev::track_response_full(
                            "sample.process.silence",
                            &r,
                            eguidev::WidgetMeta {
                                role: eguidev::WidgetRole::Button,
                                label: Some("Silence".to_string()),
                                visible: ui.is_visible() && ui.is_rect_visible(r.rect),
                                ..Default::default()
                            },
                        );
                        if r.clicked() {
                            if let Some((s, e)) = *selection {
                                event = Some(SampleEditEvent::SilenceRegion(s.min(e), s.max(e)));
                                *selection = None;
                            }
                        }
                    }
                    {
                        let r = ui.add_enabled(has_sel, egui::Button::new("Set Loop"));
                        eguidev::track_response_full(
                            "sample.process.loop",
                            &r,
                            eguidev::WidgetMeta {
                                role: eguidev::WidgetRole::Button,
                                label: Some("Set Loop".to_string()),
                                visible: ui.is_visible() && ui.is_rect_visible(r.rect),
                                ..Default::default()
                            },
                        );
                        if r.clicked() {
                            if let Some((s, e)) = *selection {
                                event = Some(SampleEditEvent::SetLoopFromSelection(s.min(e), s.max(e)));
                                *selection = None;
                            }
                        }
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("Destructive:").size(10.0).color(theme.fg_dim));
                    if len > 0 && ui.dev_button("sample.process.trim", "Trim").clicked() {
                        event = Some(SampleEditEvent::TrimSilence);
                    }
                    if ui.dev_button("sample.process.normalize", "Normalize").clicked() {
                        event = Some(SampleEditEvent::Normalize);
                    }
                    if ui.dev_button("sample.process.reverse", "Reverse").clicked() {
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
