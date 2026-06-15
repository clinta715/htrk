use eframe::egui;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::edit::EnvelopeType;
use crate::sequencer::instrument::{EnvelopeFlags, EnvelopePoint, Instrument};
use crate::sequencer::Module;
use crate::ui::TrackerTheme;

pub enum InstrumentEditEvent {
    NameChanged(String),
    NnaChanged(crate::sequencer::instrument::NewNoteAction),
    DuplicateCheckTypeChanged(crate::sequencer::instrument::DuplicateCheckType),
    DuplicateCheckActionChanged(crate::sequencer::instrument::DuplicateCheckAction),
    FadeoutChanged(u16),
    GlobalVolumeChanged(u8),
    PitchPanSeparationChanged(i8),
    PitchPanCenterChanged(u8),
    RandomVolumeChanged(u8),
    RandomPanningChanged(u8),
    FilterCutoffChanged(u16),
    FilterResonanceChanged(u8),
    FilterTypeChanged(crate::sequencer::effect::FilterType),
    FilterRandomCutoffChanged(u8),
    EnvelopePointMoved(EnvelopeType, usize, u16, u8),
    EnvelopePointAdded(EnvelopeType, u16, u8),
    EnvelopePointRemoved(EnvelopeType, usize),
    EnvelopeSustainChanged(EnvelopeType, Option<usize>),
    EnvelopeLoopChanged(EnvelopeType, bool, Option<usize>, Option<usize>),
    EnvelopeFlagsChanged(EnvelopeType, EnvelopeFlags),
    GenerateEnvelope(EnvelopeType, Vec<EnvelopePoint>),
    NoteMapChanged(u8, u8),
    SampleMapChanged(u8, u8),
    SampleMapFillAll(u8),
    VibTypeChanged(u8),
    VibSweepChanged(u8),
    VibDepthChanged(u8),
    VibRateChanged(u8),
    SaveInstrument,
    LoadInstrument,
    ExportInstrument(usize),
    ImportInstrument,
}

