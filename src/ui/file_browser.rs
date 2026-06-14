use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::hash::Hash;
use std::io::{Read, Seek};
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
            BrowserMode::Modules => vec!["htk", "it", "xm", "s3m", "mod", "669", "ult", "mmd1", "mmd3", "stm"],
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Details,
}

impl ViewMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "list" => Some(ViewMode::List),
            "details" => Some(ViewMode::Details),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ViewMode::List => "list",
            ViewMode::Details => "details",
        }
    }
}

impl Default for ViewMode {
    fn default() -> Self {
        ViewMode::Details
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Date,
    Size,
    Type,
}

impl SortBy {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "name" => Some(SortBy::Name),
            "date" => Some(SortBy::Date),
            "size" => Some(SortBy::Size),
            "type" => Some(SortBy::Type),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SortBy::Name => "name",
            SortBy::Date => "date",
            SortBy::Size => "size",
            SortBy::Type => "type",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SortBy::Name => "Name",
            SortBy::Date => "Date",
            SortBy::Size => "Size",
            SortBy::Type => "Type",
        }
    }
}

impl Default for SortBy {
    fn default() -> Self {
        SortBy::Name
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
    pub modified: Option<std::time::SystemTime>,
}

impl FileEntry {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_string();
        let is_dir = path.is_dir();
        let is_hidden = name.starts_with('.');

        let metadata = fs::metadata(path).ok()?;
        let modified = metadata.modified().ok();
        let (size, extension) = if is_dir {
            (0, String::new())
        } else {
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
            modified,
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

fn format_duration(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{:.1}s", seconds)
    } else {
        let mins = (seconds / 60.0) as u32;
        let secs = (seconds % 60.0) as u32;
        format!("{}:{:02}", mins, secs)
    }
}

fn format_date(time: std::time::SystemTime) -> String {
    let secs = time.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days_since_epoch = secs / 86400;
    let year_base = days_since_epoch / 365;
    let remaining_days = days_since_epoch % 365;
    let year = 1970 + year_base as i64;
    let month = (remaining_days / 30).max(1).min(12) as u32;
    let day = (remaining_days % 30).max(1).min(31) as u32;
    format!("{:02}/{:02}/{:02}", month, day, (year % 100) as u32)
}

const AUDIO_EXTS: [&str; 9] = ["wav", "mp3", "ogg", "flac", "it", "xm", "s3m", "mod", "669"];

pub fn is_audio_entry(entry: &FileEntry) -> bool {
    !entry.is_dir && AUDIO_EXTS.contains(&entry.extension.as_str())
}

fn detail_cell(ui: &mut egui_module::Ui, text: &str, width: f32, color: egui_module::Color32) {
    ui.add_sized(
        [width, 14.0],
        egui_module::Label::new(
            egui_module::RichText::new(text).font(egui_module::FontId::monospace(9.0)).color(color),
        )
        .truncate(),
    );
}

fn wav_duration(path: &Path) -> Option<f64> {
    let mut file = fs::File::open(path).ok()?;
    let mut buf = [0u8; 12];
    file.read_exact(&mut buf).ok()?;
    if &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
        return None;
    }

    let mut sample_rate: u32 = 0;
    let mut bytes_per_sample: u32 = 0;
    let mut num_channels: u32 = 0;
    let mut data_size: u64 = 0;
    let mut found_fmt = false;
    let mut found_data = false;

    loop {
        let mut header = [0u8; 8];
        if file.read_exact(&mut header).is_err() {
            break;
        }
        let chunk_id = &header[0..4];
        let chunk_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        if chunk_id == b"fmt " {
            let mut fmt_buf = [0u8; 16];
            if file.read_exact(&mut fmt_buf).is_err() {
                break;
            }
            let audio_format = u16::from_le_bytes([fmt_buf[0], fmt_buf[1]]);
            if audio_format != 1 && audio_format != 3 {
                return None;
            }
            num_channels = u32::from_le_bytes([fmt_buf[2], fmt_buf[3], 0, 0]);
            sample_rate = u32::from_le_bytes([fmt_buf[4], fmt_buf[5], fmt_buf[6], fmt_buf[7]]);
            let _byte_rate = u32::from_le_bytes([fmt_buf[8], fmt_buf[9], fmt_buf[10], fmt_buf[11]]);
            let _block_align = u16::from_le_bytes([fmt_buf[12], fmt_buf[13]]);
            let bits_per_sample = u32::from_le_bytes([fmt_buf[14], fmt_buf[15], 0, 0]);
            bytes_per_sample = (bits_per_sample / 8).max(1);
            found_fmt = true;

            if chunk_size > 16 {
                let skip = chunk_size - 16;
                if file.seek(std::io::SeekFrom::Current(skip as i64)).is_err() {
                    break;
                }
            }
        } else if chunk_id == b"data" {
            data_size = chunk_size as u64;
            found_data = true;
            break;
        } else {
            if file.seek(std::io::SeekFrom::Current(chunk_size as i64)).is_err() {
                break;
            }
        }
    }

