use std::sync::{Arc, RwLock};
use eframe::egui;

use crate::mcp::library::{DirFilter, SampleLibrary};
use super::style::*;
use super::theme::TrackerTheme;

pub struct SampleLibraryState {
    pub open: bool,
    pub current_path: Option<String>,
    pub page: usize,
    pub page_size: usize,
    pub search_query: String,
    pub search_mode: bool,
    pub selected_path: Option<String>,
    pub status_message: Option<String>,
}

impl Default for SampleLibraryState {
    fn default() -> Self {
        Self {
            open: false,
            current_path: None,
            page: 0,
            page_size: 50,
            search_query: String::new(),
            search_mode: false,
            selected_path: None,
            status_message: None,
        }
    }
}

/// Draw the sample library dialog. Returns `Some(path)` when the user
/// clicks Import on an entry.
pub fn draw_sample_library(
    ctx: &egui::Context,
    state: &mut SampleLibraryState,
    library: &Arc<RwLock<SampleLibrary>>,
    _module: &crate::sequencer::Module,
    theme: &TrackerTheme,
) -> Option<String> {
    let mut import_path: Option<String> = None;
    let mut open = state.open;
    egui::Window::new("Sample Library")
        .id(egui::Id::new("sample_library"))
        .open(&mut open)
        .resizable(true)
        .default_size([640.0, 480.0])
        .min_width(400.0)
        .min_height(300.0)
        .show(ctx, |ui| {
            // Toolbar
            ui.horizontal(|ui| {
                if state.search_mode {
                    if ui.button("Browse").clicked() { state.search_mode = false; state.search_query.clear(); state.page = 0; }
                } else if ui.button("Search").clicked() { state.search_mode = true; state.page = 0; }
                ui.separator();
                if state.search_mode {
                    ui.add(egui::TextEdit::singleline(&mut state.search_query).hint_text("Search...").desired_width(200.0));
                } else if let Some(ref path) = state.current_path {
                    let label = if path.len() > 80 { format!("...{}", &path[path.len().saturating_sub(77)..]) } else { path.clone() };
                    ui.label(egui::RichText::new(label).size(FONT_CAPTION).color(theme.fg_dim));
                }
                if let Some(ref msg) = state.status_message {
                    ui.label(egui::RichText::new(msg).size(FONT_CAPTION).color(theme.fg_volume));
                }
            });
            ui.separator();

            // Body
            egui::ScrollArea::vertical().id_salt("sample_library_scroll").auto_shrink([false; 2]).show(ui, |ui| {
                if state.search_mode && !state.search_query.is_empty() {
                    let results = { let lib = library.read().unwrap(); lib.search(&state.search_query, None, state.page, state.page_size) };
                    draw_results(ui, state, results, theme, &mut import_path);
                } else if let Some(ref path) = state.current_path.clone() {
                    let filter = DirFilter::default();
                    let listing = { let mut lib = library.write().unwrap(); lib.list_dir(std::path::Path::new(&path), state.page, state.page_size, Some(&filter)) };
                    match listing {
                        Ok(l) => draw_dir(ui, state, &path, l, theme, &mut import_path),
                        Err(e) => { ui.colored_label(egui::Color32::from_rgb(255, 100, 100), &e); }
                    }
                } else {
                    let lib = library.read().unwrap();
                    if lib.roots.is_empty() {
                        ui.label("No library roots configured. Add paths in Settings > Paths.");
                    } else {
                        for root in &lib.roots {
                            let name = root.to_string_lossy().to_string();
                            if ui.button(egui::RichText::new(format!("📁 {}", name)).color(theme.fg_instrument)).clicked() {
                                state.current_path = Some(name); state.page = 0;
                            }
                        }
                    }
                }
            });
        });
    state.open = open;
    import_path
}