pub fn draw_instrument_editor(
    ui: &mut egui::Ui,
    module: &Module,
    selected_instrument: &mut usize,
    selected_sample: &mut usize,
    theme: &TrackerTheme,
    playback_state: &AtomicPlaybackState,
    instrument_split: &mut f32,
    instrument_settings_split: &mut f32,
) -> Option<InstrumentEditEvent> {
    let mut event = None;

    let paint_sample_id = ui.make_persistent_id("sample_map_paint_idx");
    let mut paint_sample_idx = ui.data(|d| d.get_temp::<u8>(paint_sample_id).unwrap_or(0));
    let browser_open_id = ui.make_persistent_id("sample_browser_open");
    let mut browser_open = ui.data(|d| d.get_temp::<bool>(browser_open_id).unwrap_or(false));

    let env_type_id = ui.make_persistent_id("instrument_env_type");
    let mut env_type = ui.data(|d| d.get_temp::<crate::edit::EnvelopeType>(env_type_id).unwrap_or(crate::edit::EnvelopeType::Volume));
    let generator_open_id = ui.make_persistent_id("env_generator_open");
    let mut generator_open = ui.data(|d| d.get_temp::<bool>(generator_open_id).unwrap_or(false));

    ui.horizontal(|ui| {
        let total_w = ui.available_width();
        let list_w = (total_w * *instrument_split).max(100.0);

        // Instrument List
        ui.vertical(|ui| {
            ui.set_width(list_w);
            ui.set_height(ui.available_height());
            ui.heading("Instruments");
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                let max_idx = module.instruments.len().max(100).min(100);
                for i in 1..max_idx {
                    let is_selected = *selected_instrument == i;
                    let has_inst = module.instruments.get(i).is_some();
                    let (label, label_color) = if has_inst {
                        let inst = module.instruments.get(i).unwrap();
                        let name = if inst.name.is_empty() { "Untitled" } else { &inst.name };
                        let dot = if inst.volume_envelope.as_ref().map_or(false, |e| e.flags.enabled) { "●" } else { "○" };
                        (format!("{} {:02}: {}", dot, i, name), theme.fg_text)
                    } else {
                        (format!("  {:02}: (empty)", i), theme.fg_dimmer)
                    };
                    let response = ui.add_sized(
                        [ui.available_width(), 18.0],
                        egui::Label::new(egui::RichText::new(&label).color(label_color)).sense(egui::Sense::click()),
                    );
                    if response.clicked() {
                        *selected_instrument = i;
                        if let Some(inst) = module.instruments.get(i) {
                            if let Some(s) = inst.sample_map.iter().copied().find(|&s| s > 0) {
                                *selected_sample = s as usize;
                            }
                        }
                    }
                    // highlight background
                    if is_selected {
                        let r = response.rect;
                        let painter = ui.painter_at(r);
                        painter.rect_filled(r, 0.0, theme.bg_selected);
                        let text_col = if has_inst { theme.fg_text } else { theme.fg_dimmer };
                        painter.text(
                            egui::pos2(r.left() + 4.0, r.center().y),
                            egui::Align2::LEFT_CENTER,
                            &label,
                            egui::FontId::proportional(14.0),
                            text_col,
                        );
                    }
                    // context menu
                    if has_inst && i > 0 {
                        response.context_menu(|ui| {
                            if ui.button("Export...").clicked() {
                                event = Some(InstrumentEditEvent::ExportInstrument(i));
                                ui.close();
                            }
                            if ui.button("Import...").clicked() {
                                event = Some(InstrumentEditEvent::ImportInstrument);
                                ui.close();
                            }
                        });
                    }
                    // separator line
                    let r = response.rect;
                    ui.painter_at(r).line_segment(
                        [egui::pos2(r.left() + 2.0, r.bottom()), egui::pos2(r.right() - 2.0, r.bottom())],
                        egui::Stroke::new(1.0, theme.grid_line_minor),
                    );
                }
            });
        });

        crate::ui::draw_vertical_splitter(ui, total_w, instrument_split, 0.08, 0.40, theme);

        // Instrument Editor Main Area
        if let Some(inst) = module.instruments.get(*selected_instrument) {
            ui.vertical(|ui| {
                // ---- Header ----
                ui.horizontal(|ui| {
                    ui.heading(format!("Instrument {:02}:", *selected_instrument));
                    let mut name = inst.name.clone();
                    if ui.text_edit_singleline(&mut name).changed() {
                        event = Some(InstrumentEditEvent::NameChanged(name));
                    }
                    ui.separator();
                    if ui.button("Save...").clicked() {
                        event = Some(InstrumentEditEvent::SaveInstrument);
                    }
                    if ui.button("Load...").clicked() {
                        event = Some(InstrumentEditEvent::LoadInstrument);
                    }
                });

                // ---- Vertical split between settings and envelope ----
                let total_h = ui.available_height();
                let split_y = (total_h * *instrument_settings_split).max(80.0).min(total_h - 100.0);

                // --- Top section: settings ---
                let (top_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), split_y), egui::Sense::hover());
                {
                    let mut top_ui = ui.new_child(egui::UiBuilder::new().max_rect(top_rect).layout(*ui.layout()));
                    if let Some(e) = draw_settings_grid(
                        &mut top_ui, inst, theme, *selected_instrument, module,
                        &mut paint_sample_idx, &mut browser_open, playback_state,
                    ) {
                        event = Some(e);
                    }
                }

                // --- Horizontal splitter ---
                crate::ui::draw_horizontal_splitter(ui, total_h, instrument_settings_split, 0.15, 0.82, theme);

                // --- Bottom section: envelope editor ---
                let bottom_rect = egui::Rect::from_min_size(
                    egui::pos2(top_rect.left(), ui.cursor().top()),
                    egui::vec2(top_rect.width(), ui.available_height()),
                );
                {
                    let mut bottom_ui = ui.new_child(egui::UiBuilder::new().max_rect(bottom_rect).layout(*ui.layout()));
                    if let Some(e) = draw_envelope_section(
                        &mut bottom_ui, inst, &mut env_type, theme, playback_state,
                        *selected_instrument, &mut generator_open,
                    ) {
                        event = Some(e);
                    }
                }
            });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("No instrument selected. Click on an instrument in the list to the left.");
            });
        }
    });

    if browser_open {
        if let Some(idx) = crate::ui::sample_palette::draw_sample_browser_popup(
            ui.ctx(),
            module,
            paint_sample_idx,
            &mut browser_open,
            playback_state,
            theme,
        ) {
            paint_sample_idx = idx;
        }
    }

    if generator_open {
        if let Some(points) = draw_envelope_generator_popup(ui.ctx(), env_type, &mut generator_open) {
            event = Some(InstrumentEditEvent::GenerateEnvelope(env_type, points));
        }
    }

    ui.data_mut(|d| {
        d.insert_temp(paint_sample_id, paint_sample_idx);
        d.insert_temp(browser_open_id, browser_open);
        d.insert_temp(generator_open_id, generator_open);
    });

    event
}