    if found_fmt && found_data && sample_rate > 0 && num_channels > 0 && bytes_per_sample > 0 {
        let total_samples = data_size / (bytes_per_sample * num_channels) as u64;
        Some(total_samples as f64 / sample_rate as f64)
    } else {
        None
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
    pub search_query: String,
    pub favorites: Vec<PathBuf>,
    duration_cache: HashMap<PathBuf, Option<f64>>,
    pub view_mode: ViewMode,
    pub sort_by: SortBy,
    pub sort_descending: bool,
    pub preview_sample: Option<(PathBuf, Arc<Vec<f32>>, u32)>,
    pub preview_requested: bool,
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
            search_query: String::new(),
            favorites: Vec::new(),
            duration_cache: HashMap::new(),
            view_mode: ViewMode::Details,
            sort_by: SortBy::Name,
            sort_descending: false,
            preview_sample: None,
            preview_requested: false,
        }
    }
}

impl FileBrowser {
    pub fn open_browser(mode: BrowserMode, project_root: Option<PathBuf>) -> Self {
        let mut root = project_root
            .or_else(|| dirs::home_dir())
            .unwrap_or_else(|| PathBuf::from("."));

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

        if let Some(last) = browser.last_dirs.get(&mode) {
            if last.is_dir() && last.exists() {
                browser.current_path = last.clone();
            }
        }

        let _ = browser.refresh(None);
        browser
    }

