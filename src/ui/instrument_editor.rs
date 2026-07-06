use eframe::egui;
use crate::audio::engine::CommandSender;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::audio::plugins::HostedPluginHandle;
use crate::edit::EnvelopeType;
use crate::sequencer::instrument::{EnvelopeFlags, EnvelopePoint, Instrument};
use crate::sequencer::Module;
use crate::ui::style::FONT_TITLE;
use crate::ui::TrackerTheme;
use eguidev::DevUiExt;

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
    PluginUnload,
    /// Open the loaded CLAP instrument plugin's editor (floating window).
    /// The in-app parameter sliders (drawn in `handle_instrument_tab`)
    /// provide the procedural GUI; the embedded native-GUI mode was removed.
    OpenPluginEditor,
    /// Close the loaded CLAP instrument plugin's editor window.
    ClosePluginEditor,
}

pub fn draw_instrument_editor(
    ui: &mut egui::Ui,
    module: &Module,
    selected_instrument: &mut usize,
    selected_sample: &mut usize,
    theme: &TrackerTheme,
    playback_state: &AtomicPlaybackState,
    instrument_editor: &mut crate::ui::instrument_editor_panel::InstrumentEditor,
    config: &mut crate::app_config::AppConfig,
    plugin_handle: Option<&dyn HostedPluginHandle>,
    command_sender: &mut Option<CommandSender>,
) -> Option<InstrumentEditEvent> {
    let mut event = None;

    let paint_sample_id = ui.make_persistent_id("sample_map_paint_idx");
    let mut paint_sample_idx = ui.data(|d| d.get_temp::<u8>(paint_sample_id).unwrap_or(0));
    let browser_open_id = ui.make_persistent_id("sample_browser_open");
    let mut browser_open = ui.data(|d| d.get_temp::<bool>(browser_open_id).unwrap_or(false));
    let prev_inst_id = ui.make_persistent_id("prev_selected_instrument");
    let prev_inst = ui.data(|d| d.get_temp::<usize>(prev_inst_id).unwrap_or(0));
    let reset_palette_scroll = *selected_instrument != prev_inst;
    ui.data_mut(|d| d.insert_temp(prev_inst_id, *selected_instrument));

    let generator_open_id = ui.make_persistent_id("env_generator_open");
    let mut generator_open = ui.data(|d| d.get_temp::<bool>(generator_open_id).unwrap_or(false));
    let mut env_type = instrument_editor.envelope_type;
    let mut env_visible = instrument_editor.envelope_visible;

    let _total_w = ui.available_width();
    let total_h = ui.available_height();

    // 1. Instrument List (Side Panel)
    let list_width = instrument_editor.list_width;
    let list_panel_resp = egui::Panel::left("instrument_list_panel")
        .resizable(true)
        .size_range(100.0..=400.0)
        .default_size(list_width)
        .show_inside(ui, |ui| {
            ui.vertical(|ui| {
                ui.heading("Instruments");
                egui::ScrollArea::vertical()
                    .id_salt("instrument_list_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let max_idx = module.instruments.len();
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
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 18.0),
                                egui::Sense::click(),
                            );

                            let painter = ui.painter_at(rect);
                            if is_selected {
                                painter.rect_filled(rect, 0.0, theme.bg_selected);
                            }

                            painter.text(
                                egui::pos2(rect.left() + 4.0, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                &label,
                                egui::FontId::proportional(14.0),
                                label_color,
                            );

                            if response.clicked() {
                                *selected_instrument = i;
                                if let Some(inst) = module.instruments.get(i) {
                                    if let Some(s) = inst.sample_map.iter().copied().find(|&s| s > 0) {
                                        *selected_sample = s as usize;
                                    }
                                }
                            }
                            
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
                            ui.painter_at(rect).line_segment(
                                [egui::pos2(rect.left() + 2.0, rect.bottom()), egui::pos2(rect.right() - 2.0, rect.bottom())],
                                egui::Stroke::new(1.0, theme.grid_line_minor),
                            );
                        }
                    });
            });
        });
    instrument_editor.list_width = list_panel_resp.response.rect.width();
    config.instrument_list_width = Some(instrument_editor.list_width);

    // 2. Envelope Editor (Bottom Panel)
    if env_visible {
        let env_height = instrument_editor.envelope_height;
        let env_panel_resp = egui::Panel::bottom("instrument_envelope_panel")
            .resizable(true)
            .size_range(100.0..=total_h * 0.8)
            .default_size(env_height)
            .show_inside(ui, |ui| {
                if let Some(inst) = module.instruments.get(*selected_instrument) {
                    if let Some(e) = draw_envelope_section(
                        ui, inst, &mut env_type, theme, playback_state,
                        *selected_instrument, &mut generator_open,
                    ) {
                        event = Some(e);
                    }
                }
            });
        instrument_editor.envelope_height = env_panel_resp.response.rect.height();
        config.instrument_envelope_height = Some(instrument_editor.envelope_height);
    }

    // 3. Central Panel (Settings & Maps)
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme.bg_default))
        .show_inside(ui, |ui| {
            if let Some(inst) = module.instruments.get(*selected_instrument) {
                // Header
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("INSTRUMENT {:02X}", *selected_instrument)).strong().size(FONT_TITLE));
                    let mut name = inst.name.clone();
                    if ui.dev_text_edit("inst.header.name", &mut name).changed() {
                        event = Some(InstrumentEditEvent::NameChanged(name));
                    }
                    ui.dev_separator("inst.header.sep");
                    if ui.dev_button("inst.header.save", "Save...").clicked() {
                        event = Some(InstrumentEditEvent::SaveInstrument);
                    }
                    if ui.dev_button("inst.header.load", "Load...").clicked() {
                        event = Some(InstrumentEditEvent::LoadInstrument);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.selectable_label(env_visible, "Show Envelopes").clicked() {
                            env_visible = !env_visible;
                        }
                    });
                });
                ui.separator();

                // ── CLAP Plugin slot — prominent, above the scroll area ──
                egui::Frame::group(ui.style())
                    .fill(theme.status_bg)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("CLAP Instrument:").strong());
                            match plugin_handle {
                                Some(h) => ui.label(
                                    egui::RichText::new(h.descriptor().name.as_str())
                                        .color(theme.fg_instrument),
                                ),
                                None => ui.label(egui::RichText::new("(none — sample-based)").weak()),
                            }
                        });
                        if let Some(handle) = plugin_handle {
                            ui.horizontal(|ui| {
                                if handle.has_editor() {
                                    if handle.is_editor_open() {
                                        if ui.button("Close Editor").clicked() {
                                            event = Some(InstrumentEditEvent::ClosePluginEditor);
                                        }
                                        let mode_label = match handle.editor_mode() {
                                            Some(crate::audio::plugins::EditorMode::Floating) => "Floating",
                                            None => "",
                                        };
                                        ui.label(
                                            egui::RichText::new(mode_label)
                                                .weak()
                                                .size(10.0),
                                        );
                                    } else {
                                        // "Edit..." opens the plugin's own
                                        // floating window. The parameter sliders
                                        // below provide the procedural GUI.
                                        if ui.button("Edit...").clicked() {
                                            event = Some(InstrumentEditEvent::OpenPluginEditor);
                                        }
                                    }
                                    if let Some(err) = handle.last_editor_error() {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(255, 100, 100),
                                            err,
                                        );
                                    }
                                }
                                if ui.button("Unload").clicked() {
                                    event = Some(InstrumentEditEvent::PluginUnload);
                                }
                            });

                            // Procedural parameter GUI — one slider per
                            // exposed parameter. Replaces the old embedded
                            // native-GUI view. Values are queued to the
                            // plugin's param ring and mirrored to the
                            // audio-thread processor via SetInstrumentPluginParam.
                            let inst_idx = *selected_instrument;
                            crate::ui::sendfx_editor::draw_plugin_parameter_sliders(
                                ui,
                                handle,
                                |param_id, value| {
                                    if let Some(ref mut sender) = command_sender {
                                        sender.send(
                                            crate::audio::commands::AudioCommand::SetInstrumentPluginParam {
                                                instrument_idx: inst_idx,
                                                param_id,
                                                value,
                                            },
                                        );
                                    }
                                },
                            );
                        } else {
                            ui.horizontal(|ui| {
                                if ui.button("Load CLAP Plugin…").clicked() {
                                    instrument_editor.plugin_browser_open = true;
                                }
                            });
                        }
                    });
                ui.add_space(4.0);

                    egui::ScrollArea::vertical()
                    .id_salt("instrument_central_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(e) = draw_settings_grid(ui, inst, theme) {
                            event = Some(e);
                        }
                        ui.add_space(8.0);
                        
                        // Wrap the mapping section in its own vertical scroll if needed, 
                        // or just let the main central scroll handle it.
                        if let Some(e) = draw_maps_row(ui, inst, theme, module, &mut paint_sample_idx, &mut browser_open, playback_state, config, reset_palette_scroll) {
                            event = Some(e);
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
        if let Some(points) = draw_envelope_generator_popup(ui.ctx(), env_type, &mut generator_open, theme) {
            event = Some(InstrumentEditEvent::GenerateEnvelope(env_type, points));
        }
    }

    instrument_editor.envelope_type = env_type;
    instrument_editor.envelope_visible = env_visible;
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
) -> Option<InstrumentEditEvent> {
    let mut event = None;

    ui.columns(2, |columns| {
        let left_event = {
            let mut ev = None;
            crate::ui::draw_group(&mut columns[0], "General", theme, |ui| {
                egui::Grid::new("gen_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    ui.dev_label("inst.general.vol_label", "Global Vol:");
                    let mut gvol = inst.global_volume as f32;
                    if ui.dev_slider("inst.general.vol", &mut gvol, 0.0..=128.0).changed() {
                        ev = Some(InstrumentEditEvent::GlobalVolumeChanged(gvol as u8));
                    }
                    ui.end_row();

                    ui.label("Fadeout:");
                    let mut fade = inst.fade_out as i32;
                    if ui.dev_drag_value_i32_range("inst.general.fade", &mut fade, 0..=4095).changed() {
                        ev = Some(InstrumentEditEvent::FadeoutChanged(fade as u16));
                    }
                    ui.end_row();
                });
            });
            crate::ui::draw_group(&mut columns[0], "Pitch-Pan", theme, |ui| {
                egui::Grid::new("pp_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    ui.label("Separation:");
                    let mut sep = inst.pitch_pan_separation as f32;
                    if ui.dev_slider("inst.pitchpan.sep", &mut sep, -32.0..=32.0).changed() {
                        ev = Some(InstrumentEditEvent::PitchPanSeparationChanged(sep as i8));
                    }
                    ui.end_row();

                    ui.label("Center:");
                    let mut center = inst.pitch_pan_center as i32;
                    if ui.dev_drag_value_i32_range("inst.pitchpan.center", &mut center, 0..=119).changed() {
                        ev = Some(InstrumentEditEvent::PitchPanCenterChanged(center as u8));
                    }
                    ui.end_row();
                });
            });
            crate::ui::draw_group(&mut columns[0], "Filter", theme, |ui| {
                egui::Grid::new("filter_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    ui.label("Cutoff:");
                    let mut cutoff = inst.filter_cutoff as i32;
                    if ui.dev_drag_value_i32_range("inst.filter.cutoff", &mut cutoff, 0..=65535).changed() {
                        ev = Some(InstrumentEditEvent::FilterCutoffChanged(cutoff as u16));
                    }
                    ui.end_row();

                    ui.label("Resonance:");
                    let mut res = inst.filter_resonance as f32;
                    if ui.dev_slider("inst.filter.res", &mut res, 0.0..=255.0).changed() {
                        ev = Some(InstrumentEditEvent::FilterResonanceChanged(res as u8));
                    }
                    ui.end_row();

                    ui.label("Type:");
                    let ft = inst.filter_type;
                    let mut ft_u8 = ft.to_u8();
                    egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(3, 1)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.dev_selectable_value("inst.filter.type.lp", &mut ft_u8, 0u8, "LP");
                            ui.dev_selectable_value("inst.filter.type.hp", &mut ft_u8, 1u8, "HP");
                            ui.dev_selectable_value("inst.filter.type.bp", &mut ft_u8, 2u8, "BP");
                            ui.dev_selectable_value("inst.filter.type.notch", &mut ft_u8, 3u8, "Notch");
                        });
                    });
                    let new_ft = crate::sequencer::effect::FilterType::from_u8(ft_u8);
                    if new_ft != ft {
                        ev = Some(InstrumentEditEvent::FilterTypeChanged(new_ft));
                    }
                    ui.end_row();
                });
            });
            ev
        };

        let right_event = {
            let mut ev = None;
            crate::ui::draw_group(&mut columns[1], "NNA", theme, |ui| {
                egui::Grid::new("nna_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    ui.label("Action:");
                    use crate::sequencer::instrument::{NewNoteAction, DuplicateCheckType, DuplicateCheckAction};
                    let mut nna = inst.nna;
                    egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(3, 1)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.dev_selectable_value("inst.nna.cut", &mut nna, NewNoteAction::NoteCut, "Cut");
                            ui.dev_selectable_value("inst.nna.cont", &mut nna, NewNoteAction::Continue, "Cont");
                            ui.dev_selectable_value("inst.nna.off", &mut nna, NewNoteAction::NoteOff, "Off");
                            ui.dev_selectable_value("inst.nna.fade", &mut nna, NewNoteAction::NoteFade, "Fade");
                        });
                    });
                    if nna != inst.nna {
                        ev = Some(InstrumentEditEvent::NnaChanged(nna));
                    }
                    ui.end_row();

                    ui.dev_label("inst.nna.dct_label", "DCT:");
                    let mut dct = inst.duplicate_check_type;
                    egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(3, 1)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.dev_selectable_value("inst.nna.dct.off", &mut dct, DuplicateCheckType::Disabled, "Off");
                            ui.dev_selectable_value("inst.nna.dct.note", &mut dct, DuplicateCheckType::Note, "Note");
                            ui.dev_selectable_value("inst.nna.dct.samp", &mut dct, DuplicateCheckType::Sample, "Samp");
                            ui.dev_selectable_value("inst.nna.dct.inst", &mut dct, DuplicateCheckType::Instrument, "Inst");
                        });
                    });
                    if dct != inst.duplicate_check_type {
                        ev = Some(InstrumentEditEvent::DuplicateCheckTypeChanged(dct));
                    }
                    ui.end_row();

                    ui.dev_label("inst.nna.dna_label", "DNA:");
                    let mut dna = inst.duplicate_check_action;
                    egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(3, 1)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.dev_selectable_value("inst.nna.dna.cut", &mut dna, DuplicateCheckAction::NoteCut, "Cut");
                            ui.dev_selectable_value("inst.nna.dna.off", &mut dna, DuplicateCheckAction::NoteOff, "Off");
                            ui.dev_selectable_value("inst.nna.dna.fade", &mut dna, DuplicateCheckAction::NoteFade, "Fade");
                        });
                    });
                    if dna != inst.duplicate_check_action {
                        ev = Some(InstrumentEditEvent::DuplicateCheckActionChanged(dna));
                    }
                    ui.end_row();
                });
            });
            crate::ui::draw_group(&mut columns[1], "Random Variation", theme, |ui| {
                egui::Grid::new("rand_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    ui.label("Volume:");
                    let mut rvol = inst.random_volume as f32;
                    if ui.dev_slider("inst.random.vol", &mut rvol, 0.0..=100.0).changed() {
                        ev = Some(InstrumentEditEvent::RandomVolumeChanged(rvol as u8));
                    }
                    ui.end_row();

                    ui.label("Panning:");
                    let mut rpan = inst.random_panning as f32;
                    if ui.dev_slider("inst.random.pan", &mut rpan, 0.0..=100.0).changed() {
                        ev = Some(InstrumentEditEvent::RandomPanningChanged(rpan as u8));
                    }
                    ui.end_row();

                    ui.label("Filter Cut:");
                    let mut frc = inst.filter_random_cutoff as f32;
                    if ui.dev_slider("inst.random.flt", &mut frc, 0.0..=255.0).changed() {
                        ev = Some(InstrumentEditEvent::FilterRandomCutoffChanged(frc as u8));
                    }
                    ui.end_row();
                });
            });
            crate::ui::draw_group(&mut columns[1], "Vibrato", theme, |ui| {
                egui::Grid::new("vib_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    ui.label("Type:");
                    let mut vib_type = inst.vib_type;
                    egui::Frame::group(ui.style()).inner_margin(egui::Margin::symmetric(3, 1)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.dev_selectable_value("inst.vib.type.sine", &mut vib_type, 0u8, "Sine");
                            ui.dev_selectable_value("inst.vib.type.ramp", &mut vib_type, 1u8, "Ramp");
                            ui.dev_selectable_value("inst.vib.type.sq", &mut vib_type, 2u8, "Sq");
                            ui.dev_selectable_value("inst.vib.type.rand", &mut vib_type, 3u8, "Rand");
                        });
                    });
                    if vib_type != inst.vib_type {
                        ev = Some(InstrumentEditEvent::VibTypeChanged(vib_type));
                    }
                    ui.end_row();

                    ui.label("Sweep:");
                    let mut sweep = inst.vib_sweep as i32;
                    if ui.dev_drag_value_i32_range("inst.vib.sweep", &mut sweep, 0..=255).changed() {
                        ev = Some(InstrumentEditEvent::VibSweepChanged(sweep as u8));
                    }
                    ui.end_row();

                    ui.label("Depth:");
                    let mut depth = inst.vib_depth as i32;
                    if ui.dev_drag_value_i32_range("inst.vib.depth", &mut depth, 0..=255).changed() {
                        ev = Some(InstrumentEditEvent::VibDepthChanged(depth as u8));
                    }
                    ui.end_row();

                    ui.label("Rate:");
                    let mut rate = inst.vib_rate as i32;
                    if ui.dev_drag_value_i32_range("inst.vib.rate", &mut rate, 0..=255).changed() {
                        ev = Some(InstrumentEditEvent::VibRateChanged(rate as u8));
                    }
                    ui.end_row();
                });
            });
            ev
        };

        if left_event.is_some() {
            event = left_event;
        }
        if right_event.is_some() {
            event = right_event;
        }
    });

    event
}