fn draw_settings_grid(
    ui: &mut egui::Ui,
    inst: &Instrument,
    theme: &TrackerTheme,
    selected_instrument: usize,
    module: &Module,
    paint_sample_idx: &mut u8,
    browser_open: &mut bool,
    playback_state: &AtomicPlaybackState,
) -> Option<InstrumentEditEvent> {
    let mut event = None;

    // Row 1: NNA/DC + Volumes + Vibrato
    ui.add_space(4.0);
    egui::ScrollArea::horizontal()
        .id_salt("instrument_editor_bottom_row1")
        .max_height(110.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.heading("NNAs & Duplicate Check");
            egui::Grid::new(format!("instrument_nna_{}", selected_instrument)).show(ui, |ui| {
                ui.label("NNA:");
                ui.horizontal(|ui| {
                    use crate::sequencer::instrument::NewNoteAction;
                    if ui.selectable_label(inst.nna == NewNoteAction::NoteCut, "Cut").clicked() {
                        event = Some(InstrumentEditEvent::NnaChanged(NewNoteAction::NoteCut));
                    }
                    if ui.selectable_label(inst.nna == NewNoteAction::Continue, "Cont").clicked() {
                        event = Some(InstrumentEditEvent::NnaChanged(NewNoteAction::Continue));
                    }
                    if ui.selectable_label(inst.nna == NewNoteAction::NoteOff, "Off").clicked() {
                        event = Some(InstrumentEditEvent::NnaChanged(NewNoteAction::NoteOff));
                    }
                    if ui.selectable_label(inst.nna == NewNoteAction::NoteFade, "Fade").clicked() {
                        event = Some(InstrumentEditEvent::NnaChanged(NewNoteAction::NoteFade));
                    }
                });
                ui.end_row();

                ui.label("DCC Type:");
                ui.horizontal(|ui| {
                    use crate::sequencer::instrument::DuplicateCheckType;
                    if ui.selectable_label(inst.duplicate_check_type == DuplicateCheckType::Disabled, "Off").clicked() {
                        event = Some(InstrumentEditEvent::DuplicateCheckTypeChanged(DuplicateCheckType::Disabled));
                    }
                    if ui.selectable_label(inst.duplicate_check_type == DuplicateCheckType::Note, "Note").clicked() {
                        event = Some(InstrumentEditEvent::DuplicateCheckTypeChanged(DuplicateCheckType::Note));
                    }
                    if ui.selectable_label(inst.duplicate_check_type == DuplicateCheckType::Sample, "Samp").clicked() {
                        event = Some(InstrumentEditEvent::DuplicateCheckTypeChanged(DuplicateCheckType::Sample));
                    }
                    if ui.selectable_label(inst.duplicate_check_type == DuplicateCheckType::Instrument, "Inst").clicked() {
                        event = Some(InstrumentEditEvent::DuplicateCheckTypeChanged(DuplicateCheckType::Instrument));
                    }
                });
                ui.end_row();

                ui.label("DCC Action:");
                ui.horizontal(|ui| {
                    use crate::sequencer::instrument::DuplicateCheckAction;
                    if ui.selectable_label(inst.duplicate_check_action == DuplicateCheckAction::NoteCut, "Cut").clicked() {
                        event = Some(InstrumentEditEvent::DuplicateCheckActionChanged(DuplicateCheckAction::NoteCut));
                    }
                    if ui.selectable_label(inst.duplicate_check_action == DuplicateCheckAction::NoteOff, "Off").clicked() {
                        event = Some(InstrumentEditEvent::DuplicateCheckActionChanged(DuplicateCheckAction::NoteOff));
                    }
                    if ui.selectable_label(inst.duplicate_check_action == DuplicateCheckAction::NoteFade, "Fade").clicked() {
                        event = Some(InstrumentEditEvent::DuplicateCheckActionChanged(DuplicateCheckAction::NoteFade));
                    }
                });
                ui.end_row();
            });
        });

        ui.group(|ui| {
            ui.set_min_width(280.0);
            ui.heading("Volumes & Panning");
            let label_w = 100.0;
            let widget_w = 120.0;
            egui::Grid::new(format!("instrument_vols_{}", selected_instrument)).show(ui, |ui| {
                ui.add_sized([label_w, 0.0], egui::Label::new("Global Vol:"));
                let mut gvol = inst.global_volume;
                if ui.add_sized([widget_w, 0.0], egui::Slider::new(&mut gvol, 0..=128)).changed() {
                    event = Some(InstrumentEditEvent::GlobalVolumeChanged(gvol));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Fadeout:"));
                let mut fade = inst.fade_out;
                if ui.add_sized([widget_w, 0.0], egui::DragValue::new(&mut fade).range(0..=4095)).changed() {
                    event = Some(InstrumentEditEvent::FadeoutChanged(fade));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Pitch-Pan Sep:"));
                let mut sep = inst.pitch_pan_separation;
                if ui.add_sized([widget_w, 0.0], egui::Slider::new(&mut sep, -32..=32)).changed() {
                    event = Some(InstrumentEditEvent::PitchPanSeparationChanged(sep));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Pitch-Pan Center:"));
                let mut center = inst.pitch_pan_center;
                if ui.add_sized([widget_w, 0.0], egui::DragValue::new(&mut center).range(0..=119)).changed() {
                    event = Some(InstrumentEditEvent::PitchPanCenterChanged(center));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Random Vol:"));
                let mut rvol = inst.random_volume;
                if ui.add_sized([widget_w, 0.0], egui::Slider::new(&mut rvol, 0..=100)).changed() {
                    event = Some(InstrumentEditEvent::RandomVolumeChanged(rvol));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Random Pan:"));
                let mut rpan = inst.random_panning;
                if ui.add_sized([widget_w, 0.0], egui::Slider::new(&mut rpan, 0..=100)).changed() {
                    event = Some(InstrumentEditEvent::RandomPanningChanged(rpan));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Filter Cutoff:"));
                let mut cutoff = inst.filter_cutoff;
                if ui.add_sized([widget_w, 0.0], egui::DragValue::new(&mut cutoff).range(0..=0xFFFF)).changed() {
                    event = Some(InstrumentEditEvent::FilterCutoffChanged(cutoff));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Filter Res:"));
                let mut res = inst.filter_resonance;
                if ui.add_sized([widget_w, 0.0], egui::Slider::new(&mut res, 0..=255)).changed() {
                    event = Some(InstrumentEditEvent::FilterResonanceChanged(res));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Filter Rand Cut:"));
                let mut frc = inst.filter_random_cutoff;
                if ui.add_sized([widget_w, 0.0], egui::Slider::new(&mut frc, 0..=255)).changed() {
                    event = Some(InstrumentEditEvent::FilterRandomCutoffChanged(frc));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Filter Type:"));
                let ft = inst.filter_type;
                let mut ft_u8 = ft.to_u8();
                ui.horizontal(|ui| {
                    if ui.selectable_label(ft_u8 == 0, "LP").clicked() { ft_u8 = 0; }
                    if ui.selectable_label(ft_u8 == 1, "HP").clicked() { ft_u8 = 1; }
                    if ui.selectable_label(ft_u8 == 2, "BP").clicked() { ft_u8 = 2; }
                    if ui.selectable_label(ft_u8 == 3, "Notch").clicked() { ft_u8 = 3; }
                });
                let new_ft = crate::sequencer::effect::FilterType::from_u8(ft_u8);
                if new_ft != ft {
                    event = Some(InstrumentEditEvent::FilterTypeChanged(new_ft));
                }
                ui.end_row();
            });
        });

        ui.group(|ui| {
            ui.set_min_width(280.0);
            ui.heading("Auto-Vibrato");
            let label_w = 52.0;
            let widget_w = 100.0;
            egui::Grid::new(format!("instrument_vib_{}", selected_instrument)).show(ui, |ui| {
                ui.add_sized([label_w, 0.0], egui::Label::new("Type:"));
                ui.horizontal(|ui| {
                    if ui.selectable_label(inst.vib_type == 0, "Sine").clicked() {
                        event = Some(InstrumentEditEvent::VibTypeChanged(0));
                    }
                    if ui.selectable_label(inst.vib_type == 1, "Ramp").clicked() {
                        event = Some(InstrumentEditEvent::VibTypeChanged(1));
                    }
                    if ui.selectable_label(inst.vib_type == 2, "Square").clicked() {
                        event = Some(InstrumentEditEvent::VibTypeChanged(2));
                    }
                    if ui.selectable_label(inst.vib_type == 3, "Random").clicked() {
                        event = Some(InstrumentEditEvent::VibTypeChanged(3));
                    }
                });
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Sweep:"));
                let mut sweep = inst.vib_sweep;
                if ui.add_sized([widget_w, 0.0], egui::DragValue::new(&mut sweep).range(0..=255).speed(1)).changed() {
                    event = Some(InstrumentEditEvent::VibSweepChanged(sweep));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Depth:"));
                let mut depth = inst.vib_depth;
                if ui.add_sized([widget_w, 0.0], egui::DragValue::new(&mut depth).range(0..=255).speed(1)).changed() {
                    event = Some(InstrumentEditEvent::VibDepthChanged(depth));
                }
                ui.end_row();

                ui.add_sized([label_w, 0.0], egui::Label::new("Rate:"));
                let mut rate = inst.vib_rate;
                if ui.add_sized([widget_w, 0.0], egui::DragValue::new(&mut rate).range(0..=255).speed(1)).changed() {
                    event = Some(InstrumentEditEvent::VibRateChanged(rate));
                }
                ui.end_row();
            });
        });
    });
    });

    // Row 2: Sample Map + Note Map
    ui.add_space(4.0);
    egui::ScrollArea::horizontal()
        .id_salt("instrument_editor_bottom_row2")
        .max_height(200.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                ui.label("Paint Sample:");
                if ui.button("Browse...").clicked() {
                    *browser_open = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Fill All").clicked() {
                        event = Some(InstrumentEditEvent::SampleMapFillAll(*paint_sample_idx));
                    }
                });
            });

            crate::ui::sample_palette::draw_inline_sample_palette(ui, module, paint_sample_idx, playback_state, theme);

            if let Some(map_event) = crate::ui::sample_map::draw_sample_map(
                ui,
                &inst.sample_map,
                *paint_sample_idx,
                module,
            ) {
                match map_event {
                    crate::ui::sample_map::SampleMapEvent::NoteClicked(note) |
                    crate::ui::sample_map::SampleMapEvent::NoteDragged(note) => {
                        if inst.sample_map[note as usize].saturating_sub(1) != *paint_sample_idx {
                            event = Some(InstrumentEditEvent::SampleMapChanged(note, *paint_sample_idx));
                        }
                    }
                    crate::ui::sample_map::SampleMapEvent::NoteCleared(note) => {
                        if inst.sample_map[note as usize] != 0 {
                            event = Some(InstrumentEditEvent::SampleMapChanged(note, 0));
                        }
                    }
                }
            }
        });

        ui.group(|ui| {
            if let Some(nm_event) = crate::ui::note_map::draw_note_map(ui, &inst.note_map, *paint_sample_idx, theme) {
                if inst.note_map[nm_event.note as usize] != nm_event.new_dest {
                    event = Some(InstrumentEditEvent::NoteMapChanged(nm_event.note, nm_event.new_dest));
                }
            }
        });
    });
    });

    event
}