    pub fn open(&mut self, mode: BrowserMode, config: &mut crate::app_config::AppConfig) {
        self.mode = mode;
        self.show = true;
        self.preview_enabled = false;
        self.search_query.clear();

        if let Some(last) = self.last_dirs.get(&mode) {
            if last.is_dir() && last.exists() {
                self.current_path = last.clone();
            }
        }

        if let Some((idx, pg)) = config.get_last_selection(mode, &self.current_path) {
            self.selected_index = idx;
            self.page = pg;
        } else {
            self.selected_index = 0;
            self.page = 0;
        }

        self.view_mode = config.get_file_browser_view_mode();
        self.sort_by = config.get_file_browser_sort_by();
        self.sort_descending = config.get_file_browser_sort_desc();

        let _ = self.refresh(Some(config));
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
            } else if let Some(path) = config.get_default_dir(mode) {
                let path_clone = path.clone();
                if self.mode == mode && path.is_dir() {
                    self.current_path = path;
                }
                self.last_dirs.insert(mode, path_clone);
            }
        }
    }

    pub fn restore_favorites(&mut self, paths: &[String]) {
        self.favorites = paths.iter()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .collect();
    }

    pub fn save_favorites(&self) -> Vec<String> {
        self.favorites.iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    pub fn refresh(&mut self, config: Option<&mut crate::app_config::AppConfig>) -> std::io::Result<()> {
        let old_path = self.current_path.clone();
        let old_selection = (self.selected_index, self.page);

        self.entries.clear();
        self.selected_index = 0;
        self.page = 0;
        self.duration_cache.clear();
        self.preview_sample = None;

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

        let mut dirs: Vec<_> = entries.iter().filter(|e| e.is_dir).cloned().collect();
        let mut files: Vec<_> = entries.iter().filter(|e| !e.is_dir).cloned().collect();

        let sort_fn = |a: &FileEntry, b: &FileEntry| {
            let cmp = match self.sort_by {
                SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortBy::Date => a.modified.cmp(&b.modified),
                SortBy::Size => a.size.cmp(&b.size),
                SortBy::Type => a.extension.cmp(&b.extension),
            };
            if self.sort_descending { cmp.reverse() } else { cmp }
        };

        dirs.sort_by(&sort_fn);
        files.sort_by(&sort_fn);

        self.entries = dirs.into_iter().chain(files.into_iter()).collect();

        self.last_dirs.insert(self.mode, self.current_path.clone());

        if let Some(cfg) = config {
            if let Some((idx, pg)) = cfg.get_last_selection(self.mode, &self.current_path) {
                self.selected_index = idx.min(self.entries.len().saturating_sub(1));
                self.page = pg.min(self.total_pages().saturating_sub(1));
            }
            cfg.set_last_selection(self.mode, &old_path, old_selection.0, old_selection.1);
        }

        Ok(())
    }

    fn filtered_entries(&self) -> Vec<(usize, &FileEntry)> {
        let query = self.search_query.trim().to_lowercase();
        self.entries.iter()
            .enumerate()
            .filter(|(_, e)| {
                if query.is_empty() {
                    true
                } else {
                    e.name.to_lowercase().contains(&query)
                }
            })
            .collect()
    }

    pub fn total_pages(&self) -> usize {
        let total = self.filtered_entries().len();
        (total + ENTRIES_PER_PAGE - 1) / ENTRIES_PER_PAGE
    }

    pub fn current_page_entries(&self) -> Vec<(usize, &FileEntry)> {
        let filtered = self.filtered_entries();
        let start = self.page * ENTRIES_PER_PAGE;
        let end = (start + ENTRIES_PER_PAGE).min(filtered.len());
        if start >= filtered.len() {
            Vec::new()
        } else {
            filtered[start..end].to_vec()
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
                let _ = self.refresh(None);
                return true;
            }
        }
        false
    }

    pub fn navigate_home(&mut self) {
        if self.project_root.exists() && self.project_root != self.current_path {
            self.current_path = self.project_root.clone();
            let _ = self.refresh(None);
        }
    }

    pub fn navigate_to(&mut self, path: &Path) {
        if path.is_dir() {
            self.current_path = path.to_path_buf();
            let _ = self.refresh(None);
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        let page_entries = self.current_page_entries();
        if self.selected_index < page_entries.len() {
            Some(page_entries[self.selected_index].1)
        } else {
            None
        }
    }

    pub fn get_preview_data(&mut self, path: &PathBuf) -> Option<(Arc<Vec<f32>>, u32)> {
        if let Some((p, data, rate)) = &self.preview_sample {
            if p == path {
                return Some((Arc::clone(data), *rate));
            }
        }
        let bytes = fs::read(path).ok()?;
        let sample = crate::formats::wav::import_wav(&bytes).ok()?;
        let data = sample.data;
        let rate = sample.sample_rate;
        self.preview_sample = Some((path.clone(), Arc::clone(&data), rate));
        Some((data, rate))
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

    fn get_duration(&mut self, path: &PathBuf) -> Option<f64> {
        if let Some(&dur) = self.duration_cache.get(path) {
            return dur;
        }
        let dur = wav_duration(path);
        self.duration_cache.insert(path.clone(), dur);
        dur
    }

    fn is_current_path_favorite(&self) -> bool {
        self.favorites.iter().any(|p| p == &self.current_path)
    }

    fn add_favorite(&mut self) {
        if !self.is_current_path_favorite() {
            self.favorites.push(self.current_path.clone());
        }
    }

    fn remove_favorite(&mut self, path: &Path) {
        self.favorites.retain(|p| p != path);
    }

    pub fn render(&mut self, ui: &mut Ui, config: Option<&mut crate::app_config::AppConfig>, theme: crate::ui::TrackerTheme) -> Option<PathBuf> {
        if !self.show {
            return None;
        }

        let page_entries: Vec<(usize, FileEntry)> = self.current_page_entries()
            .into_iter()
            .map(|(i, e)| (i, e.clone()))
            .collect();
        let filtered_count = self.filtered_entries().len();
        let total_count = self.entries.len();

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
                                if let Some(last) = self.last_dirs.get(&mode) {
                                    if last.is_dir() && last.exists() {
                                        self.current_path = last.clone();
                                    }
                                }
                                let _ = self.refresh(None);
                            }
                        }
                    }
                    ui.separator();
                    let view_icon = if self.view_mode == ViewMode::Details { "☰" } else { "≡" };
                    if ui.button(view_icon).clicked() {
                        self.view_mode = if self.view_mode == ViewMode::List {
                            ViewMode::Details
                        } else {
                            ViewMode::List
                        };
                    }
                    egui_module::ComboBox::from_id_salt("file_sort")
                        .selected_text(self.sort_by.label())
                        .show_ui(ui, |ui| {
                            for sort in [SortBy::Name, SortBy::Date, SortBy::Size, SortBy::Type] {
                                if ui.selectable_label(self.sort_by == sort, sort.label()).clicked() {
                                    self.sort_by = sort;
                                }
                            }
                        });
                    let dir_icon = if self.sort_descending { "↓" } else { "↑" };
                    if ui.button(dir_icon).clicked() {
                        self.sort_descending = !self.sort_descending;
                    }
                    ui.with_layout(egui_module::Layout::right_to_left(egui_module::Align::Center), |ui| {
                        if ui.checkbox(&mut self.show_hidden, "Hidden").clicked() {
                            let _ = self.refresh(None);
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

                if !self.favorites.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui_module::RichText::new("Favorites:")
                                .font(egui_module::FontId::proportional(10.0))
                                .color(theme.fg_dim),
                        );
                        for fav in self.favorites.clone().iter() {
                            let name = fav.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("?");
                            let short = if name.len() > 18 {
                                format!("{}...", &name[..15])
                            } else {
                                name.to_string()
                            };
                            let is_current = *fav == self.current_path;
                            let btn = if is_current {
                                egui_module::RichText::new(format!("★ {}", short))
                                    .color(theme.order_selected)
                            } else {
                                egui_module::RichText::new(format!("☆ {}", short))
                                    .color(theme.fg_dim)
                            };
                            if ui.button(btn).clicked() {
                                self.navigate_to(fav);
                            }
                        }
                    });
                    ui.separator();
                }

                ui.horizontal(|ui| {
                    if self.is_current_path_favorite() {
                        if ui.button("☆ Remove from favorites").clicked() {
                            self.remove_favorite(&self.current_path.clone());
                        }
                    } else {
                        if ui.button("★ Add to favorites").clicked() {
                            self.add_favorite();
                        }
                    }

                    ui.with_layout(egui_module::Layout::right_to_left(egui_module::Align::Center), |ui| {
                        if !self.search_query.is_empty() {
                            if ui.button("✕").clicked() {
                                self.search_query.clear();
                                self.page = 0;
                                self.selected_index = 0;
                            }
                        }
                        let count_text = if self.search_query.is_empty() {
                            format!("{} items", total_count)
                        } else {
                            format!("{}/{} items", filtered_count, total_count)
                        };
                        ui.label(
                            egui_module::RichText::new(count_text)
                                .font(egui_module::FontId::monospace(9.0))
                                .color(theme.fg_dim),
                        );
                    });
                });

                let search_response = ui.add(
                    egui_module::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Filter files...")
                        .desired_width(f32::INFINITY),
                );
                if search_response.changed() {
                    self.page = 0;
                    self.selected_index = 0;
                }

                ui.separator();

                egui_module::ScrollArea::vertical()
                    .max_height(ui.available_height() - 40.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        match self.view_mode {
                            ViewMode::List => {
                                egui_module::Grid::new("file_entries_list")
                                    .striped(true)
                                    .spacing(egui_module::vec2(2.0, 2.0))
                                    .show(ui, |ui| {
                                        for (vis_idx, (_orig_idx, entry)) in page_entries.iter().enumerate() {
                                            let is_selected = self.selected_index == vis_idx;
                                            let name_text = if entry.is_dir {
                                                format!("📁 {}", entry.name)
                                            } else {
                                                entry.name.clone()
                                            };
                                            let response = ui.selectable_label(is_selected, &name_text);

                                            if entry.is_dir {
                                                detail_cell(ui, "DIR", 36.0, theme.fg_dimmer);
                                                ui.end_row();
                                            } else {
                                                let audio_exts = ["wav", "mp3", "ogg", "flac", "it", "xm", "s3m", "mod", "669"];
                                                if audio_exts.contains(&entry.extension.as_str()) {
                                                    if let Some(dur) = self.get_duration(&entry.path) {
                                                        detail_cell(ui, &format_duration(dur), 56.0, theme.fg_instrument);
                                                    } else {
                                                        detail_cell(ui, "", 56.0, theme.fg_dim);
                                                    }
                                                } else {
                                                    detail_cell(ui, "", 56.0, theme.fg_dim);
                                                }
                                                detail_cell(ui, &entry.extension.to_uppercase(), 44.0, theme.fg_dim);
                                                detail_cell(ui, &entry.format_size(), 64.0, theme.fg_dim);
                                                if let Some(modified) = entry.modified {
                                                    detail_cell(ui, &format_date(modified), 76.0, theme.fg_dim);
                                                } else {
                                                    detail_cell(ui, "", 76.0, theme.fg_dim);
                                                }
                                                ui.end_row();
                                            }

                                            if response.clicked() {
                                                self.select_index(vis_idx);
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
                            }
                            ViewMode::Details => {
                                egui_module::Grid::new("file_entries_header")
                                    .spacing(egui_module::vec2(2.0, 2.0))
                                    .show(ui, |ui| {
                                        ui.label(egui_module::RichText::new("Name").strong());
                                        detail_cell(ui, "Dur", 56.0, theme.fg_dim);
                                        detail_cell(ui, "Type", 44.0, theme.fg_dim);
                                        detail_cell(ui, "Size", 64.0, theme.fg_dim);
                                        detail_cell(ui, "Modified", 76.0, theme.fg_dim);
                                        ui.end_row();
                                    });
                                ui.separator();
                                egui_module::Grid::new("file_entries_details")
                                    .striped(true)
                                    .spacing(egui_module::vec2(2.0, 2.0))
                                    .show(ui, |ui| {
                                        for (vis_idx, (_orig_idx, entry)) in page_entries.iter().enumerate() {
                                            let is_selected = self.selected_index == vis_idx;
                                            let name_text = if entry.is_dir {
                                                format!("📁 {}", entry.name)
                                            } else {
                                                entry.name.clone()
                                            };
                                            let response = ui.selectable_label(is_selected, &name_text);

                                            if entry.is_dir {
                                                detail_cell(ui, "", 56.0, theme.fg_dim);
                                                detail_cell(ui, "DIR", 44.0, theme.fg_dimmer);
                                                detail_cell(ui, "", 64.0, theme.fg_dim);
                                                detail_cell(ui, "", 76.0, theme.fg_dim);
                                            } else {
                                                let audio_exts = ["wav", "mp3", "ogg", "flac", "it", "xm", "s3m", "mod", "669"];
                                                if audio_exts.contains(&entry.extension.as_str()) {
                                                    if let Some(dur) = self.get_duration(&entry.path) {
                                                        detail_cell(ui, &format_duration(dur), 56.0, theme.fg_instrument);
                                                    } else {
                                                        detail_cell(ui, "", 56.0, theme.fg_dim);
                                                    }
                                                } else {
                                                    detail_cell(ui, "", 56.0, theme.fg_dim);
                                                }
                                                detail_cell(ui, &entry.extension.to_uppercase(), 44.0, theme.fg_dim);
                                                detail_cell(ui, &entry.format_size(), 64.0, theme.fg_dim);
                                                if let Some(modified) = entry.modified {
                                                    detail_cell(ui, &format_date(modified), 76.0, theme.fg_dim);
                                                } else {
                                                    detail_cell(ui, "", 76.0, theme.fg_dim);
                                                }
                                            }
                                            ui.end_row();

                                            if response.clicked() {
                                                self.select_index(vis_idx);
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
                            }
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    let sel_info = page_entries.iter()
                        .find(|(vis_idx, _)| *vis_idx == self.selected_index)
                        .map(|(_, e)| e.clone());
                    if let Some(ref entry) = sel_info {
                        if !entry.is_dir {
                            if ui.button("Open").clicked() {
                                selected_path = Some(entry.path.clone());
                            }
                            if is_audio_entry(entry) {
                                if ui.button("▶ Preview").clicked() {
                                    self.preview_requested = true;
                                }
                            }
                            ui.label(format!("Size: {}", entry.format_size()));
                            let audio_exts = ["wav", "mp3", "ogg", "flac", "it", "xm", "s3m", "mod", "669"];
                            if audio_exts.contains(&entry.extension.as_str()) {
                                if let Some(dur) = self.get_duration(&entry.path) {
                                    ui.label(format!("Duration: {}", format_duration(dur)));
                                }
                            }
                            if let Some(modified) = entry.modified {
                                ui.label(format!("Modified: {}", format_date(modified)));
                            }
                            ui.label(format!("Type: {}", entry.extension.to_uppercase()));
                        } else {
                            if ui.button("Open Folder").clicked() {
                                self.navigate_to(&entry.path);
                            }
                        }
                    }
                    ui.with_layout(egui_module::Layout::right_to_left(egui_module::Align::Center), |ui| {
                        if self.has_next_page() {
                            if ui.button("Next ▶").clicked() {
                                self.next_page();
                            }
                        }
                        if self.has_prev_page() {
                            if ui.button("◀ Prev").clicked() {
                                self.prev_page();
                            }
                        }
                        let page_label = format!("Page {}/{}", self.page + 1, self.total_pages().max(1));
                        ui.label(
                            egui_module::RichText::new(page_label)
                                .font(egui_module::FontId::monospace(9.0))
                                .color(theme.fg_dim),
                        );
                    });
                });
            });

        if let Some(path) = selected_path {
            self.last_dirs.insert(self.mode, self.current_path.clone());
            if let Some(cfg) = config {
                cfg.set_last_selection(self.mode, &self.current_path, self.selected_index, self.page);
                cfg.set_file_browser_view_mode(self.view_mode);
                cfg.set_file_browser_sort_by(self.sort_by);
                cfg.set_file_browser_sort_desc(self.sort_descending);
            }
            self.close();
            return Some(path);
        }

        None
    }
}
