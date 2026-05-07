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
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            last_dirs: HashMap::new(),
            last_file_path: None,
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
}

fn mode_key(mode: BrowserMode) -> String {
    match mode {
        BrowserMode::Modules => "modules".to_string(),
        BrowserMode::Samples => "samples".to_string(),
        BrowserMode::Instruments => "instruments".to_string(),
        BrowserMode::Projects => "projects".to_string(),
    }
}