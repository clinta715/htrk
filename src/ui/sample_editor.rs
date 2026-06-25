use eframe::egui;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::Module;
use crate::ui::style::{FONT_CAPTION, FONT_TITLE};
use crate::ui::TrackerTheme;
use eguidev::DevUiExt;

#[derive(Debug, Clone)]
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
    FadeIn(usize, usize),
    FadeOut(usize, usize),
    SliceToInstrument,
    DeleteSamples(Vec<usize>),
    OpenSampleLibrary,
}

pub fn draw_sample_editor(
    ui: &mut egui::Ui,
    module: &Module,
    selected_sample: &mut usize,
    theme: &TrackerTheme,
    playback_state: &AtomicPlaybackState,
    sample_editor: &mut crate::ui::sample_editor_panel::SampleEditor,
) -> Option<SampleEditEvent> {
    let mut event = None;

    if *selected_sample != sample_editor.last_sample_index {
        sample_editor.zoom = 0.0;
        sample_editor.scroll_offset = 0.0;
        sample_editor.last_sample_index = *selected_sample;
    }

    let has_clipboard = sample_editor.clipboard.is_some();
    let selection = &mut sample_editor.selection;
    let clipboard = &mut sample_editor.clipboard;
    let amplify_factor = &mut sample_editor.amplify_factor;
    let waveform_visible = &mut sample_editor.waveform_visible;

    let list_width = sample_editor.list_width;
    let list_panel_resp = egui::Panel::left("sample_list_panel")
        .resizable(true)
        .size_range(120.0..=400.0)
        .default_size(list_width)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Samples");
                let any_selected = !sample_editor.selected_samples.is_empty();
                if any_selected {
                    let count = sample_editor.selected_samples.len();
                    if ui.dev_button("sample.delete", format!("Del {}...", count)).clicked() {
                        let mut to_delete: Vec<usize> = sample_editor.selected_samples.iter()
                            .copied()
                            .filter(|&i| i > 0 && i < module.samples.len())
                            .collect();
                        to_delete.sort();
                        to_delete.dedup();
                        if !to_delete.is_empty() {
                            event = Some(SampleEditEvent::DeleteSamples(to_delete));
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.dev_button("sample.library", "Library").clicked() {
                        event = Some(SampleEditEvent::OpenSampleLibrary);
                    }
                    if ui.dev_button("sample.import", "Open...").clicked() {
                        event = Some(SampleEditEvent::ImportSample);
                    }
                });
            });
            ui.separator();

            let ctrl_down = ui.input(|i| i.modifiers.ctrl);
            let shift_down = ui.input(|i| i.modifiers.shift);

            egui::ScrollArea::vertical()
                .id_salt("sample_list_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let num_samples = module.samples.len();
                    if num_samples <= 1 {
                        ui.label("No samples");
                    } else {
                        let row_h = 18.0_f32;
                        let mono_font = egui::FontId::monospace(11.0);
                        for i in 1..num_samples {
                            let in_selection = sample_editor.selected_samples.contains(&i);
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
                                crate::sequencer::sample::LoopType::Forward => " \u{00b7} fwd",
                                crate::sequencer::sample::LoopType::PingPong => " \u{00b7} pong",
                                crate::sequencer::sample::LoopType::Backward => " \u{00b7} bwd",
                            };

                            let detail = if has_data {
                                format!("{} \u{00b7} {}Hz{}", len_str, sample.sample_rate, loop_info)
                            } else {
                                String::new()
                            };

                            let positions = playback_state.sample_positions_for(i);
                            let is_playing = !positions.is_empty();

                            let bg = if in_selection && !is_selected {
                                theme.bg_highlight
                            } else if is_selected {
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
                                if ctrl_down {
                                    // Toggle multi-select
                                    if let Some(pos) = sample_editor.selected_samples.iter().position(|&x| x == i) {
                                        sample_editor.selected_samples.remove(pos);
                                    } else {
                                        sample_editor.selected_samples.push(i);
                                    }
                                    *selected_sample = i;
                                    *selection = None;
                                } else if shift_down {
                                    // Select range from current selected to clicked
                                    let range_start = (*selected_sample).min(i);
                                    let range_end = (*selected_sample).max(i);
                                    sample_editor.selected_samples.clear();
                                    for idx in range_start..=range_end {
                                        if idx > 0 && idx < num_samples {
                                            sample_editor.selected_samples.push(idx);
                                        }
                                    }
                                    *selected_sample = i;
                                    *selection = None;
                                } else {
                                    sample_editor.selected_samples.clear();
                                    sample_editor.selected_samples.push(i);
                                    *selected_sample = i;
                                    *selection = None;
                                }
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
    sample_editor.list_width = list_panel_resp.response.rect.width();

    if *waveform_visible {
        if let Some(sample) = module.samples.get(*selected_sample) {
            if !sample.data.is_empty() {
                let wave_height = sample_editor.waveform_height;
                let wave_panel_resp = egui::Panel::bottom("sample_waveform_panel")
                    .resizable(true)
                    .size_range(80.0..=400.0)
                    .default_size(wave_height)
                    .show_inside(ui, |ui| {
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
                            &mut sample_editor.cursor_pos,
                            &mut sample_editor.zoom,
                            &mut sample_editor.scroll_offset,
                            has_clipboard,
                        ) {
                            match w_event {
                                crate::ui::waveform::WaveformEvent::LoopStartChanged(pos) => {
                                    event = Some(SampleEditEvent::LoopStartChanged(pos));
                                }
                                crate::ui::waveform::WaveformEvent::LoopEndChanged(pos) => {
                                    event = Some(SampleEditEvent::LoopEndChanged(pos));
                                }
                                crate::ui::waveform::WaveformEvent::CutSelection => {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        event = Some(SampleEditEvent::CutRegion(start, end));
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::CopySelection => {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        event = Some(SampleEditEvent::CopyRegion(start, end));
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::PasteAtCursor => {
                                    if let Some(pos) = sample_editor.cursor_pos {
                                        event = Some(SampleEditEvent::PasteRegion(pos));
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::CropToSelection => {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        event = Some(SampleEditEvent::CropRegion(start, end));
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::SilenceSelection => {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        event = Some(SampleEditEvent::SilenceRegion(start, end));
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::Normalize => {
                                    event = Some(SampleEditEvent::Normalize);
                                }
                                crate::ui::waveform::WaveformEvent::Reverse => {
                                    event = Some(SampleEditEvent::Reverse);
                                }
                                crate::ui::waveform::WaveformEvent::TrimSilence => {
                                    event = Some(SampleEditEvent::TrimSilence);
                                }
                                crate::ui::waveform::WaveformEvent::SetLoopFromSelection => {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        event = Some(SampleEditEvent::SetLoopFromSelection(start, end));
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::FadeInSelection => {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        event = Some(SampleEditEvent::FadeIn(start, end));
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::FadeOutSelection => {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        event = Some(SampleEditEvent::FadeOut(start, end));
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::ZoomToSelection => {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        let sel_len = end.saturating_sub(start).max(1);
                                        sample_editor.zoom = sel_len as f32;
                                        sample_editor.scroll_offset = if sel_len >= sample.data.len() { 0.0 } else { start as f32 / (sample.data.len() - sel_len) as f32 };
                                    }
                                }
                                crate::ui::waveform::WaveformEvent::ZoomFit => {
                                    sample_editor.zoom = 0.0;
                                    sample_editor.scroll_offset = 0.0;
                                }
                            }
                        }
                    });
                sample_editor.waveform_height = wave_panel_resp.response.rect.height();

                // Info bar below waveform
                if let Some(sample) = module.samples.get(*selected_sample) {
                    if !sample.data.is_empty() {
                        ui.horizontal(|ui| {
                            if let Some(idx) = sample_editor.cursor_pos {
                                let time_ms = idx as f64 / sample.sample_rate as f64 * 1000.0;
                                ui.label(egui::RichText::new(format!("Pos:{} ({:.1}ms)", idx, time_ms)).size(FONT_CAPTION).monospace());
                            }
                            if let Some((s, e)) = *selection {
                                let sel_len = e.saturating_sub(s);
                                let sel_ms = sel_len as f64 / sample.sample_rate as f64 * 1000.0;
                                let mut peak: f32 = 0.0;
                                let mut sum_sq: f64 = 0.0;
                                let s_clamped = s.min(sample.data.len());
                                let e_clamped = e.min(sample.data.len());
                                for i in s_clamped..e_clamped {
                                    let v = sample.data[i].abs();
                                    if v > peak { peak = v; }
                                    sum_sq += sample.data[i] as f64 * sample.data[i] as f64;
                                }
                                let rms = (sum_sq / sel_len.max(1) as f64).sqrt() as f32;
                                let peak_db = if peak > 0.0 { 20.0 * peak.log10() } else { -f32::INFINITY };
                                let rms_db = if rms > 0.0 { 20.0 * rms.log10() } else { -f32::INFINITY };
                                ui.label(egui::RichText::new(format!("Sel:{}-{} ({}smp, {:.1}ms)", s, e, sel_len, sel_ms)).size(FONT_CAPTION).monospace());
                                ui.label(egui::RichText::new(format!("Peak:{:.1}dB", peak_db)).size(FONT_CAPTION).monospace());
                                ui.label(egui::RichText::new(format!("RMS:{:.1}dB", rms_db)).size(FONT_CAPTION).monospace());
                            }
                            ui.label(egui::RichText::new(format!("{}Hz|{}smp", sample.sample_rate, sample.data.len())).size(FONT_CAPTION).monospace());

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let zoom_pct = if sample_editor.zoom <= 0.0 || sample_editor.zoom >= sample.data.len() as f32 {
                                    100.0
                                } else {
                                    (sample.data.len() as f32 / sample_editor.zoom) * 100.0
                                };
                                ui.label(egui::RichText::new(format!("Zoom:{:.0}%", zoom_pct)).size(FONT_CAPTION).monospace());
                                if ui.dev_button("waveform.zoom_sel", "Sel").clicked() {
                                    if let Some((s, e)) = *selection {
                                        let start = s.min(e);
                                        let end = s.max(e);
                                        let sel_len = end.saturating_sub(start).max(1);
                                        sample_editor.zoom = sel_len as f32;
                                        sample_editor.scroll_offset = if sel_len >= sample.data.len() { 0.0 } else { start as f32 / (sample.data.len() - sel_len) as f32 };
                                    }
                                }
                                if ui.dev_button("waveform.fit", "Fit").clicked() {
                                    sample_editor.zoom = 0.0;
                                    sample_editor.scroll_offset = 0.0;
                                }
                            });
                        });
                    }
                }
            }
        }
    } else {
        sample_editor.cursor_pos = None;
    }

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme.bg_default))
        .show_inside(ui, |ui| {
            if let Some(sample) = module.samples.get(*selected_sample) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("SAMPLE {:02X}", *selected_sample)).strong().size(FONT_TITLE));
                    let mut name = sample.name.clone();
                    if ui.dev_text_edit("sample.header.name", &mut name).changed() {
                        event = Some(SampleEditEvent::NameChanged(name));
                    }
                    ui.dev_separator("sample.header.sep");
                    if !sample.data.is_empty() {
                        if ui.dev_button("sample.header.export", "Export...").clicked() {
                            event = Some(SampleEditEvent::ExportSample(*selected_sample));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.selectable_label(*waveform_visible, "Waveform").clicked() {
                            *waveform_visible = !*waveform_visible;
                        }
                        ui.separator();
                        if ui.dev_button("sample.header.next", ">>").clicked() && *selected_sample + 1 < module.samples.len() {
                            *selected_sample += 1;
                            *selection = None;
                        }
                        if ui.dev_button("sample.header.prev", "<<").clicked() && *selected_sample > 1 {
                            *selected_sample -= 1;
                            *selection = None;
                        }
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("sample_central_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.columns(2, |columns| {
                            let left_event = {
                                let mut ev = None;
                                crate::ui::draw_group(&mut columns[0], "Playback", theme, |ui| {
                                    egui::Grid::new("sample_playback_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                                        ui.label("Default Vol:");
                                        let mut vol = sample.default_volume as f32;
                                        if ui.dev_slider("sample.prop.volume", &mut vol, 0.0..=64.0).changed() {
                                            ev = Some(SampleEditEvent::VolumeChanged(vol as u8));
                                        }
                                        ui.end_row();

                                        ui.label("Global Vol:");
                                        let mut gvol = sample.global_volume as f32;
                                        if ui.dev_slider("sample.prop.global_volume", &mut gvol, 0.0..=64.0).changed() {
                                            ev = Some(SampleEditEvent::GlobalVolumeChanged(gvol as u8));
                                        }
                                        ui.end_row();

                                        ui.label("Panning:");
                                        let mut pan = sample.default_panning as f32;
                                        if ui.dev_slider("sample.prop.panning", &mut pan, 0.0..=64.0).changed() {
                                            ev = Some(SampleEditEvent::PanningChanged(pan as u8));
                                        }
                                        ui.end_row();
                                    });
                                });

                                crate::ui::draw_group(&mut columns[0], "Loop", theme, |ui| {
                                    egui::Grid::new("sample_loop_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                                        ui.label("Type:");
                                        ui.horizontal(|ui| {
                                            let lt = sample.loop_type;
                                            if ui.dev_selectable_value("sample.loop.none", &mut lt.clone(), crate::sequencer::sample::LoopType::None, "Off").clicked() {
                                                ev = Some(SampleEditEvent::LoopTypeChanged(crate::sequencer::sample::LoopType::None));
                                            }
                                            if ui.dev_selectable_value("sample.loop.fwd", &mut lt.clone(), crate::sequencer::sample::LoopType::Forward, "Fwd").clicked() {
                                                ev = Some(SampleEditEvent::LoopTypeChanged(crate::sequencer::sample::LoopType::Forward));
                                            }
                                            if ui.dev_selectable_value("sample.loop.pong", &mut lt.clone(), crate::sequencer::sample::LoopType::PingPong, "Pong").clicked() {
                                                ev = Some(SampleEditEvent::LoopTypeChanged(crate::sequencer::sample::LoopType::PingPong));
                                            }
                                        });
                                        ui.end_row();

                                        ui.label("Start:");
                                        let mut start = sample.loop_start as i32;
                                        if ui.dev_drag_value_i32_range("sample.prop.loop_start", &mut start, 0..=sample.data.len() as i32).changed() {
                                            ev = Some(SampleEditEvent::LoopStartChanged(start as usize));
                                        }
                                        ui.end_row();

                                        ui.label("End:");
                                        let mut end = sample.loop_end as i32;
                                        if ui.dev_drag_value_i32_range("sample.prop.loop_end", &mut end, 0..=sample.data.len() as i32).changed() {
                                            ev = Some(SampleEditEvent::LoopEndChanged(end as usize));
                                        }
                                        ui.end_row();
                                    });
                                });
                                ev
                            };

                            let right_event = {
                                let mut ev = None;
                                crate::ui::draw_group(&mut columns[1], "Tuning", theme, |ui| {
                                    egui::Grid::new("sample_tuning_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                                        ui.label("Relative Note:");
                                        let mut rel = sample.relative_note as i32;
                                        if ui.dev_drag_value_i32_range("sample.prop.relative_note", &mut rel, -96..=95).changed() {
                                            ev = Some(SampleEditEvent::RelativeNoteChanged(rel as i8));
                                        }
                                        ui.end_row();

                                        ui.label("Fine Tune:");
                                        let mut fine = sample.fine_tune as i32;
                                        if ui.dev_drag_value_i32_range("sample.prop.finetune", &mut fine, -128..=127).changed() {
                                            ev = Some(SampleEditEvent::FineTuneChanged(fine as i8));
                                        }
                                        ui.end_row();

                                        ui.label("C5 Speed:");
                                        ui.label(format!("{} Hz", sample.sample_rate));
                                        ui.end_row();
                                    });
                                });

                                crate::ui::draw_group(&mut columns[1], "Clipboard", theme, |ui| {
                                    let has_sel = selection.is_some();
                                    let has_clip = clipboard.is_some();
                                    ui.horizontal_wrapped(|ui| {
                                        let r = ui.add_enabled(has_sel, egui::Button::new("Cut"));
                                        eguidev::track_response_full(
                                            "sample.process.cut", &r,
                                            eguidev::WidgetMeta { role: eguidev::WidgetRole::Button, label: Some("Cut".to_string()),
                                                visible: ui.is_visible() && ui.is_rect_visible(r.rect), ..Default::default() },
                                        );
                                        if r.clicked() {
                                            if let Some((s, e)) = *selection {
                                                ev = Some(SampleEditEvent::CutRegion(s.min(e), s.max(e)));
                                            }
                                        }

                                        let r = ui.add_enabled(has_sel, egui::Button::new("Copy"));
                                        eguidev::track_response_full(
                                            "sample.process.copy", &r,
                                            eguidev::WidgetMeta { role: eguidev::WidgetRole::Button, label: Some("Copy".to_string()),
                                                visible: ui.is_visible() && ui.is_rect_visible(r.rect), ..Default::default() },
                                        );
                                        if r.clicked() {
                                            if let Some((s, e)) = *selection {
                                                ev = Some(SampleEditEvent::CopyRegion(s.min(e), s.max(e)));
                                            }
                                        }

                                        let r = ui.add_enabled(has_clip, egui::Button::new("Paste"));
                                        eguidev::track_response_full(
                                            "sample.process.paste", &r,
                                            eguidev::WidgetMeta { role: eguidev::WidgetRole::Button, label: Some("Paste".to_string()),
                                                visible: ui.is_visible() && ui.is_rect_visible(r.rect), ..Default::default() },
                                        );
                                        if r.clicked() {
                                            let pos = selection.map(|(s, e)| s.min(e)).unwrap_or(0);
                                            ev = Some(SampleEditEvent::PasteRegion(pos));
                                        }
                                    });
                                });
                                ev
                            };

                            if left_event.is_some() { event = left_event; }
                            if right_event.is_some() { event = right_event; }
                        });

                        ui.add_space(4.0);
                        crate::ui::draw_group(ui, "Process", theme, |ui| {
                            let has_sel = selection.is_some();
                            ui.horizontal(|ui| {
                                let r = ui.add_enabled(has_sel, egui::Button::new("Crop"));
                                eguidev::track_response_full(
                                    "sample.process.crop", &r,
                                    eguidev::WidgetMeta { role: eguidev::WidgetRole::Button, label: Some("Crop".to_string()),
                                        visible: ui.is_visible() && ui.is_rect_visible(r.rect), ..Default::default() },
                                );
                                if r.clicked() {
                                    if let Some((s, e)) = *selection {
                                        event = Some(SampleEditEvent::CropRegion(s.min(e), s.max(e)));
                                    }
                                }

                                ui.label("Amp:");
                                ui.add(egui::DragValue::new(amplify_factor).speed(0.05).range(0.0..=10.0));
                                if ui.dev_button("sample.process.amplify", "Apply").clicked() && *amplify_factor != 1.0 {
                                    event = Some(SampleEditEvent::Amplify(*amplify_factor));
                                }

                                let r = ui.add_enabled(has_sel, egui::Button::new("Silence"));
                                eguidev::track_response_full(
                                    "sample.process.silence", &r,
                                    eguidev::WidgetMeta { role: eguidev::WidgetRole::Button, label: Some("Silence".to_string()),
                                        visible: ui.is_visible() && ui.is_rect_visible(r.rect), ..Default::default() },
                                );
                                if r.clicked() {
                                    if let Some((s, e)) = *selection {
                                        event = Some(SampleEditEvent::SilenceRegion(s.min(e), s.max(e)));
                                    }
                                }

                                let r = ui.add_enabled(has_sel, egui::Button::new("Set Loop"));
                                eguidev::track_response_full(
                                    "sample.process.loop", &r,
                                    eguidev::WidgetMeta { role: eguidev::WidgetRole::Button, label: Some("Set Loop".to_string()),
                                        visible: ui.is_visible() && ui.is_rect_visible(r.rect), ..Default::default() },
                                );
                                if r.clicked() {
                                    if let Some((s, e)) = *selection {
                                        event = Some(SampleEditEvent::SetLoopFromSelection(s.min(e), s.max(e)));
                                    }
                                }
                            });
                        });

                        crate::ui::draw_group(ui, "Destructive", theme, |ui| {
                            ui.horizontal(|ui| {
                                if !sample.data.is_empty() && ui.dev_button("sample.process.trim", "Trim Silence").clicked() {
                                    event = Some(SampleEditEvent::TrimSilence);
                                }
                                if ui.dev_button("sample.process.normalize", "Normalize").clicked() {
                                    event = Some(SampleEditEvent::Normalize);
                                }
                                if ui.dev_button("sample.process.reverse", "Reverse").clicked() {
                                    event = Some(SampleEditEvent::Reverse);
                                }
                                if !sample.data.is_empty() && ui.dev_button("sample.process.slice", "Slice to Instrument...").clicked() {
                                    event = Some(SampleEditEvent::SliceToInstrument);
                                }
                            });
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
