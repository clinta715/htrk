use std::path::PathBuf;

use eframe::egui;

use crate::app_config::AppConfig;
use crate::ui::theme::ThemePreset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    Paths,
    Editor,
    Audio,
    Backup,
    Theme,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsAction {
    None,
    Save,
    Apply,
    Cancel,
}

#[derive(Clone)]
pub struct SettingsState {
    pub open: bool,
    tab: SettingsTab,

    default_sample_paths: Vec<String>,
    default_mod_path: Option<String>,
    default_xm_path: Option<String>,
    default_instrument_path: Option<String>,
    default_project_path: Option<String>,

    editor_font_size: u32,
    zoom_factor: f32,
    scroll_speed: f32,
    visible_channels: usize,
    show_row_numbers: bool,
    show_hex_row_numbers: bool,
    snap_to_grid: bool,
    follow_playback_default: bool,

    default_amplify_factor: f32,

    auto_backup_interval_secs: u64,
    backup_directory: Option<String>,

    theme_preset: String,

    new_sample_path: String,
}

impl SettingsState {
    pub fn from_config(config: &AppConfig) -> Self {
        SettingsState {
            open: false,
            tab: SettingsTab::Paths,

            default_sample_paths: config.default_sample_paths.clone(),
            default_mod_path: config.default_mod_path.clone(),
            default_xm_path: config.default_xm_path.clone(),
            default_instrument_path: config.default_instrument_path.clone(),
            default_project_path: config.default_project_path.clone(),

            editor_font_size: config.editor_font_size,
            zoom_factor: config.zoom_factor,
            scroll_speed: config.scroll_speed,
            visible_channels: config.visible_channels,
            show_row_numbers: config.show_row_numbers,
            show_hex_row_numbers: config.show_hex_row_numbers,
            snap_to_grid: config.snap_to_grid,
            follow_playback_default: config.follow_playback_default,

            default_amplify_factor: config.default_amplify_factor,

            auto_backup_interval_secs: config.auto_backup_interval_secs,
            backup_directory: config.backup_directory.clone(),

            theme_preset: config.theme_preset.clone(),

            new_sample_path: String::new(),
        }
    }

    pub fn apply_to_config(&self, config: &mut AppConfig) {
        config.default_sample_paths = self.default_sample_paths.clone();
        config.default_mod_path = self.default_mod_path.clone();
        config.default_xm_path = self.default_xm_path.clone();
        config.default_instrument_path = self.default_instrument_path.clone();
        config.default_project_path = self.default_project_path.clone();

        config.editor_font_size = self.editor_font_size;
        config.zoom_factor = self.zoom_factor;
        config.scroll_speed = self.scroll_speed;
        config.visible_channels = self.visible_channels;
        config.show_row_numbers = self.show_row_numbers;
        config.show_hex_row_numbers = self.show_hex_row_numbers;
        config.snap_to_grid = self.snap_to_grid;
        config.follow_playback_default = self.follow_playback_default;

        config.default_amplify_factor = self.default_amplify_factor;

        config.auto_backup_interval_secs = self.auto_backup_interval_secs;
        config.backup_directory = self.backup_directory.clone();

        config.theme_preset = self.theme_preset.clone();
    }
}

pub fn draw_settings_window(ctx: &egui::Context, state: &mut SettingsState) -> SettingsAction {
    let mut action = SettingsAction::None;
    let mut is_open = state.open;
    egui::Window::new("Settings")
        .open(&mut is_open)
        .resizable(true)
        .default_width(520.0)
        .default_height(480.0)
        .min_width(400.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tabs = [
                    (SettingsTab::Paths, "Paths"),
                    (SettingsTab::Editor, "Editor"),
                    (SettingsTab::Audio, "Audio"),
                    (SettingsTab::Backup, "Backup"),
                    (SettingsTab::Theme, "Theme"),
                ];
                for (tab, label) in tabs {
                    if ui.selectable_label(state.tab == tab, label).clicked() {
                        state.tab = tab;
                    }
                }
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| match state.tab {
                    SettingsTab::Paths => draw_paths_tab(ui, state),
                    SettingsTab::Editor => draw_editor_tab(ui, state),
                    SettingsTab::Audio => draw_audio_tab(ui, state),
                    SettingsTab::Backup => draw_backup_tab(ui, state),
                    SettingsTab::Theme => draw_theme_tab(ui, state),
                });

            ui.separator();
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                if ui.button("Save").clicked() {
                    action = SettingsAction::Save;
                    state.open = false;
                }
                if ui.button("Apply").clicked() {
                    action = SettingsAction::Apply;
                }
                if ui.button("Cancel").clicked() {
                    action = SettingsAction::Cancel;
                    state.open = false;
                }
            });
        });
    if !is_open && action == SettingsAction::None {
        action = SettingsAction::Cancel;
    }
    state.open = is_open;
    action
}