fn draw_maps_row(
    ui: &mut egui::Ui,
    inst: &Instrument,
    theme: &TrackerTheme,
    module: &Module,
    paint_sample_idx: &mut u8,
    browser_open: &mut bool,
    playback_state: &AtomicPlaybackState,
    config: &mut crate::app_config::AppConfig,
    reset_palette_scroll: bool,
) -> Option<InstrumentEditEvent> {
    let mut event = None;

    ui.add_space(4.0);
    ui.vertical(|ui| {
        // --- Sample Map ---
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Sample Map").strong());
                ui.add_space(8.0);
                if ui.button(" - ").on_hover_text("Shrink grid").clicked() {
                    config.map_cell_size = (config.map_cell_size - 2.0).max(16.0);
                }
                if ui.button(" + ").on_hover_text("Grow grid").clicked() {
                    config.map_cell_size = (config.map_cell_size + 2.0).min(80.0);
                }
                ui.separator();
                if ui.dev_button("inst.map.browse", "Browse...").clicked() {
                    *browser_open = true;
                }
                ui.separator();
                if ui.dev_button("inst.map.fill_all", "Fill All").clicked() {
                    event = Some(InstrumentEditEvent::SampleMapFillAll(*paint_sample_idx));
                }
            });
            ui.add_space(4.0);
            crate::ui::sample_palette::draw_inline_sample_palette(ui, module, paint_sample_idx, playback_state, theme, reset_palette_scroll);
            if let Some(map_event) = crate::ui::sample_map::draw_sample_map(
                ui, &inst.sample_map, *paint_sample_idx, module, config.map_cell_size, theme,
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
                    crate::ui::sample_map::SampleMapEvent::NoteDropped(note, sample) => {
                        if inst.sample_map[note as usize] != sample {
                            event = Some(InstrumentEditEvent::SampleMapChanged(note, sample));
                        }
                    }
                }
            }
        });

        ui.add_space(8.0);

        // --- Note Map (Transpose) ---
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Note Map (Transpose)").strong());
                ui.add_space(8.0);
                ui.separator();
            });
            ui.add_space(4.0);
            if let Some(nm_event) = crate::ui::note_map::draw_note_map(ui, &inst.note_map, *paint_sample_idx, theme, config.map_cell_size) {
                if inst.note_map[nm_event.note as usize] != nm_event.new_dest {
                    event = Some(InstrumentEditEvent::NoteMapChanged(nm_event.note, nm_event.new_dest));
                }
            }
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

    // Envelope tabs with status indicators
    egui::Frame::NONE
        .fill(theme.status_bg)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .corner_radius(egui::CornerRadius::same(4))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("instrument_envelope_tabs_scroll")
                .auto_shrink([false, true])
                .show(ui, |ui| {
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

                    ui.dev_selectable_value("inst.env.tab.vol", env_type, EnvelopeType::Volume, egui::RichText::new(format!("{} Vol ({})", vol_ind, vol_pts)).color(env_colors[0].0));
                    ui.dev_selectable_value("inst.env.tab.pan", env_type, EnvelopeType::Panning, egui::RichText::new(format!("{} Pan ({})", pan_ind, pan_pts)).color(env_colors[1].0));
                    ui.dev_selectable_value("inst.env.tab.pit", env_type, EnvelopeType::Pitch, egui::RichText::new(format!("{} Pitch ({})", pit_ind, pit_pts)).color(env_colors[2].0));
                    ui.dev_selectable_value("inst.env.tab.flt", env_type, EnvelopeType::Filter, egui::RichText::new(format!("{} Flt ({})", flt_ind, flt_pts)).color(env_colors[3].0));
                });
            });
        });

    let envelope = inst.envelope(*env_type);

    let env_hovered_id = ui.make_persistent_id("env_hovered");

    if let Some(ref env) = envelope {
        // Envelope controls
        let hv = ui.data(|d| d.get_temp::<Option<usize>>(env_hovered_id).flatten());
        let frame_margin = egui::Margin::symmetric(5, 0);
        egui::ScrollArea::horizontal()
            .id_salt("instrument_envelope_controls_scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
            ui.horizontal(|ui| {
            egui::Frame::group(ui.style()).inner_margin(frame_margin).show(ui, |ui| {
                let mut enabled = env.flags.enabled;
                if ui.dev_checkbox("inst.env.enabled", &mut enabled, "Enabled").changed() {
                    let mut new_flags = env.flags;
                    new_flags.enabled = enabled;
                    event = Some(InstrumentEditEvent::EnvelopeFlagsChanged(*env_type, new_flags));
                }
                let mut sustain_flag = env.flags.sustain;
                if ui.dev_checkbox("inst.env.sustain", &mut sustain_flag, "Sustain").changed() {
                    let mut new_flags = env.flags;
                    new_flags.sustain = sustain_flag;
                    event = Some(InstrumentEditEvent::EnvelopeFlagsChanged(*env_type, new_flags));
                }
                let mut carry = env.flags.carry;
                if ui.dev_checkbox("inst.env.carry", &mut carry, "Carry").changed() {
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
                ui.dev_label("inst.env.sus_display", egui::RichText::new(sustain_str).color(theme.fg_dim));
                if ui.dev_button("inst.env.sus_set", "Set").clicked() {
                    if let Some(idx) = hv {
                        if idx < env.points.len() {
                            event = Some(InstrumentEditEvent::EnvelopeSustainChanged(*env_type, Some(idx)));
                        }
                    }
                }
                if ui.dev_button("inst.env.sus_clr", "Clr").clicked() {
                    event = Some(InstrumentEditEvent::EnvelopeSustainChanged(*env_type, None));
                }
            });
            egui::Frame::group(ui.style()).inner_margin(frame_margin).show(ui, |ui| {
                let mut loop_on = env.flags.loop_;
                if ui.dev_checkbox("inst.env.loop", &mut loop_on, "Loop").changed() {
                    if loop_on {
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
                    if ui.dev_button("inst.env.loop_start", "LSt").clicked() {
                        if let Some(idx) = hv {
                            if idx < env.points.len() {
                                event = Some(InstrumentEditEvent::EnvelopeLoopChanged(*env_type, true, Some(idx), env.loop_end));
                            }
                        }
                    }
                    if ui.dev_button("inst.env.loop_end", "LEn").clicked() {
                        if let Some(idx) = hv {
                            if idx < env.points.len() {
                                event = Some(InstrumentEditEvent::EnvelopeLoopChanged(*env_type, true, env.loop_start, Some(idx)));
                            }
                        }
                    }
                }
            });
            egui::Frame::group(ui.style()).inner_margin(frame_margin).show(ui, |ui| {
                if ui.dev_button("inst.env.add_point", "+Point").clicked() {
                    let last_tick = env.points.last().map(|p| p.tick).unwrap_or(0);
                    let new_tick = last_tick.saturating_add(16).min(255);
                    event = Some(InstrumentEditEvent::EnvelopePointAdded(*env_type, new_tick, 32));
                }
                if ui.dev_button("inst.env.generate", egui::RichText::new("Generate").color(theme.fg_volume)).clicked() {
                    *generator_open = true;
                }
            });
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
            ui.dev_label("inst.env.empty", format!("{:?} Envelope — not created", env_type));
            if ui.dev_button("inst.env.create", "Create Envelope").clicked() {
                event = Some(InstrumentEditEvent::EnvelopePointAdded(*env_type, 0, 64));
            }
        });
    }

    event
}

fn generate_envelope_points(
    shape: crate::sequencer::envelope_generator::GeneratorShape,
    length: u16,
    cycles: f32,
    depth: u8,
    offset: u8,
    duty: f32,
) -> Vec<crate::sequencer::instrument::EnvelopePoint> {
    let depth_f = depth as f32 / 64.0;
    let offset_f = offset as f32 / 64.0;
    let pairs = crate::sequencer::envelope_generator::generate_values(shape, length, cycles, depth_f, offset_f, duty);
    pairs.into_iter().map(|(_pos, val)| {
        crate::sequencer::instrument::EnvelopePoint {
            tick: _pos,
            value: (val * 64.0).round() as u8,
        }
    }).collect()
}

fn draw_envelope_generator_popup(
    ctx: &egui::Context,
    env_type: crate::edit::EnvelopeType,
    open: &mut bool,
    theme: &TrackerTheme,
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
                ui.label(egui::RichText::new("Envelope:").color(theme.fg_dim));
                ui.strong(egui::RichText::new(match env_type {
                    crate::edit::EnvelopeType::Volume => "Volume",
                    crate::edit::EnvelopeType::Panning => "Panning",
                    crate::edit::EnvelopeType::Pitch => "Pitch",
                    crate::edit::EnvelopeType::Filter => "Filter",
                }).color(theme.fg_text));
            });

            ui.add_space(4.0);
            section_header(ui, "Shape", theme);

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

            ui.add_space(4.0);
            section_header(ui, "Parameters", theme);
            ui.add_space(2.0);
            egui::Grid::new("gen_grid").show(ui, |ui| {
                ui.label(egui::RichText::new("Length:").color(theme.fg_dim));
                ui.add(egui::Slider::new(&mut length, 32..=4096));
                ui.end_row();

                ui.label(egui::RichText::new("Cycles:").color(theme.fg_dim));
                ui.add(egui::Slider::new(&mut cycles, 0.25..=64.0).step_by(0.25));
                ui.end_row();

                ui.label(egui::RichText::new("Depth:").color(theme.fg_dim));
                ui.add(egui::Slider::new(&mut depth, 1..=64));
                ui.end_row();

                ui.label(egui::RichText::new("Offset:").color(theme.fg_dim));
                ui.add(egui::Slider::new(&mut offset, 0..=64));
                ui.end_row();

                if shape_idx == 5 {
                    ui.label(egui::RichText::new("Duty %:").color(theme.fg_dim));
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
                0 => crate::sequencer::envelope_generator::GeneratorShape::Sine,
                1 => crate::sequencer::envelope_generator::GeneratorShape::Square,
                2 => crate::sequencer::envelope_generator::GeneratorShape::Triangle,
                3 => crate::sequencer::envelope_generator::GeneratorShape::SawUp,
                4 => crate::sequencer::envelope_generator::GeneratorShape::SawDown,
                5 => crate::sequencer::envelope_generator::GeneratorShape::Pulse,
                _ => crate::sequencer::envelope_generator::GeneratorShape::Random,
            };

            // preview
            let preview_points = generate_envelope_points(shape, length, cycles, depth, offset, duty);
            ui.add_space(4.0);
            section_header(ui, "Preview", theme);
            ui.add_space(2.0);
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
                    theme.bg_highlight.gamma_multiply(0.3),
                    egui::Stroke::NONE,
                ));
                // line
                if preview_points.len() > 1 {
                    let line_pts: Vec<egui::Pos2> = preview_points.iter().map(to_screen).collect();
                    painter.add(egui::Shape::line(
                        line_pts,
                        egui::Stroke::new(1.5, theme.fg_instrument),
                    ));
                }
                // points
                for p in &preview_points {
                    let pos = to_screen(p);
                    painter.circle_filled(pos, 2.0, theme.fg_instrument);
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

fn section_header(ui: &mut egui::Ui, text: &str, theme: &TrackerTheme) {
    super::style::section_header(ui, text, theme);
}
