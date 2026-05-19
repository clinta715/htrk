use eframe::egui;

use super::pattern_grid::ColumnVisibility;
use super::theme::{ThemePreset, TrackerTheme};
use crate::app_config::SpacingMode;

pub struct MenuResponse {
    pub new_song: bool,
    pub open_file: bool,
    pub import_sample: bool,
    pub import_instrument: bool,
    pub save_file: bool,
    pub save_as: bool,
    pub export_wav: bool,
    pub undo: bool,
    pub redo: bool,
    pub cut: bool,
    pub copy: bool,
    pub paste: bool,
    pub select_all: bool,
    pub cut_track: bool,
    pub copy_track: bool,
    pub delete_track: bool,
    pub cut_column: bool,
    pub copy_column: bool,
    pub delete_column: bool,
    pub follow_playback: bool,
    pub theme_changed: Option<ThemePreset>,
    pub col_vis: Option<ColumnVisibility>,
    pub spacing_mode_changed: Option<SpacingMode>,

    pub show_shortcuts: bool,
    pub show_about: bool,
    pub show_settings: bool,
}

impl Default for MenuResponse {
    fn default() -> Self {
        MenuResponse {
            new_song: false,
            open_file: false,
            import_sample: false,
            import_instrument: false,
            save_file: false,
            save_as: false,
            export_wav: false,
            undo: false,
            redo: false,
            cut: false,
            copy: false,
            paste: false,
            select_all: false,
            cut_track: false,
            copy_track: false,
            delete_track: false,
            cut_column: false,
            copy_column: false,
            delete_column: false,
            follow_playback: false,
            theme_changed: None,
            col_vis: None,
            spacing_mode_changed: None,

            show_shortcuts: false,
            show_about: false,
            show_settings: false,
        }
    }
}

