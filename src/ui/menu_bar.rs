use eframe::egui;
use eguidev::DevUiExt;

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
    pub quit: bool,
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
            quit: false,
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

    egui::MenuBar::new().ui(ui, |ui| {
        ui.dev_menu_button("menu.file", "File", |ui| {
            if ui.dev_button("menu.file.new_song", "New Song    Ctrl+N").clicked() {
                resp.new_song = true;
                ui.close();
            }
            if ui.dev_button("menu.file.open", "Open...     Ctrl+O").clicked() {
                resp.open_file = true;
                ui.close();
            }
            if ui.dev_button("menu.file.import_sample", "Import Sample...    Ctrl+I").clicked() {
                resp.import_sample = true;
                ui.close();
            }
            if ui.dev_button("menu.file.import_instrument", "Import Instrument...  Ctrl+Shift+I").clicked() {
                resp.import_instrument = true;
                ui.close();
            }
            ui.dev_separator("menu.file.sep1");
            if ui.dev_button("menu.file.save", "Save        Ctrl+S").clicked() {
                resp.save_file = true;
                ui.close();
            }
            if ui.dev_button("menu.file.save_as", "Save As...  Ctrl+Shift+S").clicked() {
                resp.save_as = true;
                ui.close();
            }
            if ui.dev_button("menu.file.export_wav", "Export as WAV...").clicked() {
                resp.export_wav = true;
                ui.close();
            }
            ui.dev_separator("menu.file.sep2");
            if ui.dev_button("menu.file.settings", "Settings...    F10").clicked() {
                resp.show_settings = true;
                ui.close();
            }
            if ui.dev_button("menu.file.quit", "Quit    Ctrl+Q").clicked() {
                resp.quit = true;
                ui.close();
            }
        });

        ui.dev_menu_button("menu.edit", "Edit", |ui| {
            if ui.add_enabled(can_undo, egui::Button::new("Undo   Ctrl+Z")).clicked() {
                resp.undo = true;
                ui.close();
            }
            if ui.add_enabled(can_redo, egui::Button::new("Redo   Ctrl+Y")).clicked() {
                resp.redo = true;
                ui.close();
            }
            ui.dev_separator("menu.edit.sep1");
            if ui.add_enabled(has_selection, egui::Button::new("Cut Block    Ctrl+X")).clicked() {
                resp.cut = true;
                ui.close();
            }
            if ui.add_enabled(has_selection, egui::Button::new("Copy Block   Ctrl+C")).clicked() {
                resp.copy = true;
                ui.close();
            }
            if ui.dev_button("menu.edit.paste", "Paste Block  Ctrl+V").clicked() {
                resp.paste = true;
                ui.close();
            }
            ui.dev_separator("menu.edit.sep2");

            ui.dev_menu_button("menu.edit.track", "Track", |ui| {
                if ui.dev_button("menu.edit.track.cut", "Cut Track      Shift+F3").clicked() {
                    resp.cut_track = true;
                    ui.close();
                }
                if ui.dev_button("menu.edit.track.copy", "Copy Track     Shift+F4").clicked() {
                    resp.copy_track = true;
                    ui.close();
                }
                if ui.dev_button("menu.edit.track.paste", "Paste Track    Shift+F5").clicked() {
                    resp.paste = true;
                    ui.close();
                }
                ui.dev_separator("menu.edit.track.sep");
                if ui.dev_button("menu.edit.track.clear", "Clear Track    Shift+Del").clicked() {
                    resp.delete_track = true;
                    ui.close();
                }
            });

            ui.dev_menu_button("menu.edit.column", "Column", |ui| {
                if ui.dev_button("menu.edit.column.cut", "Cut Column     Alt+F3").clicked() {
                    resp.cut_column = true;
                    ui.close();
                }
                if ui.dev_button("menu.edit.column.copy", "Copy Column    Alt+F4").clicked() {
                    resp.copy_column = true;
                    ui.close();
                }
                if ui.dev_button("menu.edit.column.paste", "Paste Column   Alt+F5").clicked() {
                    resp.paste = true;
                    ui.close();
                }
                ui.dev_separator("menu.edit.column.sep");
                if ui.dev_button("menu.edit.column.clear", "Clear Column").clicked() {
                    resp.delete_column = true;
                    ui.close();
                }
            });

            ui.dev_separator("menu.edit.sep3");
            if ui.dev_button("menu.edit.select_all", "Select All  Ctrl+A").clicked() {
                resp.select_all = true;
                ui.close();
            }
        });

        ui.dev_menu_button("menu.view", "View", |ui| {
            let label = if follow_playback {
                "Follow Playback  [ON]"
            } else {
                "Follow Playback  [OFF]"
            };
            if ui.dev_button("menu.view.follow_playback", label).clicked() {
                resp.follow_playback = !follow_playback;
                ui.close();
            }
            ui.dev_separator("menu.view.sep1");
            ui.dev_menu_button("menu.view.columns", "Columns", |ui| {
                let mut note = col_vis.note;
                if ui.dev_checkbox("menu.view.columns.note", &mut note, "Note      Ctrl+1").clicked() {
                    col_vis.note = note;
                    resp.col_vis = Some(*col_vis);
                }
                let mut instr = col_vis.instrument;
                if ui.dev_checkbox("menu.view.columns.instrument", &mut instr, "Instrument      Ctrl+2").clicked() {
                    col_vis.instrument = instr;
                    resp.col_vis = Some(*col_vis);
                }
                let mut vol = col_vis.volume;
                if ui.dev_checkbox("menu.view.columns.volume", &mut vol, "Volume      Ctrl+3").clicked() {
                    col_vis.volume = vol;
                    resp.col_vis = Some(*col_vis);
                }
                let mut eff = col_vis.effect;
                if ui.dev_checkbox("menu.view.columns.effect", &mut eff, "Effect      Ctrl+4").clicked() {
                    col_vis.effect = eff;
                    resp.col_vis = Some(*col_vis);
                }
                ui.dev_separator("menu.view.columns.sep");
                if ui.dev_button("menu.view.columns.reset", "Reset to Default").clicked() {
                    col_vis.note = true;
                    col_vis.instrument = true;
                    col_vis.volume = true;
                    col_vis.effect = true;
                    resp.col_vis = Some(*col_vis);
                }
            });
            ui.dev_menu_button("menu.view.spacing", "Spacing", |ui| {
                let modes = [
                    (SpacingMode::Compact, "Compact"),
                    (SpacingMode::Normal, "Normal"),
                    (SpacingMode::Wide, "Wide"),
                    (SpacingMode::ExtraWide, "Extra Wide"),
                ];
                for (i, (mode, label)) in modes.iter().enumerate() {
                    let check = if *mode == current_spacing { "> " } else { "  " };
                    if ui.dev_button(format!("menu.view.spacing.{}", i), format!("{}{}  Ctrl+Shift+Space", check, label)).clicked() {
                        resp.spacing_mode_changed = Some(*mode);
                        ui.close();
                    }
                }
            });
            ui.dev_separator("menu.view.sep2");
            for (i, preset) in ThemePreset::all().iter().enumerate() {
                let label = if *preset == current_theme {
                    format!("> {}", preset.name())
                } else {
                    format!("  {}", preset.name())
                };
                if ui.dev_button(format!("menu.view.theme.{}", i), label).clicked() {
                    resp.theme_changed = Some(*preset);
                    ui.close();
                }
            }
        });

        ui.dev_menu_button("menu.audio", "Audio", |ui| {
            ui.dev_label("menu.audio.rate", format!("Rate: {} Hz  Format: {}", sample_rate, sample_format));
            ui.dev_label("menu.audio.device_hint", "Device selection is in Settings (F10)");
        });

        ui.dev_menu_button("menu.help", "Help", |ui| {
            if ui.dev_button("menu.help.shortcuts", "Keyboard Shortcuts   F3").clicked() {
                resp.show_shortcuts = true;
                ui.close();
            }
            if ui.dev_button("menu.help.about", "About htrk").clicked() {
                resp.show_about = true;
                ui.close();
            }
        });
    });

    resp
}