fn draw_envelope_section(
    ui: &mut egui::Ui,
    inst: &Instrument,
    env_type: &mut EnvelopeType,
    theme: &TrackerTheme,
    playback_state: &AtomicPlaybackState,
    selected_instrument: usize,
    generator_open: &mut bool,
) -> Option<InstrumentEditEvent> {
    let mut event = None;
    let env_type_id = ui.make_persistent_id("instrument_env_type");

    // Envelope tabs with status indicators
    ui.horizontal(|ui| {
        let vol_active = inst.volume_envelope.as_ref().map_or(false, |e| e.flags.enabled);
        let pan_active = inst.panning_envelope.as_ref().map_or(false, |e| e.flags.enabled);
        let pit_active = inst.pitch_envelope.as_ref().map_or(false, |e| e.flags.enabled);
        let flt_active = inst.filter_envelope.as_ref().map_or(false, |e| e.flags.enabled);

        let vol_pts = inst.volume_envelope.as_ref().map_or(0, |e| e.points.len());
        let pan_pts = inst.panning_envelope.as_ref().map_or(0, |e| e.points.len());
        let pit_pts = inst.pitch_envelope.as_ref().map_or(0, |e| e.points.len());
        let flt_pts = inst.filter_envelope.as_ref().map_or(0, |e| e.points.len());

        let env_colors = theme.envelope_colors;
        let vol_ind = if vol_active { "●" } else { "○" };
        let pan_ind = if pan_active { "●" } else { "○" };
        let pit_ind = if pit_active { "●" } else { "○" };
        let flt_ind = if flt_active { "●" } else { "○" };

        if ui.selectable_label(*env_type == EnvelopeType::Volume, egui::RichText::new(format!("{} Vol  {}", vol_ind, vol_pts)).color(env_colors[0].0)).clicked() {
            *env_type = EnvelopeType::Volume;
            ui.data_mut(|d| d.insert_temp(env_type_id, *env_type));
        }
        if ui.selectable_label(*env_type == EnvelopeType::Panning, egui::RichText::new(format!("{} Pan  {}", pan_ind, pan_pts)).color(env_colors[1].0)).clicked() {
            *env_type = EnvelopeType::Panning;
            ui.data_mut(|d| d.insert_temp(env_type_id, *env_type));
        }
        if ui.selectable_label(*env_type == EnvelopeType::Pitch, egui::RichText::new(format!("{} Pitch {}", pit_ind, pit_pts)).color(env_colors[2].0)).clicked() {
            *env_type = EnvelopeType::Pitch;
            ui.data_mut(|d| d.insert_temp(env_type_id, *env_type));
        }
        if ui.selectable_label(*env_type == EnvelopeType::Filter, egui::RichText::new(format!("{} Flt  {}", flt_ind, flt_pts)).color(env_colors[3].0)).clicked() {
            *env_type = EnvelopeType::Filter;
            ui.data_mut(|d| d.insert_temp(env_type_id, *env_type));
        }
    });

    let envelope = match env_type {
        EnvelopeType::Volume => &inst.volume_envelope,
        EnvelopeType::Panning => &inst.panning_envelope,
        EnvelopeType::Pitch => &inst.pitch_envelope,
        EnvelopeType::Filter => &inst.filter_envelope,
    };

    let env_hovered_id = ui.make_persistent_id("env_hovered");

    if let Some(ref env) = envelope {
        // Envelope controls
        let hv = ui.data(|d| d.get_temp::<Option<usize>>(env_hovered_id).flatten());
        let frame_margin = egui::Margin::symmetric(5, 0);
        ui.horizontal(|ui| {
            egui::Frame::group(ui.style()).inner_margin(frame_margin).show(ui, |ui| {
                let mut enabled = env.flags.enabled;
                if ui.checkbox(&mut enabled, "Enabled").changed() {
                    let mut new_flags = env.flags;
                    new_flags.enabled = enabled;
                    event = Some(InstrumentEditEvent::EnvelopeFlagsChanged(*env_type, new_flags));
                }
                let mut sustain_flag = env.flags.sustain;
                if ui.checkbox(&mut sustain_flag, "Sustain").changed() {
                    let mut new_flags = env.flags;
                    new_flags.sustain = sustain_flag;
                    event = Some(InstrumentEditEvent::EnvelopeFlagsChanged(*env_type, new_flags));
                }
                let mut carry = env.flags.carry;
                if ui.checkbox(&mut carry, "Carry").changed() {
                    let mut new_flags = env.flags;
                    new_flags.carry = carry;
                    event = Some(InstrumentEditEvent::EnvelopeFlagsChanged(*env_type, new_flags));
                }
            });
            egui::Frame::group(ui.style()).inner_margin(frame_margin).show(ui, |ui| {
                let sustain_str = match env.sustain_point {
                    Some(idx) if idx < env.points.len() => format!("Sus: p{}", idx),
                    _ => "Sus: --".to_string(),
                };
                ui.label(egui::RichText::new(sustain_str).color(theme.fg_dim));
                if ui.button("Set").clicked() {
                    if let Some(idx) = hv {
                        if idx < env.points.len() {
                            event = Some(InstrumentEditEvent::EnvelopeSustainChanged(*env_type, Some(idx)));
                        }
                    }
                }
                if ui.button("Clr").clicked() {
                    event = Some(InstrumentEditEvent::EnvelopeSustainChanged(*env_type, None));
                }
            });
            egui::Frame::group(ui.style()).inner_margin(frame_margin).show(ui, |ui| {
                let loop_label = if env.flags.loop_ { "Loop On" } else { "Loop Off" };
                if ui.selectable_label(env.flags.loop_, loop_label).clicked() {
                    let new_enabled = !env.flags.loop_;
                    if new_enabled {
                        let ls = env.loop_start.or_else(|| {
                            if !env.points.is_empty() { Some(0) } else { None }
                        });
                        let le = env.loop_end.or_else(|| {
                            if env.points.len() > 1 { Some(env.points.len() - 1) } else { None }
                        });
                        event = Some(InstrumentEditEvent::EnvelopeLoopChanged(*env_type, true, ls, le));
                    } else {
                        event = Some(InstrumentEditEvent::EnvelopeLoopChanged(*env_type, false, env.loop_start, env.loop_end));
                    }
                }
                if env.flags.loop_ {
                    if ui.button("LSt").clicked() {
                        if let Some(idx) = hv {
                            if idx < env.points.len() {
                                event = Some(InstrumentEditEvent::EnvelopeLoopChanged(*env_type, true, Some(idx), env.loop_end));
                            }
                        }
                    }
                    if ui.button("LEn").clicked() {
                        if let Some(idx) = hv {
                            if idx < env.points.len() {
                                event = Some(InstrumentEditEvent::EnvelopeLoopChanged(*env_type, true, env.loop_start, Some(idx)));
                            }
                        }
                    }
                }
            });
            egui::Frame::group(ui.style()).inner_margin(frame_margin).show(ui, |ui| {
                if ui.button("+Point").clicked() {
                    let last_tick = env.points.last().map(|p| p.tick).unwrap_or(0);
                    let new_tick = last_tick.saturating_add(16).min(255);
                    event = Some(InstrumentEditEvent::EnvelopePointAdded(*env_type, new_tick, 32));
                }
                if ui.button(egui::RichText::new("Generate").color(theme.fg_volume)).clicked() {
                    *generator_open = true;
                }
            });
        });

        // Envelope graph
        let env_type_idx = match env_type {
            EnvelopeType::Volume => 0,
            EnvelopeType::Panning => 1,
            EnvelopeType::Pitch => 2,
            EnvelopeType::Filter => 3,
        };
        let env_positions = playback_state.env_positions_for_instrument(
            env_type_idx,
            selected_instrument as u8,
        );
        let env_resp = crate::ui::envelope_editor::draw_envelope_editor(ui, env, *env_type, &env_positions, theme);
        ui.data_mut(|d| d.insert_temp(env_hovered_id, env_resp.hovered_point));
        if let Some(env_event) = env_resp.event {
            match env_event {
                crate::ui::envelope_editor::EnvelopeEditEvent::PointMoved(idx, t, v) => {
                    event = Some(InstrumentEditEvent::EnvelopePointMoved(*env_type, idx, t, v));
                }
                crate::ui::envelope_editor::EnvelopeEditEvent::PointAdded(t, v) => {
                    event = Some(InstrumentEditEvent::EnvelopePointAdded(*env_type, t, v));
                }
                crate::ui::envelope_editor::EnvelopeEditEvent::PointRemoved(idx) => {
                    event = Some(InstrumentEditEvent::EnvelopePointRemoved(*env_type, idx));
                }
            }
        }
    } else {
        ui.horizontal(|ui| {
            ui.label(format!("{:?} Envelope — not created", env_type));
            if ui.button("Create Envelope").clicked() {
                event = Some(InstrumentEditEvent::EnvelopePointAdded(*env_type, 0, 64));
            }
        });
    }

    event
}

