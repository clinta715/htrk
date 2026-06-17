use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ui::file_browser::BrowserMode;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SpacingMode {
    Compact,
    Normal,
    Wide,
    ExtraWide,
}

impl Default for SpacingMode {
    fn default() -> Self {
        SpacingMode::Normal
    }
}

impl SpacingMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "compact" => Some(SpacingMode::Compact),
            "normal" => Some(SpacingMode::Normal),
            "wide" => Some(SpacingMode::Wide),
            "extra_wide" => Some(SpacingMode::ExtraWide),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SpacingMode::Compact => "compact",
            SpacingMode::Normal => "normal",
            SpacingMode::Wide => "wide",
            SpacingMode::ExtraWide => "extra_wide",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub last_dirs: HashMap<String, String>,
    #[serde(default)]
    pub last_selections: HashMap<String, (u32, u32)>,
    #[serde(default)]
    pub last_file_path: Option<String>,
    #[serde(default)]
    pub favorites: Vec<String>,

    #[serde(default)]
    pub default_sample_paths: Vec<String>,
    #[serde(default)]
    pub default_mod_path: Option<String>,
    #[serde(default)]
    pub default_instrument_path: Option<String>,
    #[serde(default)]
    pub default_wav_path: Option<String>,
    #[serde(default)]
    pub sample_export_bit_depth: Option<u8>,
    #[serde(default)]
    pub default_project_path: Option<String>,

    #[serde(default = "default_font_size")]
    pub editor_font_size: u32,
    #[serde(default = "default_zoom")]
    pub zoom_factor: f32,
    #[serde(default = "default_scroll_speed")]
    pub scroll_speed: f32,
    #[serde(default = "default_visible_channels")]
    pub visible_channels: usize,
    #[serde(default = "default_true")]
    pub show_row_numbers: bool,
    #[serde(default)]
    pub show_hex_row_numbers: bool,
    #[serde(default)]
    pub snap_to_grid: bool,
    #[serde(default = "default_true")]
    pub follow_playback_default: bool,

    #[serde(default = "default_amplify")]
    pub default_amplify_factor: f32,

    #[serde(default)]
    pub auto_backup_interval_secs: u64,
    #[serde(default)]
    pub backup_directory: Option<String>,

    #[serde(default = "default_theme")]
    pub theme_preset: String,

    #[serde(default = "default_spacing_mode")]
    pub spacing_mode: String,

    #[serde(default)]
    pub debug: bool,

    #[serde(default = "default_row_highlight_minor")]
    pub row_highlight_minor: u8,
    #[serde(default = "default_row_highlight_major")]
    pub row_highlight_major: u8,

    #[serde(default = "default_interpolation")]
    pub default_interpolation: String,
    #[serde(default = "default_limiter")]
    pub limiter_mode: String,
    #[serde(default)]
    pub output_device_name: Option<String>,
    #[serde(default)]
    pub preferred_sample_rate: Option<u32>,

    #[serde(default = "default_file_browser_view_mode")]
    pub file_browser_view_mode: String,
    #[serde(default = "default_file_browser_sort_by")]
    pub file_browser_sort_by: String,
    #[serde(default)]
    pub file_browser_sort_desc: bool,

    #[serde(default = "default_sample_length_bg")]
    pub sample_length_bg: bool,

    #[serde(default = "default_col_vis")]
    pub col_vis_note: bool,
    #[serde(default = "default_col_vis")]
    pub col_vis_instrument: bool,
    #[serde(default = "default_col_vis")]
    pub col_vis_volume: bool,
    #[serde(default = "default_col_vis")]
    pub col_vis_effect: bool,

    #[serde(default)]
    pub window_width: Option<f32>,
    #[serde(default)]
    pub window_height: Option<f32>,

    #[serde(default)]
    pub instrument_list_width: Option<f32>,
    #[serde(default)]
    pub instrument_envelope_height: Option<f32>,

    #[serde(default)]
    pub sample_list_width: Option<f32>,
    #[serde(default)]
    pub sample_waveform_height: Option<f32>,

    #[serde(default)]
    pub order_list_width: Option<f32>,

    #[serde(default)]
    pub file_browser_name_width: Option<f32>,
    #[serde(default)]
    pub file_browser_dur_width: Option<f32>,
    #[serde(default)]
    pub file_browser_type_width: Option<f32>,
    #[serde(default)]
    pub file_browser_size_width: Option<f32>,
    #[serde(default)]
    pub file_browser_modified_width: Option<f32>,

    #[serde(default = "default_grid_cell_size")]
    pub sample_map_cell_size: f32,
    #[serde(default = "default_grid_cell_size")]
    pub note_map_cell_size: f32,

    #[serde(default = "default_true")]
    pub confirm_on_exit: bool,
}

