use std::collections::HashMap;
use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use eframe::egui as egui_module;
use egui_module::Ui;

const ENTRIES_PER_PAGE: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BrowserMode {
    Modules,
    Samples,
    Instruments,
    Projects,
}

impl BrowserMode {
    pub fn extensions(&self) -> Vec<&'static str> {
        match self {
            BrowserMode::Modules => vec!["htk", "it", "xm", "s3m", "mod"],
            BrowserMode::Samples => vec!["wav", "raw"],
            BrowserMode::Instruments => vec!["hti"],
            BrowserMode::Projects => vec!["htk"],
        }
    }
    
    pub fn tab_label(&self) -> &'static str {
        match self {
            BrowserMode::Modules => "Modules",
            BrowserMode::Samples => "Samples",
            BrowserMode::Instruments => "Instruments",
            BrowserMode::Projects => "Projects",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub extension: String,
    pub is_hidden: bool,
}

impl FileEntry {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_string();
        let is_dir = path.is_dir();
        let is_hidden = name.starts_with('.');
        
        let (size, extension) = if is_dir {
            (0, String::new())
        } else {
            let metadata = fs::metadata(path).ok()?;
            let ext = path.extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            (metadata.len(), ext)
        };
        
        Some(FileEntry {
            name,
            path: path.to_path_buf(),
            is_dir,
            size,
            extension,
            is_hidden,
        })
    }
    
    pub fn matches_filter(&self, extensions: &[&str]) -> bool {
        if self.is_dir { return true; }
        extensions.iter().any(|ext| self.extension == *ext)
    }
    
    pub fn format_size(&self) -> String {
        if self.is_dir {
            "--".to_string()
        } else if self.size < 1024 {
            format!("{}B", self.size)
        } else if self.size < 1024 * 1024 {
            format!("{:.1}KB", self.size as f64 / 1024.0)
        } else {
            format!("{:.1}MB", self.size as f64 / (1024.0 * 1024.0))
        }
    }
}

pub struct FileBrowser {
    pub mode: BrowserMode,
    pub show: bool,
    pub current_path: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected_index: usize,
    pub page: usize,
    pub show_hidden: bool,
    pub preview_enabled: bool,
    pub last_dirs: HashMap<BrowserMode, PathBuf>,
    pub project_root: PathBuf,
}

impl Default for FileBrowser {
    fn default() -> Self {
        let project_root = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        
        Self {
            mode: BrowserMode::Modules,
            show: false,
            current_path: project_root.clone(),
            entries: Vec::new(),
            selected_index: 0,
            page: 0,
            show_hidden: false,
            preview_enabled: false,
            last_dirs: HashMap::new(),
            project_root,
        }
    }
}

impl FileBrowser {
    pub fn open_browser(mode: BrowserMode, project_root: Option<PathBuf>) -> Self {
        let mut root = project_root
            .or_else(|| dirs::home_dir())
            .unwrap_or_else(|| PathBuf::from("."));
        
        // If the path doesn't exist, fall back to current directory
        if !root.exists() {
            root = PathBuf::from(".");
        }
        
        let mut browser = Self {
            mode,
            show: false,
            current_path: root.clone(),
            ..Default::default()
        };
        browser.project_root = root;
        
        // Restore last directory for this mode
        if let Some(last) = browser.last_dirs.get(&mode) {
            if last.is_dir() && last.exists() {
                browser.current_path = last.clone();
            }
        }
        
        let _ = browser.refresh();
        browser
    }
    
    pub fn open(&mut self, mode: BrowserMode) {
        self.mode = mode;
        self.show = true;
        self.selected_index = 0;
        self.page = 0;
        self.preview_enabled = false;
        
        // Restore last directory for this mode
        if let Some(last) = self.last_dirs.get(&mode) {
            if last.is_dir() && last.exists() {
                self.current_path = last.clone();
            }
        }
        
        let _ = self.refresh();
    }
    
    pub fn close(&mut self) {
        self.show = false;
    }

    pub fn restore_last_dirs(&mut self, config: &crate::app_config::AppConfig) {
        for mode in [BrowserMode::Modules, BrowserMode::Samples, BrowserMode::Instruments, BrowserMode::Projects] {
            if let Some(path) = config.get_last_dir(mode) {
                let path_clone = path.clone();
                if self.mode == mode && path.is_dir() {
                    self.current_path = path;
                }
                self.last_dirs.insert(mode, path_clone);
            }
        }
    }
    
    pub fn refresh(&mut self) -> std::io::Result<()> {
        self.entries.clear();
        self.selected_index = 0;
        self.page = 0;
        
        if !self.current_path.exists() {
            return Ok(());
        }
        
        let entries: Vec<_> = fs::read_dir(&self.current_path)?
            .filter_map(|e| e.ok())
            .filter_map(|e| FileEntry::from_path(&e.path()))
            .filter(|e| {
                if e.name.starts_with('.') && !self.show_hidden {
                    return false;
                }
                e.matches_filter(&self.mode.extensions())
            })
            .collect();
        
        // Sort: directories first, then files, alphabetically
        let mut dirs: Vec<_> = entries.iter().filter(|e| e.is_dir).cloned().collect();
        let mut files: Vec<_> = entries.iter().filter(|e| !e.is_dir).cloned().collect();
        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        
        self.entries = dirs.into_iter().chain(files.into_iter()).collect();
        
        // Save current directory for this mode
        self.last_dirs.insert(self.mode, self.current_path.clone());
        
        Ok(())
    }
    
