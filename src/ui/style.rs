use eframe::egui;
use super::theme::TrackerTheme;

// ── Typography scale ────────────────────────────────────────────
// Use these constants throughout the UI instead of magic number `.size()` calls.

/// View titles — "INSTRUMENT 0A", "SAMPLE 01", section top-level names (16px)
pub const FONT_TITLE: f32 = 16.0;

/// Section headers within panels (13px)
pub const FONT_SECTION: f32 = 13.0;

/// Labels, status bar, dialog text, list items (11px)
pub const FONT_BODY: f32 = 11.0;

/// Pattern cell data; config-driven default (12px)
pub const FONT_DATA: f32 = 12.0;

/// Tooltips, hints, small controls (10px)
pub const FONT_CAPTION: f32 = 10.0;

/// File meta, axis labels, sub-detail (9px)
pub const FONT_DETAIL: f32 = 9.0;

/// Oscilloscope / envelope axis marks (7px)
pub const FONT_MICRO: f32 = 7.0;

// ── Spacing scale ───────────────────────────────────────────────
// Use these instead of inline magic numbers for padding/gaps.

/// Minimal separation between adjacent elements
pub const SP_XS: f32 = 2.0;

/// Default/compact padding
pub const SP_SM: f32 = 4.0;

/// Standard section gap
pub const SP_MD: f32 = 8.0;

/// Major separation between groups
pub const SP_LG: f32 = 12.0;

/// Fixed status bar height
pub const STATUS_BAR_H: f32 = 22.0;

/// Typical list/detail row height
pub const LIST_ROW_H: f32 = 16.0;

// ── Shared UI helpers ───────────────────────────────────────────

/// A section header label: strong, `FONT_SECTION`, colored with `theme.fg_instrument`.
/// Replaces the copy-pasted `section_header` in 4+ files.
pub fn section_header(ui: &mut egui::Ui, text: &str, theme: &TrackerTheme) {
    ui.label(
        egui::RichText::new(text)
            .size(FONT_SECTION)
            .strong()
            .color(theme.fg_instrument),
    );
}

/// A centered dialog window with consistent defaults.
/// Always centered, non-collapsible, with a title in Title Case.
/// Mark `resizable(true)` only for dynamic-content dialogs (file browser, phrase generator).
pub fn dialog(title: &str, id: &str) -> egui::Window<'static> {
    egui::Window::new(title)
        .id(egui::Id::new(id))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
}