fn default_col_vis() -> bool { true }

fn default_row_highlight_minor() -> u8 { 4 }
fn default_row_highlight_major() -> u8 { 16 }

fn default_font_size() -> u32 { 12 }
fn default_zoom() -> f32 { 1.0 }
fn default_scroll_speed() -> f32 { 1.0 }
fn default_visible_channels() -> usize { 16 }
fn default_true() -> bool { true }
fn default_amplify() -> f32 { 2.0 }
fn default_theme() -> String { "DarkModern".to_string() }
fn default_spacing_mode() -> String { "normal".to_string() }
fn default_file_browser_view_mode() -> String { "details".to_string() }
fn default_file_browser_sort_by() -> String { "name".to_string() }
fn default_interpolation() -> String { "Linear".to_string() }
fn default_limiter() -> String { "HardClip".to_string() }
fn default_sample_length_bg() -> bool { false }

fn default_grid_cell_size() -> f32 { 28.0 }

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            last_dirs: HashMap::new(),
            last_selections: HashMap::new(),
            last_file_path: None,
            favorites: Vec::new(),

            default_sample_paths: Vec::new(),
            default_mod_path: None,
            default_instrument_path: None,
            default_wav_path: None,
            sample_export_bit_depth: None,
            default_project_path: None,

            editor_font_size: default_font_size(),
            zoom_factor: default_zoom(),
            scroll_speed: default_scroll_speed(),
            visible_channels: default_visible_channels(),
            show_row_numbers: true,
            show_hex_row_numbers: false,
            snap_to_grid: false,
            follow_playback_default: true,

            default_amplify_factor: default_amplify(),

            auto_backup_interval_secs: 0,
            backup_directory: None,

            theme_preset: default_theme(),
            spacing_mode: default_spacing_mode(),
            debug: false,
            row_highlight_minor: default_row_highlight_minor(),
            row_highlight_major: default_row_highlight_major(),
            default_interpolation: default_interpolation(),
            limiter_mode: default_limiter(),
            output_device_name: None,
            preferred_sample_rate: None,

            file_browser_view_mode: default_file_browser_view_mode(),
            file_browser_sort_by: default_file_browser_sort_by(),
            file_browser_sort_desc: false,
            sample_length_bg: default_sample_length_bg(),
            col_vis_note: true,
            col_vis_instrument: true,
            col_vis_volume: true,
            col_vis_effect: true,
            window_width: None,
            window_height: None,
            instrument_list_width: None,
            instrument_envelope_height: None,
            sample_list_width: None,
            sample_waveform_height: None,
            order_list_width: None,
            file_browser_name_width: None,
            file_browser_dur_width: None,
            file_browser_type_width: None,
            file_browser_size_width: None,
            file_browser_modified_width: None,
            sample_map_cell_size: default_grid_cell_size(),
            note_map_cell_size: default_grid_cell_size(),
            confirm_on_exit: true,
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("htrk")
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_file();
        if !path.exists() {
            return Self::default();
        }
        let data = match fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => return Self::default(),
        };
        match toml::from_str(&data) {
            Ok(cfg) => cfg,
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let dir = Self::config_dir();
        let _ = fs::create_dir_all(&dir);
        let data = match toml::to_string_pretty(self) {
            Ok(d) => d,
            Err(_) => return,
        };
        let _ = fs::write(Self::config_file(), data);
    }

    pub fn get_last_dir(&self, mode: BrowserMode) -> Option<PathBuf> {
        let key = mode_key(mode);
        self.last_dirs.get(&key).and_then(|s| {
            let p = PathBuf::from(s);
            if p.is_dir() { Some(p) } else { None }
        })
    }

    pub fn set_last_dir(&mut self, mode: BrowserMode, path: PathBuf) {
        let key = mode_key(mode);
        self.last_dirs.insert(key, path.to_string_lossy().into_owned());
    }

    pub fn get_last_selection(&self, mode: BrowserMode, path: &Path) -> Option<(usize, usize)> {
        let key = format!("{}:{}", mode_key(mode), path.to_string_lossy());
        self.last_selections.get(&key).map(|(s, p)| (*s as usize, *p as usize))
    }

    pub fn set_last_selection(&mut self, mode: BrowserMode, path: &Path, selected_index: usize, page: usize) {
        let key = format!("{}:{}", mode_key(mode), path.to_string_lossy());
        self.last_selections.insert(key, (selected_index as u32, page as u32));
    }

    pub fn get_backup_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.backup_directory {
            let p = PathBuf::from(dir);
            if p.is_dir() {
                return p;
            }
        }
        dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("htrk_backups")
    }

    pub fn get_default_dir(&self, mode: BrowserMode) -> Option<PathBuf> {
        match mode {
            BrowserMode::Samples => {
                for s in &self.default_sample_paths {
                    let p = PathBuf::from(s);
                    if p.is_dir() {
                        return Some(p);
                    }
                }
                None
            }
            BrowserMode::Modules => self.default_mod_path.as_ref().and_then(|s| {
                let p = PathBuf::from(s);
                if p.is_dir() { Some(p) } else { None }
            }),
            BrowserMode::Instruments => self.default_instrument_path.as_ref().and_then(|s| {
                let p = PathBuf::from(s);
                if p.is_dir() { Some(p) } else { None }
            }),
            BrowserMode::Projects => self.default_project_path.as_ref().and_then(|s| {
                let p = PathBuf::from(s);
                if p.is_dir() { Some(p) } else { None }
            }),
        }
    }

    pub fn get_sample_export_bit_depth(&self) -> u8 {
        self.sample_export_bit_depth.unwrap_or(16)
    }

    pub fn set_sample_export_bit_depth(&mut self, depth: u8) {
        self.sample_export_bit_depth = Some(depth);
    }

    pub fn get_spacing_mode(&self) -> SpacingMode {
        SpacingMode::from_str(&self.spacing_mode).unwrap_or(SpacingMode::Normal)
    }

    pub fn set_spacing_mode(&mut self, mode: SpacingMode) {
        self.spacing_mode = mode.as_str().to_string();
    }

    pub fn get_file_browser_view_mode(&self) -> crate::ui::file_browser::ViewMode {
        crate::ui::file_browser::ViewMode::from_str(&self.file_browser_view_mode)
            .unwrap_or(crate::ui::file_browser::ViewMode::Details)
    }

    pub fn set_file_browser_view_mode(&mut self, mode: crate::ui::file_browser::ViewMode) {
        self.file_browser_view_mode = mode.as_str().to_string();
    }

    pub fn get_file_browser_sort_by(&self) -> crate::ui::file_browser::SortBy {
        crate::ui::file_browser::SortBy::from_str(&self.file_browser_sort_by)
            .unwrap_or(crate::ui::file_browser::SortBy::Name)
    }

    pub fn set_file_browser_sort_by(&mut self, sort: crate::ui::file_browser::SortBy) {
        self.file_browser_sort_by = sort.as_str().to_string();
    }

    pub fn get_file_browser_sort_desc(&self) -> bool {
        self.file_browser_sort_desc
    }

    pub fn set_file_browser_sort_desc(&mut self, desc: bool) {
        self.file_browser_sort_desc = desc;
    }

    pub fn get_sample_length_bg(&self) -> bool {
        self.sample_length_bg
    }

    pub fn set_sample_length_bg(&mut self, enabled: bool) {
        self.sample_length_bg = enabled;
    }

    pub fn toggle_sample_length_bg(&mut self) {
        self.sample_length_bg = !self.sample_length_bg;
    }

    pub fn get_col_vis(&self) -> crate::ui::pattern_grid::ColumnVisibility {
        crate::ui::pattern_grid::ColumnVisibility {
            note: self.col_vis_note,
            instrument: self.col_vis_instrument,
            volume: self.col_vis_volume,
            effect: self.col_vis_effect,
        }
    }

    pub fn set_col_vis(&mut self, col_vis: crate::ui::pattern_grid::ColumnVisibility) {
        self.col_vis_note = col_vis.note;
        self.col_vis_instrument = col_vis.instrument;
        self.col_vis_volume = col_vis.volume;
        self.col_vis_effect = col_vis.effect;
    }
}

fn mode_key(mode: BrowserMode) -> String {
    match mode {
        BrowserMode::Modules => "modules".to_string(),
        BrowserMode::Samples => "samples".to_string(),
        BrowserMode::Instruments => "instruments".to_string(),
        BrowserMode::Projects => "projects".to_string(),
    }
}
