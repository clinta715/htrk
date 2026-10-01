use super::*;

impl HtrkApp {
    pub(super) fn draw_dialogs(&mut self, ctx: &egui::Context) {
        if self.show_shortcuts {
            crate::ui::help_screen::draw_shortcuts_window(
                ctx,
                &mut self.show_shortcuts,
                &self.theme,
            );
        }

        if self.settings_state.open {
            let action = crate::ui::settings_window::draw_settings_window(
                ctx,
                &mut self.settings_state,
                &self.output_device_names,
                self.selected_device_name.as_deref(),
                &self.theme,
            );
            match action {
                crate::ui::settings_window::SettingsAction::Save
                | crate::ui::settings_window::SettingsAction::Apply => {
                    self.settings_state.apply_to_config(&mut self.config);
                    self.config.save();
                    self.apply_config_to_live_state();
                    self.pending_reinit = true;
                }
                crate::ui::settings_window::SettingsAction::Cancel => {
                    self.settings_state =
                        crate::ui::settings_window::SettingsState::from_config(&self.config);
                }
                crate::ui::settings_window::SettingsAction::RefreshDevices => {
                    self.refresh_output_devices();
                }
                crate::ui::settings_window::SettingsAction::SelectDevice(name) => {
                    self.pending_device_switch = Some(name);
                }
                crate::ui::settings_window::SettingsAction::None => {}
            }
        }

        if crate::ui::wav_export_window::draw_wav_export(ctx, &mut self.wav_export_state) {
            crate::actions::export_wav_with_settings(self);
        }

        if self.show_about {
            egui::Window::new("About Holofonic Tracker")
                .open(&mut self.show_about)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .default_width(350.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading(concat!("Holofonic Tracker v", env!("CARGO_PKG_VERSION")));
                        ui.add_space(8.0);
                        ui.label("A modern tracker / music sequencer");
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Built with Rust + egui + cpal")
                                .size(11.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(12.0);
                        ui.label("Supports .htk, .it, .xm, .s3m, .mod formats");
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("Development guided by Clint Anderson")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(180, 180, 200)),
                        );
                        ui.label(
                            egui::RichText::new("clinta@gmail.com")
                                .size(10.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("AI-assisted code from GLM, DeepSeek, and MiniMax")
                                .size(10.0)
                                .color(egui::Color32::GRAY),
                        );
                        ui.add_space(8.0);
                    });
                });
        }

        if self.show_exit_confirm {
            let mut open = true;
            egui::Window::new("Unsaved Changes")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .default_width(400.0)
                .show(ctx, |ui| {
                    ui.label("The current project has unsaved changes.");
                    ui.label("Do you want to save before exiting?");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            crate::actions::save_current_file(self);
                            self.show_exit_confirm = false;
                            if !self.core.module_dirty() {
                                self.exit_confirmed = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            } else {
                                self.close_after_save = true;
                            }
                        }
                        if ui.button("Don't Save").clicked() {
                            self.show_exit_confirm = false;
                            self.exit_confirmed = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_exit_confirm = false;
                        }
                    });
                });
            if !open {
                self.show_exit_confirm = false;
            }
        }

        if self.show_phrase_generator {
            let num_ch = self.core.num_channels_checked();
            let num_rows = self.core.current_pattern().map_or(64, |p| p.num_rows);
            let cursor_ch = self.core.cursor.channel;
            if let Some(params) = crate::ui::phrase_generator_dialog::draw_phrase_generator(
                ctx,
                &mut self.show_phrase_generator,
                &self.theme,
                num_ch,
                num_rows,
                cursor_ch,
                &mut self.phrase_gen_state,
            ) {
                let (start_row, end_row) = match &self.core.selection {
                    Some(sel) => {
                        let (min, max) = sel.normalized();
                        (min.row, max.row)
                    }
                    None => (0, num_rows.saturating_sub(1)),
                };
                let notes = crate::tools::phrase_generator::generate_phrase(
                    &params, start_row, end_row, num_ch,
                );
                if !notes.is_empty() {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut m) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(m) {
                            let pat_idx = *arc_module
                                .order_list
                                .get(self.core.selected_order)
                                .unwrap_or(&0) as usize;
                            if pat_idx < arc_module.patterns.len() {
                                let pattern = &arc_module.patterns[pat_idx];
                                let mut old_cells = Vec::new();
                                let mut new_cells = Vec::new();
                                for &(row, ch, cell) in &notes {
                                    if row < pattern.num_rows
                                        && ch < crate::sequencer::pattern::MAX_CHANNELS
                                    {
                                        old_cells.push((row, ch, pattern.data[row][ch]));
                                        new_cells.push((row, ch, cell));
                                    }
                                }
                                let cmd = Box::new(crate::edit::BulkSetCellsCommand {
                                    order: self.core.selected_order,
                                    old_cells,
                                    new_cells,
                                });
                                let _ = self.core.undo_manager.execute(cmd, arc_module);
                            }
                        }
                    }
                    self.core.sync_module_to_audio();
                }
            }
        }

        if let Some(ref mut dialog) = self.sample_export_dialog {
            if let Some((path, bit_depth)) = dialog.show(ctx) {
                let sample_idx = dialog.sample_index;
                if let Some(ref module) = self.core.module {
                    if let Some(sample) = module.samples.get(sample_idx) {
                        let wav_data = crate::formats::wav::export_wav(sample, bit_depth);
                        if let Err(e) = std::fs::write(&path, wav_data) {
                            eprintln!("Failed to write sample: {}", e);
                        } else {
                            if let Some(parent) = path.parent() {
                                self.config.default_wav_path =
                                    Some(parent.to_string_lossy().into_owned());
                            }
                            self.config.set_sample_export_bit_depth(bit_depth);
                        }
                    }
                }
                self.sample_export_dialog = None;
            }
        }

        if self.slice_dialog_open {
            let mut open = true;
            let source_sample = self.slice_config.source_sample;
            let has_module = self.core.module.is_some();

            egui::Window::new("Slice to Instrument")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .default_size([320.0, 380.0])
                .show(ctx, |ui| {
                    use crate::actions::slice_to_instrument::SliceMode;

                    let config = &mut self.slice_config;
                    let theme = &self.theme;

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Source:").color(theme.fg_dim));
                        if let Some(ref module) = self.core.module {
                            ui.label(
                                module
                                    .samples
                                    .get(config.source_sample)
                                    .map(|s| {
                                        if s.name.is_empty() {
                                            format!("Sample {:02X}", config.source_sample)
                                        } else {
                                            s.name.clone()
                                        }
                                    })
                                    .unwrap_or_default(),
                            );
                        }
                    });

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Mode")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(100, 200, 255)),
                    );

                    let modes = ["Time Divisions", "Onset Detection"];
                    let mut mode_idx = if config.mode == SliceMode::TimeDivisions {
                        0
                    } else {
                        1
                    };
                    egui::ComboBox::from_id_salt("slice_mode")
                        .selected_text(modes[mode_idx])
                        .show_ui(ui, |ui| {
                            for (i, name) in modes.iter().enumerate() {
                                if ui.selectable_label(mode_idx == i, *name).clicked() {
                                    mode_idx = i;
                                    config.mode = if i == 0 {
                                        SliceMode::TimeDivisions
                                    } else {
                                        SliceMode::Onsets
                                    };
                                }
                            }
                        });

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Parameters")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(100, 200, 255)),
                    );
                    ui.add_space(2.0);

                    match config.mode {
                        SliceMode::TimeDivisions => {
                            egui::Grid::new("slice_time_grid")
                                .num_columns(2)
                                .spacing([8.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("BPM:").color(theme.fg_dim));
                                    ui.add(egui::Slider::new(&mut config.bpm, 30.0..=300.0));
                                    ui.end_row();

                                    ui.label(egui::RichText::new("Division:").color(theme.fg_dim));
                                    let divisions = [4u8, 8, 16, 32];
                                    let div_names = ["1/4", "1/8", "1/16", "1/32"];
                                    let mut div_idx = divisions
                                        .iter()
                                        .position(|d| *d == config.division)
                                        .unwrap_or(2);
                                    egui::ComboBox::from_id_salt("slice_division")
                                        .selected_text(div_names[div_idx])
                                        .show_ui(ui, |ui| {
                                            for (i, name) in div_names.iter().enumerate() {
                                                if ui
                                                    .selectable_label(div_idx == i, *name)
                                                    .clicked()
                                                {
                                                    div_idx = i;
                                                    config.division = divisions[i];
                                                }
                                            }
                                        });
                                    ui.end_row();
                                });
                        }
                        SliceMode::Onsets => {
                            egui::Grid::new("slice_onset_grid")
                                .num_columns(2)
                                .spacing([8.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("Sensitivity:").color(theme.fg_dim),
                                    );
                                    ui.add(egui::Slider::new(&mut config.sensitivity, 0.05..=1.0));
                                    ui.end_row();

                                    ui.label(
                                        egui::RichText::new("Min Spacing:").color(theme.fg_dim),
                                    );
                                    ui.add(
                                        egui::Slider::new(&mut config.min_spacing_ms, 5.0..=500.0)
                                            .suffix("ms"),
                                    );
                                    ui.end_row();
                                });
                        }
                    }

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Instrument")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(100, 200, 255)),
                    );
                    ui.add_space(2.0);
                    egui::Grid::new("slice_inst_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Base Note:").color(theme.fg_dim));
                            let note_names = [
                                "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#",
                                "B-",
                            ];
                            let oct = config.base_note / 12;
                            let note = config.base_note % 12;
                            let note_str = format!("{}{}", note_names[note as usize], oct);
                            egui::ComboBox::from_id_salt("slice_base_note")
                                .selected_text(&note_str)
                                .show_ui(ui, |ui| {
                                    for o in 0..10u8 {
                                        for n in 0..12u8 {
                                            let midi = o * 12 + n;
                                            let name = format!("{}{}", note_names[n as usize], o);
                                            let selected = config.base_note == midi;
                                            if ui.selectable_label(selected, &name).clicked() {
                                                config.base_note = midi;
                                            }
                                        }
                                    }
                                });
                            ui.end_row();

                            ui.label(egui::RichText::new("Target:").color(theme.fg_dim));
                            if let Some(ref module) = self.core.module {
                                let mut inst_names: Vec<String> = module
                                    .instruments
                                    .iter()
                                    .enumerate()
                                    .map(|(i, inst)| {
                                        if inst.name.is_empty() {
                                            format!("Inst {:02X}", i)
                                        } else {
                                            format!("{:02X}:{}", i, inst.name)
                                        }
                                    })
                                    .collect();
                                inst_names[0] = "---".to_string();
                                inst_names.push("+ Create New".to_string());

                                let current =
                                    config
                                        .target_instrument
                                        .map_or(inst_names.len() - 1, |idx| {
                                            if idx < module.instruments.len() {
                                                idx
                                            } else {
                                                inst_names.len() - 1
                                            }
                                        });

                                let mut sel = current;
                                egui::ComboBox::from_id_salt("slice_target_inst")
                                    .selected_text(&inst_names[sel])
                                    .show_ui(ui, |ui| {
                                        for (i, name) in inst_names.iter().enumerate() {
                                            if ui
                                                .selectable_label(sel == i, name.as_str())
                                                .clicked()
                                            {
                                                sel = i;
                                                if i < module.instruments.len() {
                                                    config.target_instrument = Some(i);
                                                } else {
                                                    config.target_instrument = None;
                                                }
                                            }
                                        }
                                    });
                            }
                            ui.end_row();
                        });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() && has_module {
                            self.core.ensure_module_ownership();
                            if let Some(ref mut module) = self.core.module {
                                if let Some(arc_module) = Arc::get_mut(module) {
                                    // Capture pre-state
                                    let target_inst =
                                        config.target_instrument.unwrap_or_else(|| {
                                            let mut free = 1usize;
                                            while free < arc_module.instruments.len()
                                                && !arc_module.instruments[free].name.is_empty()
                                                && arc_module.instruments[free]
                                                    .sample_map
                                                    .iter()
                                                    .any(|&s| s != 0)
                                            {
                                                free += 1;
                                            }
                                            if free >= arc_module.instruments.len()
                                                && arc_module.instruments.len()
                                                    < crate::sequencer::module::MAX_INSTRUMENTS
                                            {
                                                arc_module
                                                    .instruments
                                                    .push(crate::sequencer::Instrument::default());
                                            }
                                            free.min(arc_module.instruments.len().saturating_sub(1))
                                        });

                                    let pre_sample_count = arc_module.samples.len();
                                    let pre_instrument_name =
                                        arc_module.instruments[target_inst].name.clone();
                                    let pre_sample_map =
                                        arc_module.instruments[target_inst].sample_map;

                                    let mut slice_config = config.clone();
                                    slice_config.target_instrument = Some(target_inst);

                                    match crate::actions::slice_to_instrument::compute_slices(
                                        arc_module,
                                        &slice_config,
                                    ) {
                                        Ok((slice_samples, result)) => {
                                            let cmd =
                                                Box::new(crate::edit::SliceToInstrumentCommand {
                                                    target_instrument: result.target_instrument,
                                                    pre_sample_count,
                                                    pre_instrument_name,
                                                    pre_sample_map,
                                                    slice_samples,
                                                    post_name: format!("Sliced: {}", {
                                                        arc_module
                                                            .samples
                                                            .get(source_sample)
                                                            .map(|s| s.name.as_str())
                                                            .unwrap_or("")
                                                    }),
                                                    post_base_note: config.base_note,
                                                    post_slice_count: result.slice_count as u8,
                                                });
                                            let _ = self.core.undo_manager.execute(cmd, arc_module);
                                            self.core.sync_module_to_audio();
                                            self.slice_dialog_open = false;
                                        }
                                        Err(e) => {
                                            eprintln!("Slice error: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.slice_dialog_open = false;
                        }
                    });
                });
            if !open {
                self.slice_dialog_open = false;
            }
        }

        if self.file_browser.show {
            let mut file_browser_open = true;

            egui::Window::new("File Browser")
                .open(&mut file_browser_open)
                .default_size([550.0, 400.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(true)
                .show(ctx, |ui| {
                    if let Some(path) =
                        self.file_browser
                            .render(ui, Some(&mut self.config), self.theme.clone())
                    {
                        let path_str = path.to_string_lossy().to_string();
                        match self.browser_purpose {
                            BrowserPurpose::General => {
                                let ext = path
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                if ext == "wav" {
                                    crate::actions::import_wav(self, &path_str);
                                } else if ext == "mid" || ext == "midi" {
                                    crate::actions::import_midi(self, &path_str);
                                } else {
                                    crate::actions::load_file(self, &path_str);
                                }
                            }
                            BrowserPurpose::LoadInstrument => {
                                crate::actions::load_instrument_from_file(self, &path_str);
                            }
                            BrowserPurpose::SaveInstrument => {
                                let inst_idx = self.core.selected_instrument;
                                crate::actions::save_instrument_to_file(self, inst_idx, &path_str);
                            }
                            BrowserPurpose::ExportInstrument(idx) => {
                                crate::actions::save_instrument_to_file(self, idx, &path_str);
                            }
                            BrowserPurpose::SaveProject => {
                                self.core.save_file(&path_str);
                                crate::actions::add_recent_file(self, &path_str);
                            }
                        }
                        self.browser_purpose = BrowserPurpose::General;
                    }
                });
            if self.file_browser.preview_requested {
                self.file_browser.preview_requested = false;
                self.preview_browser_sample(60);
            }
            if !file_browser_open {
                self.file_browser.close();
            }
        }

        // ── Sample Library dialog ──
        if self.sample_library_state.open {
            if let Some(module) = self.core.module.as_ref() {
                let import_path = crate::ui::sample_library::draw_sample_library(
                    ctx,
                    &mut self.sample_library_state,
                    &self.sample_library,
                    module,
                    &self.theme,
                );
                if let Some(path) = import_path {
                    self.import_sample_from_library(&path);
                }
            }
        }

        if self.close_after_save && !self.core.module_dirty() {
            self.close_after_save = false;
            self.exit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        crate::actions::check_auto_backup(self);

        ctx.request_repaint();
    }
}
