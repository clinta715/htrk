use eframe::egui;

use crate::tools::phrase_generator::{self, ChordType, GenMode, PhraseParams, Progression};
use crate::tools::scale::{Scale, ROOT_NAMES};
use super::theme::TrackerTheme;

pub fn draw_phrase_generator(
    ctx: &egui::Context,
    open: &mut bool,
    theme: &TrackerTheme,
    num_channels: usize,
    num_rows: usize,
    _cursor_ch: usize,
) -> Option<PhraseParams> {
    let mut result = None;
    let mut should_close = false;

    egui::Window::new("Generate Phrase")
        .id(egui::Id::new("phrase_generator"))
        .open(open)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let mode_id = ui.make_persistent_id("phr_mode");
            let mut mode_idx = ui.data(|d| d.get_temp::<usize>(mode_id).unwrap_or(0));
            let scale_id = ui.make_persistent_id("phr_scale");
            let mut scale_idx = ui.data(|d| d.get_temp::<usize>(scale_id).unwrap_or(0));
            let root_id = ui.make_persistent_id("phr_root");
            let mut root = ui.data(|d| d.get_temp::<usize>(root_id).unwrap_or(0));
            let oct_min_id = ui.make_persistent_id("phr_oct_min");
            let mut oct_min = ui.data(|d| d.get_temp::<u8>(oct_min_id).unwrap_or(3));
            let oct_max_id = ui.make_persistent_id("phr_oct_max");
            let mut oct_max = ui.data(|d| d.get_temp::<u8>(oct_max_id).unwrap_or(5));
            let density_id = ui.make_persistent_id("phr_density");
            let mut density = ui.data(|d| d.get_temp::<f32>(density_id).unwrap_or(0.3));
            let step_id = ui.make_persistent_id("phr_step");
            let mut step_size = ui.data(|d| d.get_temp::<u8>(step_id).unwrap_or(3));
            let pulses_id = ui.make_persistent_id("phr_pulses");
            let mut pulses = ui.data(|d| d.get_temp::<usize>(pulses_id).unwrap_or(8));
            let rotation_id = ui.make_persistent_id("phr_rotation");
            let mut rotation = ui.data(|d| d.get_temp::<usize>(rotation_id).unwrap_or(0));
            let inst_id = ui.make_persistent_id("phr_inst");
            let mut instr_str: String = ui.data(|d| d.get_temp::<String>(inst_id).unwrap_or_default());
            let seed_id = ui.make_persistent_id("phr_seed");
            let mut seed_str: String = ui.data(|d| d.get_temp::<String>(seed_id).unwrap_or_else(|| "0".to_string()));
            let kick_id = ui.make_persistent_id("phr_kick");
            let mut kick_ch = ui.data(|d| d.get_temp::<usize>(kick_id).unwrap_or(0));
            let snare_id = ui.make_persistent_id("phr_snare");
            let mut snare_ch = ui.data(|d| d.get_temp::<usize>(snare_id).unwrap_or(1));
            let hat_id = ui.make_persistent_id("phr_hat");
            let mut hat_ch = ui.data(|d| d.get_temp::<usize>(hat_id).unwrap_or(2));
            let chord_type_id = ui.make_persistent_id("phr_chord_type");
            let mut chord_type_idx = ui.data(|d| d.get_temp::<usize>(chord_type_id).unwrap_or(0));
            let progression_id = ui.make_persistent_id("phr_progression");
            let mut progression_idx = ui.data(|d| d.get_temp::<usize>(progression_id).unwrap_or(0));
            let bars_per_chord_id = ui.make_persistent_id("phr_bars_per_chord");
            let mut bars_per_chord = ui.data(|d| d.get_temp::<u8>(bars_per_chord_id).unwrap_or(4));

            let all_scales = Scale::all();
            let mode = GenMode::all().get(mode_idx).copied().unwrap_or(GenMode::Melodic);
            let scale = all_scales.get(scale_idx).copied().unwrap_or(Scale::Major);

            let mut drum_warning = String::new();

            section_header(ui, "Mode");
            let modes = GenMode::all();
            egui::ComboBox::from_id_salt("phr_mode_combo")
                .selected_text(modes[mode_idx].name())
                .show_ui(ui, |ui| {
                    for (i, m) in modes.iter().enumerate() {
                        if ui.selectable_label(mode_idx == i, m.name()).clicked() {
                            mode_idx = i;
                        }
                    }
                });

            ui.add_space(4.0);
            section_header(ui, "Scale");
            ui.add_space(2.0);
            egui::ComboBox::from_id_salt("phr_scale_combo")
                .selected_text(scale.name())
                .show_ui(ui, |ui| {
                    for (i, s) in all_scales.iter().enumerate() {
                        if ui.selectable_label(scale_idx == i, s.name()).clicked() {
                            scale_idx = i;
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Root:");
                egui::ComboBox::from_id_salt("phr_root_combo")
                    .selected_text(ROOT_NAMES[root])
                    .show_ui(ui, |ui| {
                        for (i, name) in ROOT_NAMES.iter().enumerate() {
                            if ui.selectable_label(root == i, *name).clicked() {
                                root = i;
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Octave Range:");
                ui.add(egui::DragValue::new(&mut oct_min).range(0..=9).speed(1));
                ui.label("to");
                ui.add(egui::DragValue::new(&mut oct_max).range(1..=9).speed(1));
            });
            if oct_min > oct_max {
                oct_max = oct_min;
            }

            ui.add_space(4.0);
            section_header(ui, "Rhythm");

            match mode {
                GenMode::Melodic => {
                    ui.horizontal(|ui| {
                        ui.label("Density:");
                        ui.add(egui::Slider::new(&mut density, 0.0..=1.0).step_by(0.05));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Step Size:");
                        ui.add(egui::Slider::new(&mut step_size, 1..=7).step_by(1.0));
                    });
                }
                GenMode::Euclidean => {
                    ui.horizontal(|ui| {
                        ui.label("Pulses:");
                        ui.add(egui::DragValue::new(&mut pulses).range(1..=num_rows).speed(1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Rotation:");
                        ui.add(egui::DragValue::new(&mut rotation).range(0..=num_rows.saturating_sub(1)).speed(1));
                    });
                }
                GenMode::Drum => {
                    let max_ch = num_channels.saturating_sub(1);
                    ui.horizontal(|ui| {
                        ui.label("Kick Ch:");
                        ui.add(egui::DragValue::new(&mut kick_ch).range(0..=max_ch).speed(1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Snare Ch:");
                        ui.add(egui::DragValue::new(&mut snare_ch).range(0..=max_ch).speed(1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Hat Ch:");
                        ui.add(egui::DragValue::new(&mut hat_ch).range(0..=max_ch).speed(1));
                    });

                    let max_needed = *[kick_ch, snare_ch, hat_ch].iter().max().unwrap_or(&0);
                    if max_needed >= num_channels {
                        drum_warning = format!(
                            "Warning: channel {} exceeds available channels (0-{}). Those drums will be skipped.",
                            max_needed, max_ch
                        );
                    }
                }
                GenMode::Chord => {
                    let chord_types = ChordType::all();
                    let progressions = Progression::all();
                    ui.horizontal(|ui| {
                        ui.label("Chord Type:");
                        egui::ComboBox::from_id_salt("phr_chord_type_combo")
                            .selected_text(chord_types[chord_type_idx].name())
                            .show_ui(ui, |ui| {
                                for (i, ct) in chord_types.iter().enumerate() {
                                    if ui.selectable_label(chord_type_idx == i, ct.name()).clicked() {
                                        chord_type_idx = i;
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Progression:");
                        egui::ComboBox::from_id_salt("phr_progression_combo")
                            .selected_text(progressions[progression_idx].name())
                            .show_ui(ui, |ui| {
                                for (i, p) in progressions.iter().enumerate() {
                                    if ui.selectable_label(progression_idx == i, p.name()).clicked() {
                                        progression_idx = i;
                                    }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Bars per Chord:");
                        ui.add(egui::DragValue::new(&mut bars_per_chord).range(1..=16).speed(1));
                    });
                }
            }

            ui.add_space(4.0);
            section_header(ui, "Output");
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label("Instrument:");
                let mut inst_val: u32 = instr_str.parse().unwrap_or(0);
                ui.add(egui::DragValue::new(&mut inst_val).range(0..=255).speed(1));
                instr_str = inst_val.to_string();
            });

            ui.horizontal(|ui| {
                ui.label("Seed:");
                let mut seed_val: u64 = seed_str.parse().unwrap_or(0);
                if ui.add_sized([20.0, 16.0], egui::Button::new("<")).clicked() {
                    seed_val = seed_val.saturating_sub(1);
                    seed_str = seed_val.to_string();
                }
                let mut seed_buf = seed_str.clone();
                let resp = ui.add_sized([100.0, 16.0], egui::TextEdit::singleline(&mut seed_buf).desired_width(80.0));
                if resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    seed_str = seed_buf;
                }
                if ui.add_sized([20.0, 16.0], egui::Button::new(">")).clicked() {
                    seed_val = seed_val.saturating_add(1);
                    seed_str = seed_val.to_string();
                }
                if ui.button("🎲").clicked() {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
                    seed_str = (nanos as u64).wrapping_mul(6364136223846793005).to_string();
                }
            });

            let seed: u64 = seed_str.parse().unwrap_or(0);
            let instrument: Option<u8> = instr_str.parse::<u8>().ok();

            if !drum_warning.is_empty() {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(&drum_warning).color(egui::Color32::from_rgb(255, 180, 60)).size(11.0));
            }

            ui.add_space(4.0);
            section_header(ui, "Preview");

            let chord_types = ChordType::all();
            let chord_type = chord_types.get(chord_type_idx).copied().unwrap_or(ChordType::Triad);
            let progressions = Progression::all();
            let progression = progressions.get(progression_idx).copied().unwrap_or(Progression::OneFourFiveOne);
            let max_ch = num_channels.saturating_sub(1);
            let chord_channels = [
                0.min(max_ch),
                1.min(max_ch),
                2.min(max_ch),
                3.min(max_ch),
            ];

            let params = PhraseParams {
                mode,
                scale,
                root: root as u8,
                octave_min: oct_min,
                octave_max: oct_max,
                density,
                step_size,
                seed,
                instrument,
                pulses,
                rotation,
                kick_ch,
                snare_ch,
                hat_ch,
                chord_type,
                progression,
                bars_per_chord,
                chord_channels,
            };

            let preview_notes = phrase_generator::generate_phrase(&params, 0, num_rows.saturating_sub(1).min(63), num_channels);

            ui.add_space(2.0);
            let (preview_rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().min(400.0), 48.0),
                egui::Sense::hover(),
            );
            let painter = ui.painter_at(preview_rect);
            painter.rect_filled(preview_rect, 2.0, theme.panel_bg);
            let total_rows = num_rows.min(64);

            let mut max_notes_by_row = vec![0usize; total_rows];
            for &(row, _, _) in &preview_notes {
                if row < total_rows {
                    max_notes_by_row[row] = max_notes_by_row[row].saturating_add(1);
                }
            }

            let max_count = max_notes_by_row.iter().copied().max().unwrap_or(1).max(1) as f32;
            let cell_w = preview_rect.width() / total_rows as f32;
            for (row, &count) in max_notes_by_row.iter().enumerate() {
                if count > 0 {
                    let x = preview_rect.left() + row as f32 * cell_w;
                    let frac = count as f32 / max_count;
                    let h = (preview_rect.height() * frac).max(2.0);
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(x, preview_rect.bottom() - h),
                        egui::vec2(cell_w.max(1.0), h),
                    );
                    painter.rect_filled(rect, 0.0, theme.fg_instrument);
                }
            }

            use std::collections::BTreeSet;
            if mode == GenMode::Drum {
                let row_steps: BTreeSet<usize> = preview_notes.iter().map(|(r, _, _)| *r).collect();
                for row in row_steps {
                    if row < total_rows {
                        let x = preview_rect.left() + row as f32 * cell_w + cell_w * 0.15;
                        let colors = [theme.vu_green, theme.vu_yellow, theme.vu_red];
                        let drum_chs: Vec<_> = preview_notes.iter().filter(|(r, _, _)| *r == row).collect();
                        for (i, (_, ch, _)) in drum_chs.iter().enumerate() {
                            let _ = ch;
                            let color = colors[i.min(2)];
                            let dot_y = preview_rect.top() + 4.0 + i as f32 * 8.0;
                            painter.circle_filled(egui::pos2(x + i as f32 * 4.0, dot_y), 2.5, color);
                        }
                    }
                }
            } else {
                let active_rows: BTreeSet<usize> = preview_notes.iter().map(|(r, _, _)| *r).collect();
                for &row in &active_rows {
                    if row < total_rows {
                        let x = preview_rect.left() + row as f32 * cell_w + cell_w * 0.3;
                        let y = preview_rect.center().y;
                        painter.circle_filled(egui::pos2(x, y), 2.5, theme.fg_instrument);
                    }
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let valid = true;
                let btn = egui::Button::new("Apply");
                if ui.add_enabled(valid, btn).clicked() {
                    result = Some(params);
                    should_close = true;
                }
                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });

            ui.data_mut(|d| {
                d.insert_temp(mode_id, mode_idx);
                d.insert_temp(scale_id, scale_idx);
                d.insert_temp(root_id, root);
                d.insert_temp(oct_min_id, oct_min);
                d.insert_temp(oct_max_id, oct_max);
                d.insert_temp(density_id, density);
                d.insert_temp(step_id, step_size);
                d.insert_temp(pulses_id, pulses);
                d.insert_temp(rotation_id, rotation);
                d.insert_temp(inst_id, instr_str);
                d.insert_temp(seed_id, seed_str);
                d.insert_temp(kick_id, kick_ch);
                d.insert_temp(snare_id, snare_ch);
                d.insert_temp(hat_id, hat_ch);
                d.insert_temp(chord_type_id, chord_type_idx);
                d.insert_temp(progression_id, progression_idx);
                d.insert_temp(bars_per_chord_id, bars_per_chord);
            });
        });

    if should_close {
        *open = false;
    }

    result
}

fn section_header(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(100, 200, 255)),
    );
}