    pub fn total_pages(&self) -> usize {
        (self.entries.len() + ENTRIES_PER_PAGE - 1) / ENTRIES_PER_PAGE
    }
    
    pub fn current_page_entries(&self) -> &[FileEntry] {
        let start = self.page * ENTRIES_PER_PAGE;
        let end = (start + ENTRIES_PER_PAGE).min(self.entries.len());
        if start >= self.entries.len() {
            &[]
        } else {
            &self.entries[start..end]
        }
    }
    
    pub fn has_next_page(&self) -> bool {
        self.page + 1 < self.total_pages()
    }
    
    pub fn has_prev_page(&self) -> bool {
        self.page > 0
    }
    
    pub fn next_page(&mut self) {
        if self.has_next_page() {
            self.page += 1;
            self.selected_index = 0;
        }
    }
    
    pub fn prev_page(&mut self) {
        if self.has_prev_page() {
            self.page -= 1;
            self.selected_index = 0;
        }
    }
    
    pub fn navigate_up(&mut self) -> bool {
        if let Some(parent) = self.current_path.parent() {
            if parent.exists() {
                self.current_path = parent.to_path_buf();
                let _ = self.refresh();
                return true;
            }
        }
        false
    }
    
    pub fn navigate_home(&mut self) {
        if self.project_root.exists() && self.project_root != self.current_path {
            self.current_path = self.project_root.clone();
            let _ = self.refresh();
        }
    }
    
    pub fn navigate_to(&mut self, path: &Path) {
        if path.is_dir() {
            self.current_path = path.to_path_buf();
            let _ = self.refresh();
        }
    }
    
    pub fn selected_entry(&self) -> Option<&FileEntry> {
        let page_entries = self.current_page_entries();
        if self.selected_index < page_entries.len() {
            Some(&page_entries[self.selected_index])
        } else {
            None
        }
    }
    
    pub fn select_index(&mut self, index: usize) {
        let page_entries = self.current_page_entries();
        if index < page_entries.len() {
            self.selected_index = index;
        }
    }
    
    pub fn move_selection_down(&mut self) {
        let page_entries = self.current_page_entries();
        if self.selected_index < page_entries.len().saturating_sub(1) {
            self.selected_index += 1;
        } else if self.has_next_page() {
            self.next_page();
        }
    }
    
    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
} else if self.has_prev_page() {
            self.prev_page();
            let page_entries = self.current_page_entries();
            self.selected_index = page_entries.len().saturating_sub(1);
        }
    }

    pub fn render(&mut self, ui: &mut Ui) -> Option<PathBuf> {
        if !self.show {
            return None;
        }

        let entries: Vec<_> = {
            let extensions = self.mode.extensions();
            let show_hidden = self.show_hidden;
            self.entries.iter()
                .filter(|e| {
                    if !show_hidden && e.is_hidden {
                        return false;
                    }
                    e.matches_filter(&extensions)
                })
                .enumerate()
                .map(|(i, e)| (i, e.clone()))
                .collect()
        };

        let mut selected_path: Option<PathBuf> = None;

        egui_module::Frame::default()
            .fill(ui.style().visuals.window_fill)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let modes = [
                        BrowserMode::Modules,
                        BrowserMode::Samples,
                        BrowserMode::Instruments,
                        BrowserMode::Projects,
                    ];
                    for mode in modes {
                        let label = mode.tab_label();
                        let is_active = self.mode == mode;
                        if ui.selectable_label(is_active, label).clicked() {
                            if self.mode != mode {
                                self.mode = mode;
                                let _ = self.refresh();
                            }
                        }
                    }
                    ui.with_layout(egui_module::Layout::right_to_left(egui_module::Align::Center), |ui| {
                        if ui.checkbox(&mut self.show_hidden, "Hidden").clicked() {
                            let _ = self.refresh();
                        }
                        if ui.button("✕").clicked() {
                            self.close();
                        }
                    });
                });

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("⬆").clicked() {
                        self.navigate_up();
                    }
                    if ui.button("🏠").clicked() {
                        self.navigate_home();
                    }
                    ui.separator();
                    let current_drive = self.current_path.to_string_lossy()
                        .chars().next().unwrap_or('C').to_ascii_uppercase();
                    for letter in b'A'..=b'Z' {
                        let drive = format!("{}:\\", letter as char);
                        let path = PathBuf::from(&drive);
                        if path.is_dir() {
                            let label = format!("{}:", letter as char);
                            let is_current = letter as char == current_drive;
                            let response = ui.selectable_label(is_current, &label);
                            if response.clicked() {
                                self.navigate_to(&path);
                            }
                        }
                    }
                    ui.separator();
                    ui.label(self.current_path.to_string_lossy());
                });

                ui.separator();

                egui_module::ScrollArea::vertical().show(ui, |ui| {
                    for (i, entry) in &entries {
                        let is_selected = self.selected_index == *i;
                        let response = ui.selectable_label(is_selected, entry.name.as_str());
                        if response.clicked() {
                            self.select_index(*i);
                        }
                        if response.double_clicked() {
                            if entry.is_dir {
                                self.navigate_to(&entry.path);
                            } else {
                                selected_path = Some(entry.path.clone());
                            }
                        }
                    }
                });
            });
        
        if let Some(path) = selected_path {
            self.last_dirs.insert(self.mode, self.current_path.clone());
            self.close();
            return Some(path);
        }

        None
    }
}