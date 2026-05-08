use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ui::file_browser::BrowserMode;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub last_dirs: HashMap<String, String>,
    #[serde(default)]
    pub last_file_path: Option<String>,
    #[serde(default)]
    pub favorites: Vec<String>,

    #[serde(default)]
    pub default_sample_paths: Vec<String>,
    #[serde(default)]
    pub default_mod_path: Option<String>,
    #[serde(default)]
    pub default_xm_path: Option<String>,
    #[serde(default)]
    pub default_instrument_path: Option<String>,
    #[serde(default)]
    pub default_project_path: Option<String>,

    #[serde(default = "default_font_size")]
    pub editor_font_size: u32,
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
}

fn default_font_size() -> u32 { 12 }
fn default_scroll_speed() -> f32 { 1.0 }
fn default_visible_channels() -> usize { 16 }
fn default_true() -> bool { true }
fn default_amplify() -> f32 { 2.0 }
fn default_theme() -> String { "DarkModern".to_string() }

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            last_dirs: HashMap::new(),
            last_file_path: None,
            favorites: Vec::new(),

            default_sample_paths: Vec::new(),
            default_mod_path: None,
            default_xm_path: None,
            default_instrument_path: None,
            default_project_path: None,

            editor_font_size: default_font_size(),
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
}

fn mode_key(mode: BrowserMode) -> String {
    match mode {
        BrowserMode::Modules => "modules".to_string(),
        BrowserMode::Samples => "samples".to_string(),
        BrowserMode::Instruments => "instruments".to_string(),
        BrowserMode::Projects => "projects".to_string(),
    }
}