#[derive(Clone, Copy, PartialEq)]
enum EnvGeneratorShape {
    Sine,
    Square,
    Triangle,
    SawUp,
    SawDown,
    Pulse,
    Random,
}

fn generate_envelope_points(
    shape: EnvGeneratorShape,
    length: u16,
    cycles: f32,
    depth: u8,
    offset: u8,
    duty: f32,
) -> Vec<crate::sequencer::instrument::EnvelopePoint> {
    use crate::sequencer::instrument::EnvelopePoint;
    use std::f32::consts::TAU;

    let length_f = length as f32;
    let depth_f = depth as f32;
    let offset_f = offset as f32;
    let num_cycles = cycles.max(0.25);

    if length < 2 {
        return vec![EnvelopePoint { tick: 0, value: offset }];
    }

    let clamp_val = |v: f32| -> u8 { v.clamp(0.0, 64.0) as u8 };

    match shape {
        EnvGeneratorShape::Sine => {
            let pts_per_cycle = 16usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize + 1;
            let mut points = Vec::with_capacity(total_pts);
            for i in 0..total_pts {
                let t = (i as f32 / (total_pts - 1) as f32) * length_f;
                let phase = num_cycles * TAU * t / length_f;
                let v = offset_f + (depth_f / 2.0) * phase.sin();
                points.push(EnvelopePoint {
                    tick: t.round() as u16,
                    value: clamp_val(v),
                });
            }
            points
        }
        EnvGeneratorShape::Square => {
            let pts_per_cycle = 4usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 1);
            let hi = offset_f + depth_f / 2.0;
            let lo = offset_f - depth_f / 2.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                let mid = (cycle_start + cycle_end) / 2.0;
                points.push(EnvelopePoint { tick: cycle_start.round() as u16, value: clamp_val(hi) });
                points.push(EnvelopePoint { tick: mid.round() as u16, value: clamp_val(hi) });
                points.push(EnvelopePoint { tick: mid.round() as u16, value: clamp_val(lo) });
                points.push(EnvelopePoint { tick: cycle_end.round() as u16, value: clamp_val(lo) });
            }
            points.sort_by_key(|p| p.tick);
            // unique by tick
            points.dedup_by_key(|p| p.tick);
            points
        }
        EnvGeneratorShape::Triangle => {
            let pts_per_cycle = 4usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 1);
            let hi = offset_f + depth_f / 2.0;
            let lo = offset_f - depth_f / 2.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                let mid = (cycle_start + cycle_end) / 2.0;
                points.push(EnvelopePoint { tick: cycle_start.round() as u16, value: clamp_val(lo) });
                points.push(EnvelopePoint { tick: mid.round() as u16, value: clamp_val(hi) });
                points.push(EnvelopePoint { tick: cycle_end.round() as u16, value: clamp_val(lo) });
            }
            points.sort_by_key(|p| p.tick);
            points.dedup_by_key(|p| p.tick);
            points
        }
        EnvGeneratorShape::SawUp => {
            let pts_per_cycle = 2usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 2);
            let hi = offset_f + depth_f / 2.0;
            let lo = offset_f - depth_f / 2.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                points.push(EnvelopePoint { tick: cycle_start.round() as u16, value: clamp_val(lo) });
                points.push(EnvelopePoint { tick: cycle_end.round() as u16, value: clamp_val(hi) });
            }
            points.sort_by_key(|p| p.tick);
            points.dedup_by_key(|p| p.tick);
            points
        }
        EnvGeneratorShape::SawDown => {
            let pts_per_cycle = 2usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 2);
            let hi = offset_f + depth_f / 2.0;
            let lo = offset_f - depth_f / 2.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                points.push(EnvelopePoint { tick: cycle_start.round() as u16, value: clamp_val(hi) });
                points.push(EnvelopePoint { tick: cycle_end.round() as u16, value: clamp_val(lo) });
            }
            points.sort_by_key(|p| p.tick);
            points.dedup_by_key(|p| p.tick);
            points
        }
        EnvGeneratorShape::Pulse => {
            let pts_per_cycle = 4usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 1);
            let hi = offset_f + depth_f / 2.0;
            let lo = offset_f - depth_f / 2.0;
            let duty_f = duty / 100.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                let transition = cycle_start + (cycle_end - cycle_start) * duty_f;
                points.push(EnvelopePoint { tick: cycle_start.round() as u16, value: clamp_val(hi) });
                points.push(EnvelopePoint { tick: transition.round() as u16, value: clamp_val(hi) });
                points.push(EnvelopePoint { tick: transition.round() as u16, value: clamp_val(lo) });
                points.push(EnvelopePoint { tick: cycle_end.round() as u16, value: clamp_val(lo) });
            }
            points.sort_by_key(|p| p.tick);
            points.dedup_by_key(|p| p.tick);
            points
        }
        EnvGeneratorShape::Random => {
            let step = (length_f / (num_cycles * 8.0)).max(1.0).round() as u16;
            let num_pts = (length as u16 / step).max(2) as usize;
            let mut points = Vec::with_capacity(num_pts);
            let mut cur = offset_f;
            let half_depth = depth_f / 2.0;
            let mut seed: u32 = (length as u32) ^ (cycles as u32).wrapping_mul(12345) ^ (depth as u32) * 6789;
            for i in 0..num_pts {
                let t = ((i as f32 / (num_pts - 1) as f32) * length_f).round() as u16;
                points.push(EnvelopePoint { tick: t, value: clamp_val(cur) });
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let delta = ((seed as f32) / (u32::MAX as f32) - 0.5) * half_depth * 0.5;
                cur = (cur + delta).clamp(offset_f - half_depth, offset_f + half_depth);
            }
            points
        }
    }
}