fn draw_paths_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.add_space(4.0);
    section_header(ui, "Default Sample Directories");
    ui.add_space(2.0);

    let mut remove_idx = None;
    for (i, path) in state.default_sample_paths.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{:>2}.", i + 1)).monospace().size(11.0));
            let resp = ui.add_sized(
                [ui.available_width() - 90.0, 20.0],
                egui::TextEdit::singleline(path).font(egui::FontId::monospace(11.0)),
            );
            if resp.lost_focus() && path.trim().is_empty() {
                remove_idx = Some(i);
            }
            if ui.button("...").clicked() {
                if let Some(p) = pick_folder() {
                    *path = p.to_string_lossy().into_owned();
                }
            }
            if ui.button("X").clicked() {
                remove_idx = Some(i);
            }
        });
    }
    if let Some(idx) = remove_idx {
        state.default_sample_paths.remove(idx);
    }

    ui.horizontal(|ui| {
        ui.add_space(20.0);
        ui.add(egui::TextEdit::singleline(&mut state.new_sample_path)
            .font(egui::FontId::monospace(11.0))
            .hint_text("Add path..."));
        if ui.button("+ Add").clicked() {
            let p = state.new_sample_path.trim().to_string();
            if !p.is_empty() {
                state.default_sample_paths.push(p);
                state.new_sample_path.clear();
            }
        }
        if ui.button("... Browse").clicked() {
            if let Some(p) = pick_folder() {
                state.default_sample_paths.push(p.to_string_lossy().into_owned());
            }
        }
    });

    ui.add_space(12.0);
    section_header(ui, "Default Directories");
    ui.add_space(2.0);

    path_row(ui, "Module Path", &mut state.default_mod_path);
    path_row(ui, "XM Export Path", &mut state.default_xm_path);
    path_row(ui, "Instrument Path", &mut state.default_instrument_path);
    path_row(ui, "Project Path", &mut state.default_project_path);
}

fn path_row(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let mut text = value.take().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(12.0));
        ui.add_space(8.0);
        let resp = ui.add_sized(
            [ui.available_width() - 70.0, 20.0],
            egui::TextEdit::singleline(&mut text).font(egui::FontId::monospace(11.0)),
        );
        if ui.button("...").clicked() {
            if let Some(p) = pick_folder() {
                text = p.to_string_lossy().into_owned();
            }
        }
        let _ = resp;
    });
    if text.trim().is_empty() {
        *value = None;
    } else {
        *value = Some(text);
    }
}

fn draw_editor_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.add_space(4.0);
    section_header(ui, "Pattern Editor");
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Font Size:");
        ui.add(egui::DragValue::new(&mut state.editor_font_size).range(8..=24).speed(1));
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Zoom Factor:");
        ui.add(egui::Slider::new(&mut state.zoom_factor, 0.5..=2.5).step_by(0.1));
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Scroll Speed:");
        ui.add(egui::Slider::new(&mut state.scroll_speed, 0.5..=4.0).step_by(0.1));
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Visible Channels:");
        let ch_options = [4, 8, 16, 32, 64];
        egui::ComboBox::from_id_salt("settings_visible_channels")
            .selected_text(format!("{}", state.visible_channels))
            .show_ui(ui, |ui| {
                for &ch in &ch_options {
                    let label = format!("{}", ch);
                    if ui.selectable_label(state.visible_channels == ch, &label).clicked() {
                        state.visible_channels = ch;
                    }
                }
            });
    });
    ui.add_space(8.0);

    ui.checkbox(&mut state.show_row_numbers, "Show Row Numbers");
    ui.checkbox(&mut state.show_hex_row_numbers, "Hex Row Numbers");
    ui.checkbox(&mut state.snap_to_grid, "Snap Selection to Grid");
    ui.checkbox(&mut state.follow_playback_default, "Follow Playback (default)");
}

fn draw_audio_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.add_space(4.0);
    section_header(ui, "Audio Defaults");
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Default Amplify Factor:");
        ui.add(egui::Slider::new(&mut state.default_amplify_factor, 0.1..=10.0).step_by(0.1));
    });
}

fn draw_backup_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.add_space(4.0);
    section_header(ui, "Auto-Backup");
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Interval (seconds):");
        ui.add(egui::DragValue::new(&mut state.auto_backup_interval_secs).range(0..=3600).speed(10));
        ui.label("(0 = off)");
    });
    ui.add_space(4.0);

    let default_dir = AppConfig::default().get_backup_dir();
    ui.horizontal(|ui| {
        ui.label("Backup Directory:");
    });
    let mut text = state.backup_directory.take().unwrap_or_default();
    ui.horizontal(|ui| {
        let hint = format!("Default: {}", default_dir.to_string_lossy());
        ui.add_sized(
            [ui.available_width() - 70.0, 20.0],
            egui::TextEdit::singleline(&mut text)
                .font(egui::FontId::monospace(11.0))
                .hint_text(&hint),
        );
        if ui.button("...").clicked() {
            if let Some(p) = pick_folder() {
                text = p.to_string_lossy().into_owned();
            }
        }
    });
    if text.trim().is_empty() {
        state.backup_directory = None;
    } else {
        state.backup_directory = Some(text);
    }

    ui.add_space(8.0);
    let backup_dir = if state.backup_directory.is_some() {
        PathBuf::from(state.backup_directory.as_ref().unwrap())
    } else {
        default_dir
    };
    ui.label(
        egui::RichText::new(format!("Backups will be saved to: {}", backup_dir.to_string_lossy()))
            .size(11.0)
            .color(egui::Color32::GRAY),
    );
}

fn draw_theme_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.add_space(4.0);
    section_header(ui, "Theme");
    ui.add_space(4.0);

    for preset in ThemePreset::all() {
        let is_selected = state.theme_preset == preset.config_key();
        if ui.selectable_label(is_selected, preset.name()).clicked() {
            state.theme_preset = preset.config_key().to_string();
        }
    }
}

fn section_header(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(100, 200, 255)),
    );
}

fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}
