use eframe::egui;
use crate::edit::EnvelopeType;
use crate::sequencer::instrument::EnvelopeFlags;
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
    NoteMapChanged(u8, u8),
    SampleMapChanged(u8, u8),
    SampleMapFillAll(u8),
    VibTypeChanged(u8),
    VibSweepChanged(u8),
    VibDepthChanged(u8),
    VibRateChanged(u8),
    SaveInstrument,
    LoadInstrument,
}

pub fn draw_instrument_editor(
    ui: &mut egui::Ui,
    module: &Module,
    selected_instrument: &mut usize,
    _theme: &TrackerTheme,
) -> Option<InstrumentEditEvent> {
    let mut event = None;

    let paint_sample_id = ui.make_persistent_id("sample_map_paint_idx");
    let mut paint_sample_idx = ui.data(|d| d.get_temp::<u8>(paint_sample_id).unwrap_or(0));
    let browser_open_id = ui.make_persistent_id("sample_browser_open");
    let mut browser_open = ui.data(|d| d.get_temp::<bool>(browser_open_id).unwrap_or(false));

    let env_type_id = ui.make_persistent_id("instrument_env_type");
    let mut env_type = ui.data(|d| d.get_temp::<crate::edit::EnvelopeType>(env_type_id).unwrap_or(crate::edit::EnvelopeType::Volume));

    ui.horizontal(|ui| {
        // Instrument List
        ui.vertical(|ui| {
            ui.set_width(150.0);
            ui.set_height(ui.available_height());
            ui.heading("Instruments");
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                for i in 1..module.instruments.len().max(100) {
                    if let Some(inst) = module.instruments.get(i) {
                        let name = &inst.name;
                        let label = format!("{:02}: {}", i, if name.is_empty() { "Untitled" } else { name });
                        if ui.selectable_label(*selected_instrument == i, label).clicked() {
                            *selected_instrument = i;
                        }
                    } else {
                        let label = format!("{:02}: (empty)", i);
                        if ui.selectable_label(*selected_instrument == i, label).clicked() {
                            *selected_instrument = i;
                        }
                    }
                }
            });
        });

        ui.separator();

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

                // ---- Envelope Editor ----
                ui.group(|ui| {
                    ui.set_min_height(250.0);

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

                        if ui.selectable_label(env_type == EnvelopeType::Volume, format!("Vol {}({})", if vol_active { "[ON]" } else { "[OFF]" }, vol_pts)).clicked() {
                            env_type = EnvelopeType::Volume;
                            ui.data_mut(|d| d.insert_temp(env_type_id, env_type));
                        }
                        if ui.selectable_label(env_type == EnvelopeType::Panning, format!("Pan {}({})", if pan_active { "[ON]" } else { "[OFF]" }, pan_pts)).clicked() {
                            env_type = EnvelopeType::Panning;
                            ui.data_mut(|d| d.insert_temp(env_type_id, env_type));
                        }
                        if ui.selectable_label(env_type == EnvelopeType::Pitch, format!("Pitch {}({})", if pit_active { "[ON]" } else { "[OFF]" }, pit_pts)).clicked() {
                            env_type = EnvelopeType::Pitch;
                            ui.data_mut(|d| d.insert_temp(env_type_id, env_type));
                        }
                        if ui.selectable_label(env_type == EnvelopeType::Filter, format!("Flt {}({})", if flt_active { "[ON]" } else { "[OFF]" }, flt_pts)).clicked() {
                            env_type = EnvelopeType::Filter;
                            ui.data_mut(|d| d.insert_temp(env_type_id, env_type));
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
                        ui.horizontal(|ui| {
                            let mut enabled = env.flags.enabled;
                            if ui.checkbox(&mut enabled, "Enabled").changed() {
                                let mut new_flags = env.flags;
                                new_flags.enabled = enabled;
                                event = Some(InstrumentEditEvent::EnvelopeFlagsChanged(env_type, new_flags));
                            }
                            let mut sustain_flag = env.flags.sustain;
                            if ui.checkbox(&mut sustain_flag, "Sustain").changed() {
                                let mut new_flags = env.flags;
                                new_flags.sustain = sustain_flag;
                                event = Some(InstrumentEditEvent::EnvelopeFlagsChanged(env_type, new_flags));
                            }
                            let mut carry = env.flags.carry;
                            if ui.checkbox(&mut carry, "Carry").changed() {
                                let mut new_flags = env.flags;
                                new_flags.carry = carry;
                                event = Some(InstrumentEditEvent::EnvelopeFlagsChanged(env_type, new_flags));
                            }
                            ui.separator();
                            let sustain_str = match env.sustain_point {
                                Some(idx) if idx < env.points.len() => format!("Sustain: p{}", idx),
                                _ => "Sustain: --".to_string(),
                            };
                            ui.label(sustain_str);
                            let hv = ui.data(|d| d.get_temp::<Option<usize>>(env_hovered_id).flatten());
                            if ui.button("Set Sus").clicked() {
                                if let Some(idx) = hv {
                                    if idx < env.points.len() {
                                        event = Some(InstrumentEditEvent::EnvelopeSustainChanged(env_type, Some(idx)));
                                    }
                                }
                            }
                            if ui.button("Clr Sus").clicked() {
                                event = Some(InstrumentEditEvent::EnvelopeSustainChanged(env_type, None));
                            }
                            ui.separator();
                            let loop_label = if env.flags.loop_ { "Loop: On" } else { "Loop: Off" };
                            if ui.button(loop_label).clicked() {
                                let new_enabled = !env.flags.loop_;
                                if new_enabled {
                                    let ls = env.loop_start.or_else(|| {
                                        if !env.points.is_empty() { Some(0) } else { None }
                                    });
                                    let le = env.loop_end.or_else(|| {
                                        if env.points.len() > 1 { Some(env.points.len() - 1) } else { None }
                                    });
                                    event = Some(InstrumentEditEvent::EnvelopeLoopChanged(env_type, true, ls, le));
                                } else {
                                    event = Some(InstrumentEditEvent::EnvelopeLoopChanged(env_type, false, env.loop_start, env.loop_end));
                                }
                            }
                            if env.flags.loop_ {
                                if ui.button("Set LSt").clicked() {
                                    if let Some(idx) = hv {
                                        if idx < env.points.len() {
                                            event = Some(InstrumentEditEvent::EnvelopeLoopChanged(env_type, true, Some(idx), env.loop_end));
                                        }
                                    }
                                }
                                if ui.button("Set LEn").clicked() {
                                    if let Some(idx) = hv {
                                        if idx < env.points.len() {
                                            event = Some(InstrumentEditEvent::EnvelopeLoopChanged(env_type, true, env.loop_start, Some(idx)));
                                        }
                                    }
                                }
                            }
                            ui.separator();
                            if ui.button("+Point").clicked() {
                                let last_tick = env.points.last().map(|p| p.tick).unwrap_or(0);
                                let new_tick = last_tick.saturating_add(16).min(255);
                                event = Some(InstrumentEditEvent::EnvelopePointAdded(env_type, new_tick, 32));
                            }
                        });

                        // Envelope graph
                        let env_resp = crate::ui::envelope_editor::draw_envelope_editor(ui, env, env_type);
                        ui.data_mut(|d| d.insert_temp(env_hovered_id, env_resp.hovered_point));
                        if let Some(env_event) = env_resp.event {
                            match env_event {
                                crate::ui::envelope_editor::EnvelopeEditEvent::PointMoved(idx, t, v) => {
                                    event = Some(InstrumentEditEvent::EnvelopePointMoved(env_type, idx, t, v));
                                }
                                crate::ui::envelope_editor::EnvelopeEditEvent::PointAdded(t, v) => {
                                    event = Some(InstrumentEditEvent::EnvelopePointAdded(env_type, t, v));
                                }
                                crate::ui::envelope_editor::EnvelopeEditEvent::PointRemoved(idx) => {
                                    event = Some(InstrumentEditEvent::EnvelopePointRemoved(env_type, idx));
                                }
                            }
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(format!("{:?} Envelope — not created", env_type));
                            if ui.button("Create Envelope").clicked() {
                                event = Some(InstrumentEditEvent::EnvelopePointAdded(env_type, 0, 64));
                            }
                        });
                    }
                });

                // ---- Bottom properties: two rows ----
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
                        egui::Grid::new(format!("instrument_nna_{}", *selected_instrument)).show(ui, |ui| {
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
                        egui::Grid::new(format!("instrument_vols_{}", *selected_instrument)).show(ui, |ui| {
                            ui.label("Global Vol:");
                            let mut gvol = inst.global_volume;
                            if ui.add(egui::Slider::new(&mut gvol, 0..=128)).changed() {
                                event = Some(InstrumentEditEvent::GlobalVolumeChanged(gvol));
                            }
                            ui.end_row();

                            ui.label("Fadeout:");
                            let mut fade = inst.fade_out;
                            if ui.add(egui::DragValue::new(&mut fade).range(0..=4095)).changed() {
                                event = Some(InstrumentEditEvent::FadeoutChanged(fade));
                            }
                            ui.end_row();

                            ui.label("Pitch-Pan Sep:");
                            let mut sep = inst.pitch_pan_separation;
                            if ui.add(egui::Slider::new(&mut sep, -32..=32)).changed() {
                                event = Some(InstrumentEditEvent::PitchPanSeparationChanged(sep));
                            }
                            ui.end_row();

                            ui.label("Pitch-Pan Center:");
                            let mut center = inst.pitch_pan_center;
                            if ui.add(egui::DragValue::new(&mut center).range(0..=119)).changed() {
                                event = Some(InstrumentEditEvent::PitchPanCenterChanged(center));
                            }
                            ui.end_row();

                            ui.label("Random Vol:");
                            let mut rvol = inst.random_volume;
                            if ui.add(egui::Slider::new(&mut rvol, 0..=100)).changed() {
                                event = Some(InstrumentEditEvent::RandomVolumeChanged(rvol));
                            }
                            ui.end_row();

                            ui.label("Random Pan:");
                            let mut rpan = inst.random_panning;
                            if ui.add(egui::Slider::new(&mut rpan, 0..=100)).changed() {
                                event = Some(InstrumentEditEvent::RandomPanningChanged(rpan));
                            }
                            ui.end_row();

                            ui.label("Filter Cutoff:");
                            let mut cutoff = inst.filter_cutoff;
                            if ui.add(egui::DragValue::new(&mut cutoff).range(0..=0xFFFF)).changed() {
                                event = Some(InstrumentEditEvent::FilterCutoffChanged(cutoff));
                            }
                            ui.end_row();

                            ui.label("Filter Res:");
                            let mut res = inst.filter_resonance;
                            if ui.add(egui::Slider::new(&mut res, 0..=255)).changed() {
                                event = Some(InstrumentEditEvent::FilterResonanceChanged(res));
                            }
                            ui.end_row();

                            ui.label("Filter Rand Cut:");
                            let mut frc = inst.filter_random_cutoff;
                            if ui.add(egui::Slider::new(&mut frc, 0..=255)).changed() {
                                event = Some(InstrumentEditEvent::FilterRandomCutoffChanged(frc));
                            }
                            ui.end_row();

                            ui.label("Filter Type:");
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
                        egui::Grid::new(format!("instrument_vib_{}", *selected_instrument)).show(ui, |ui| {
                            ui.label("Type:");
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

                            ui.label("Sweep:");
                            let mut sweep = inst.vib_sweep;
                            if ui.add(egui::DragValue::new(&mut sweep).range(0..=255).speed(1)).changed() {
                                event = Some(InstrumentEditEvent::VibSweepChanged(sweep));
                            }
                            ui.end_row();

                            ui.label("Depth:");
                            let mut depth = inst.vib_depth;
                            if ui.add(egui::DragValue::new(&mut depth).range(0..=255).speed(1)).changed() {
                                event = Some(InstrumentEditEvent::VibDepthChanged(depth));
                            }
                            ui.end_row();

                            ui.label("Rate:");
                            let mut rate = inst.vib_rate;
                            if ui.add(egui::DragValue::new(&mut rate).range(0..=255).speed(1)).changed() {
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
                                browser_open = true;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("Fill All").clicked() {
                                    event = Some(InstrumentEditEvent::SampleMapFillAll(paint_sample_idx));
                                }
                            });
                        });

                        crate::ui::sample_palette::draw_inline_sample_palette(ui, module, &mut paint_sample_idx);

                        if let Some(map_event) = crate::ui::sample_map::draw_sample_map(
                            ui,
                            &inst.sample_map,
                            paint_sample_idx,
                            module,
                        ) {
                            match map_event {
                                crate::ui::sample_map::SampleMapEvent::NoteClicked(note) |
                                crate::ui::sample_map::SampleMapEvent::NoteDragged(note) => {
                                    if inst.sample_map[note as usize].saturating_sub(1) != paint_sample_idx {
                                        event = Some(InstrumentEditEvent::SampleMapChanged(note, paint_sample_idx));
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
                        if let Some(nm_event) = crate::ui::note_map::draw_note_map(ui, &inst.note_map, paint_sample_idx) {
                            if inst.note_map[nm_event.note as usize] != nm_event.new_dest {
                                event = Some(InstrumentEditEvent::NoteMapChanged(nm_event.note, nm_event.new_dest));
                            }
                        }
                    });
                });
                });
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
        ) {
            paint_sample_idx = idx;
        }
    }

    ui.data_mut(|d| {
        d.insert_temp(paint_sample_id, paint_sample_idx);
        d.insert_temp(browser_open_id, browser_open);
    });

    event
}