fn draw_dir(ui: &mut egui::Ui, state: &mut SampleLibraryState, path: &str, listing: crate::mcp::library::DirListing, theme: &TrackerTheme, import_path: &mut Option<String>) {
    // Parent directory
    if let Some(parent) = std::path::Path::new(path).parent() {
        let p = parent.to_string_lossy().to_string();
        if ui.button(egui::RichText::new("📁 ..").color(theme.fg_instrument)).clicked() {
            state.current_path = if p.is_empty() { None } else { Some(p) }; state.page = 0; return;
        }
    }
    // Subdirectories
    for sub in &listing.subdirectories {
        if ui.button(egui::RichText::new(format!("📁 {}", sub.name)).color(theme.fg_instrument)).clicked() {
            state.current_path = Some(sub.path.clone()); state.page = 0; return;
        }
    }
    // Samples
    if listing.samples.is_empty() && listing.subdirectories.is_empty() { ui.label("(empty)"); }
    for sample in &listing.samples {
        ui.horizontal(|ui| {
            let row_clicked = ui.add(egui::Label::new(egui::RichText::new(&sample.name).size(FONT_BODY)).sense(egui::Sense::click())).clicked();
            if row_clicked { state.selected_path = Some(sample.path.clone()); }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Import").clicked() { *import_path = Some(sample.path.clone()); }
                let mut parts: Vec<String> = Vec::new();
                if let Some(d) = sample.duration { parts.push(format!("{:.1}s", d)); }
                if let Some(sr) = sample.sample_rate { parts.push(format!("{}Hz", sr)); }
                if let Some(b) = sample.bit_depth { parts.push(format!("{}bit", b)); }
                if let Some(ref n) = sample.root_note { parts.push(n.clone()); }
                if let Some(ref c) = sample.category { parts.push(c.clone()); }
                ui.label(egui::RichText::new(parts.join(" | ")).size(FONT_CAPTION).color(theme.fg_dim));
            });
        });
        ui.separator();
    }
    // Pagination
    if listing.total_pages > 1 {
        ui.horizontal(|ui| {
            if ui.add_enabled(state.page > 0, egui::Button::new("Prev")).clicked() { state.page = state.page.saturating_sub(1); }
            ui.label(format!("Page {}/{} ({} total)", state.page + 1, listing.total_pages, listing.total_samples));
            if ui.add_enabled(state.page + 1 < listing.total_pages, egui::Button::new("Next")).clicked() { state.page += 1; }
        });
    }
}

fn draw_results(ui: &mut egui::Ui, state: &mut SampleLibraryState, results: crate::mcp::library::SearchResults, theme: &TrackerTheme, import_path: &mut Option<String>) {
    if results.results.is_empty() { ui.label("No results."); return; }
    for sample in &results.results {
        ui.horizontal(|ui| {
            let row_clicked = ui.add(egui::Label::new(egui::RichText::new(&sample.name).size(FONT_BODY)).sense(egui::Sense::click())).clicked();
            if row_clicked { state.selected_path = Some(sample.path.clone()); }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Import").clicked() { *import_path = Some(sample.path.clone()); }
                let mut parts: Vec<String> = Vec::new();
                if let Some(d) = sample.duration { parts.push(format!("{:.1}s", d)); }
                if let Some(ref n) = sample.root_note { parts.push(n.clone()); }
                if let Some(ref c) = sample.category { parts.push(c.clone()); }
                ui.label(egui::RichText::new(parts.join(" | ")).size(FONT_CAPTION).color(theme.fg_dim));
            });
        });
        ui.separator();
    }
    if results.total_pages > 1 {
        ui.horizontal(|ui| {
            if ui.add_enabled(state.page > 0, egui::Button::new("Prev")).clicked() { state.page = state.page.saturating_sub(1); }
            ui.label(format!("Page {}/{} ({} total)", state.page + 1, results.total_pages, results.total_results));
            if ui.add_enabled(state.page + 1 < results.total_pages, egui::Button::new("Next")).clicked() { state.page += 1; }
        });
    }
}