fn draw_envelope_generator_popup(
    ctx: &egui::Context,
    env_type: crate::edit::EnvelopeType,
    open: &mut bool,
) -> Option<Vec<crate::sequencer::instrument::EnvelopePoint>> {
    use crate::sequencer::instrument::EnvelopePoint;

    let popup_id = format!("envelope_generator_{:?}", env_type);
    let id_salt = egui::Id::new(&popup_id);

    let mut result = None;
    let mut should_close = false;

    egui::Window::new("Generate Envelope")
        .id(id_salt)
        .open(open)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Envelope:");
                ui.strong(match env_type {
                    crate::edit::EnvelopeType::Volume => "Volume",
                    crate::edit::EnvelopeType::Panning => "Panning",
                    crate::edit::EnvelopeType::Pitch => "Pitch",
                    crate::edit::EnvelopeType::Filter => "Filter",
                });
            });

            let shape_id = ui.make_persistent_id("gen_shape");
            let mut shape_idx = ui.data(|d| d.get_temp::<usize>(shape_id).unwrap_or(0));
            let length_id = ui.make_persistent_id("gen_length");
            let mut length = ui.data(|d| d.get_temp::<u16>(length_id).unwrap_or(256));
            let cycles_id = ui.make_persistent_id("gen_cycles");
            let mut cycles = ui.data(|d| d.get_temp::<f32>(cycles_id).unwrap_or(1.0));
            let depth_id = ui.make_persistent_id("gen_depth");
            let mut depth = ui.data(|d| d.get_temp::<u8>(depth_id).unwrap_or(48));
            let offset_id = ui.make_persistent_id("gen_offset");
            let mut offset = ui.data(|d| d.get_temp::<u8>(offset_id).unwrap_or(32));
            let duty_id = ui.make_persistent_id("gen_duty");
            let mut duty = ui.data(|d| d.get_temp::<f32>(duty_id).unwrap_or(50.0));

            let shapes = ["Sine", "Square", "Triangle", "Saw Up", "Saw Down", "Pulse", "Random"];
            egui::ComboBox::from_id_salt("gen_shape_combo")
                .selected_text(shapes[shape_idx])
                .show_ui(ui, |ui| {
                    for (i, name) in shapes.iter().enumerate() {
                        if ui.selectable_label(shape_idx == i, *name).clicked() {
                            shape_idx = i;
                        }
                    }
                });

            egui::Grid::new("gen_grid").show(ui, |ui| {
                ui.label("Length:");
                ui.add(egui::Slider::new(&mut length, 32..=4096));
                ui.end_row();

                ui.label("Cycles:");
                ui.add(egui::Slider::new(&mut cycles, 0.25..=64.0).step_by(0.25));
                ui.end_row();

                ui.label("Depth:");
                ui.add(egui::Slider::new(&mut depth, 1..=64));
                ui.end_row();

                ui.label("Offset:");
                ui.add(egui::Slider::new(&mut offset, 0..=64));
                ui.end_row();

                if shape_idx == 5 {
                    ui.label("Duty %:");
                    ui.add(egui::Slider::new(&mut duty, 5.0..=95.0).step_by(1.0));
                    ui.end_row();
                }
            });

            // Recompute offset if depth would push it out of range
            let half_depth = depth as f32 / 2.0;
            if (offset as f32) - half_depth < 0.0 {
                offset = half_depth as u8;
            }
            if (offset as f32) + half_depth > 64.0 {
                offset = 64 - half_depth as u8;
            }

            let shape = match shape_idx {
                0 => EnvGeneratorShape::Sine,
                1 => EnvGeneratorShape::Square,
                2 => EnvGeneratorShape::Triangle,
                3 => EnvGeneratorShape::SawUp,
                4 => EnvGeneratorShape::SawDown,
                5 => EnvGeneratorShape::Pulse,
                _ => EnvGeneratorShape::Random,
            };

            // preview
            let preview_points = generate_envelope_points(shape, length, cycles, depth, offset, duty);
            ui.add_space(4.0);
            ui.label("Preview:");
            let (preview_rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().min(400.0), 80.0),
                egui::Sense::hover(),
            );
            if !preview_points.is_empty() {
                let max_tick = preview_points.last().map(|p| p.tick).unwrap_or(256) as f32;
                let max_val = 64.0;
                let painter = ui.painter_at(preview_rect);
                let to_screen = |p: &EnvelopePoint| {
                    let x = preview_rect.left() + (p.tick as f32 / max_tick) * preview_rect.width();
                    let y = preview_rect.bottom() - (p.value as f32 / max_val) * preview_rect.height();
                    egui::pos2(x, y)
                };
                // fill
                let mut fill_points: Vec<egui::Pos2> = Vec::new();
                fill_points.push(egui::pos2(preview_rect.left(), preview_rect.bottom()));
                for p in &preview_points {
                    fill_points.push(to_screen(p));
                }
                fill_points.push(egui::pos2(preview_rect.right(), preview_rect.bottom()));
                painter.add(egui::Shape::convex_polygon(
                    fill_points,
                    ui.visuals().widgets.noninteractive.bg_fill.gamma_multiply(0.3),
                    egui::Stroke::NONE,
                ));
                // line
                if preview_points.len() > 1 {
                    let line_pts: Vec<egui::Pos2> = preview_points.iter().map(to_screen).collect();
                    painter.add(egui::Shape::line(
                        line_pts,
                        egui::Stroke::new(1.5, ui.visuals().widgets.noninteractive.fg_stroke.color),
                    ));
                }
                // points
                for p in &preview_points {
                    let pos = to_screen(p);
                    painter.circle_filled(pos, 2.0, ui.visuals().widgets.noninteractive.fg_stroke.color);
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    let points = generate_envelope_points(shape, length, cycles, depth, offset, duty);
                    if !points.is_empty() {
                        result = Some(points);
                    }
                    should_close = true;
                }
                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });

            ui.data_mut(|d| {
                d.insert_temp(shape_id, shape_idx);
                d.insert_temp(length_id, length);
                d.insert_temp(cycles_id, cycles);
                d.insert_temp(depth_id, depth);
                d.insert_temp(offset_id, offset);
                d.insert_temp(duty_id, duty);
            });
        });

    if should_close {
        *open = false;
    }

    result
}