pub fn draw_menu_bar(
    ui: &mut egui::Ui,
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
    follow_playback: bool,
    current_theme: ThemePreset,
    current_spacing: SpacingMode,
    _theme: &TrackerTheme,
    sample_rate: u32,
    sample_format: &str,
    col_vis: &mut ColumnVisibility,
) -> MenuResponse {
    let mut resp = MenuResponse::default();

    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("New Song    Ctrl+N").clicked() {
                resp.new_song = true;
                ui.close_menu();
            }
            if ui.button("Open...     Ctrl+O").clicked() {
                resp.open_file = true;
                ui.close_menu();
            }
            if ui.button("Import Sample...    Ctrl+I").clicked() {
                resp.import_sample = true;
                ui.close_menu();
            }
            if ui.button("Import Instrument...  Ctrl+Shift+I").clicked() {
                resp.import_instrument = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Save        Ctrl+S").clicked() {
                resp.save_file = true;
                ui.close_menu();
            }
            if ui.button("Save As...  Ctrl+Shift+S").clicked() {
                resp.save_as = true;
                ui.close_menu();
            }
            if ui.button("Export as WAV...").clicked() {
                resp.export_wav = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Settings...    F10").clicked() {
                resp.show_settings = true;
                ui.close_menu();
            }
        });

        ui.menu_button("Edit", |ui| {
            if ui.add_enabled(can_undo, egui::Button::new("Undo   Ctrl+Z")).clicked() {
                resp.undo = true;
                ui.close_menu();
            }
            if ui.add_enabled(can_redo, egui::Button::new("Redo   Ctrl+Y")).clicked() {
                resp.redo = true;
                ui.close_menu();
            }
            ui.separator();
            if ui
                .add_enabled(has_selection, egui::Button::new("Cut Block    Ctrl+X"))
                .clicked()
            {
                resp.cut = true;
                ui.close_menu();
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("Copy Block   Ctrl+C"))
                .clicked()
            {
                resp.copy = true;
                ui.close_menu();
            }
            if ui.button("Paste Block  Ctrl+V").clicked() {
                resp.paste = true;
                ui.close_menu();
            }
            ui.separator();

            ui.menu_button("Track", |ui| {
                if ui.button("Cut Track      Shift+F3").clicked() {
                    resp.cut_track = true;
                    ui.close_menu();
                }
                if ui.button("Copy Track     Shift+F4").clicked() {
                    resp.copy_track = true;
                    ui.close_menu();
                }
                if ui.button("Paste Track    Shift+F5").clicked() {
                    resp.paste = true; // reusing paste logic
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Clear Track    Shift+Del").clicked() {
                    resp.delete_track = true;
                    ui.close_menu();
                }
            });

            ui.menu_button("Column", |ui| {
                if ui.button("Cut Column     Alt+F3").clicked() {
                    resp.cut_column = true;
                    ui.close_menu();
                }
                if ui.button("Copy Column    Alt+F4").clicked() {
                    resp.copy_column = true;
                    ui.close_menu();
                }
                if ui.button("Paste Column   Alt+F5").clicked() {
                    resp.paste = true; // reusing paste logic
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Clear Column").clicked() {
                    resp.delete_column = true;
                    ui.close_menu();
                }
            });

            ui.separator();
            if ui.button("Select All  Ctrl+A").clicked() {
                resp.select_all = true;
                ui.close_menu();
            }
        });

        ui.menu_button("View", |ui| {
            let label = if follow_playback {
                "Follow Playback  [ON]"
            } else {
                "Follow Playback  [OFF]"
            };
            if ui.button(label).clicked() {
                resp.follow_playback = !follow_playback;
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button("Columns", |ui| {
                let mut note = col_vis.note;
                let note_check = if note { "✓ " } else { "  " };
                if ui.checkbox(&mut note, format!("{}Note      Ctrl+1", note_check)).clicked() {
                    col_vis.note = note;
                    resp.col_vis = Some(*col_vis);
                }
                let mut instr = col_vis.instrument;
                let instr_check = if instr { "✓ " } else { "  " };
                if ui.checkbox(&mut instr, format!("{}Instrument      Ctrl+2", instr_check)).clicked() {
                    col_vis.instrument = instr;
                    resp.col_vis = Some(*col_vis);
                }
                let mut vol = col_vis.volume;
                let vol_check = if vol { "✓ " } else { "  " };
                if ui.checkbox(&mut vol, format!("{}Volume      Ctrl+3", vol_check)).clicked() {
                    col_vis.volume = vol;
                    resp.col_vis = Some(*col_vis);
                }
                let mut eff = col_vis.effect;
                let eff_check = if eff { "✓ " } else { "  " };
                if ui.checkbox(&mut eff, format!("{}Effect      Ctrl+4", eff_check)).clicked() {
                    col_vis.effect = eff;
                    resp.col_vis = Some(*col_vis);
                }
                ui.separator();
                let all_visible = col_vis.note && col_vis.instrument && col_vis.volume && col_vis.effect;
                let reset_label = if all_visible { "Reset to Default (all visible)" } else { "Reset to Default" };
                if ui.button(reset_label).clicked() {
                    col_vis.note = true;
                    col_vis.instrument = true;
                    col_vis.volume = true;
                    col_vis.effect = true;
                    resp.col_vis = Some(*col_vis);
                }
            });
            ui.menu_button("Spacing", |ui| {
                let modes = [
                    (SpacingMode::Compact, "Compact"),
                    (SpacingMode::Normal, "Normal"),
                    (SpacingMode::Wide, "Wide"),
                    (SpacingMode::ExtraWide, "Extra Wide"),
                ];
                for (mode, label) in modes {
                    let check = if mode == current_spacing { "> " } else { "  " };
                    if ui.button(format!("{}{}  Ctrl+Shift+Space", check, label)).clicked() {
                        resp.spacing_mode_changed = Some(mode);
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            for preset in ThemePreset::all() {
                let label = if *preset == current_theme {
                    format!("> {}", preset.name())
                } else {
                    format!("  {}", preset.name())
                };
                if ui.button(label).clicked() {
                    resp.theme_changed = Some(*preset);
                    ui.close_menu();
                }
            }
        });

        ui.menu_button("Audio", |ui| {
            ui.label(format!("Rate: {} Hz  Format: {}", sample_rate, sample_format));
            ui.label("Device selection is in Settings (F10)");
        });

        ui.menu_button("Help", |ui| {
            if ui.button("Keyboard Shortcuts   F3").clicked() {
                resp.show_shortcuts = true;
                ui.close_menu();
            }
            if ui.button("About htrk").clicked() {
                resp.show_about = true;
                ui.close_menu();
            }
        });
    });

    resp
}
