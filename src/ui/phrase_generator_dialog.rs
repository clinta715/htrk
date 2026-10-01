use eframe::egui;

use super::style::FONT_BODY;
use super::theme::TrackerTheme;
use crate::tools::phrase_generator::{self, ChordType, GenMode, PhraseParams, Progression};
use crate::tools::scale::{Scale, ROOT_NAMES};

/// Phrase generator dialog parameters (typed UI state, P3).
/// Session-only by design (see AGENTS.md §15): not persisted to AppConfig.
#[derive(Clone, Debug)]
pub struct PhraseGenState {
    pub mode_idx: usize,
    pub scale_idx: usize,
    pub root: usize,
    pub oct_min: u8,
    pub oct_max: u8,
    pub density: f32,
    pub step_size: u8,
    pub pulses: usize,
    pub rotation: usize,
    pub instr_str: String,
    pub seed_str: String,
    pub kick_ch: usize,
    pub snare_ch: usize,
    pub hat_ch: usize,
    pub chord_type_idx: usize,
    pub progression_idx: usize,
    pub bars_per_chord: u8,
}

impl Default for PhraseGenState {
    fn default() -> Self {
        PhraseGenState {
            mode_idx: 0,
            scale_idx: 0,
            root: 0,
            oct_min: 3,
            oct_max: 5,
            density: 0.3,
            step_size: 3,
            pulses: 8,
            rotation: 0,
            instr_str: String::new(),
            seed_str: "0".to_string(),
            kick_ch: 0,
            snare_ch: 1,
            hat_ch: 2,
            chord_type_idx: 0,
            progression_idx: 0,
            bars_per_chord: 4,
        }
    }
}

pub fn draw_phrase_generator(
    ctx: &egui::Context,
    open: &mut bool,
    theme: &TrackerTheme,
    num_channels: usize,
    num_rows: usize,
    _cursor_ch: usize,
    state: &mut PhraseGenState,
) -> Option<PhraseParams> {
    let mut result = None;
    let mut should_close = false;

    egui::Window::new("Generate Phrase")
        .id(egui::Id::new("phrase_generator"))
        .open(open)
        .resizable(true)
        .default_size([280.0, 520.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let mut mode_idx = state.mode_idx;
            let mut scale_idx = state.scale_idx;
            let mut root = state.root;
            let mut oct_min = state.oct_min;
            let mut oct_max = state.oct_max;
            let mut density = state.density;
            let mut step_size = state.step_size;
            let mut pulses = state.pulses;
            let mut rotation = state.rotation;
            let mut instr_str = state.instr_str.clone();
            let mut seed_str = state.seed_str.clone();
            let mut kick_ch = state.kick_ch;
            let mut snare_ch = state.snare_ch;
            let mut hat_ch = state.hat_ch;
            let mut chord_type_idx = state.chord_type_idx;
            let mut progression_idx = state.progression_idx;
            let mut bars_per_chord = state.bars_per_chord;

            let all_scales = Scale::all();
            let mode = GenMode::all().get(mode_idx).copied().unwrap_or(GenMode::Melodic);
            let scale = all_scales.get(scale_idx).copied().unwrap_or(Scale::Major);

            let mut drum_warning = String::new();

            section_header(ui, "Mode", theme);
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
            section_header(ui, "Scale", theme);
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
            section_header(ui, "Rhythm", theme);

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
            section_header(ui, "Output", theme);
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
                ui.label(egui::RichText::new(&drum_warning).color(theme.fg_effect).size(FONT_BODY));
            }

            ui.add_space(4.0);
            section_header(ui, "Preview", theme);

            let chord_types = ChordType::all();
            let chord_type = chord_types.get(chord_type_idx).copied().unwrap_or(ChordType::Triad);
            let progressions = Progression::all();
            let progression = progressions.get(progression_idx).copied().unwrap_or(Progression::OneFourFiveOne);
            let max_ch = num_channels.saturating_sub(1);
            let chord_channels = [
                0,
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
                kick_instrument: None,
                snare_instrument: None,
                hat_instrument: None,
                kick_density: None,
                snare_density: None,
                hat_density: None,
                swing: 0.0,
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

            state.mode_idx = mode_idx;
            state.scale_idx = scale_idx;
            state.root = root;
            state.oct_min = oct_min;
            state.oct_max = oct_max;
            state.density = density;
            state.step_size = step_size;
            state.pulses = pulses;
            state.rotation = rotation;
            state.instr_str = instr_str;
            state.seed_str = seed_str;
            state.kick_ch = kick_ch;
            state.snare_ch = snare_ch;
            state.hat_ch = hat_ch;
            state.chord_type_idx = chord_type_idx;
            state.progression_idx = progression_idx;
            state.bars_per_chord = bars_per_chord;
        });

    if should_close {
        *open = false;
    }

    result
}

fn section_header(ui: &mut egui::Ui, text: &str, theme: &super::theme::TrackerTheme) {
    super::style::section_header(ui, text, theme);
}
