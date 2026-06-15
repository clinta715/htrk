use eframe::egui;

pub mod automation_editor;
pub mod channel_headers;
pub mod envelope_editor;
pub mod file_browser;
pub mod help_screen;
pub mod instrument_editor;
pub mod menu_bar;
pub mod note_map;
pub mod oscilloscope;
pub mod order_list;
pub mod pattern_grid;
pub mod playback_view;
pub mod sample_editor;
pub mod sample_export_dialog;
pub mod sample_map;
pub mod sample_palette;
pub mod sendfx_editor;
pub mod settings_window;
pub mod status_bar;
pub mod theme;
pub mod transport;
pub mod waveform;
pub mod wav_export_window;

pub use theme::TrackerTheme;

const V_SPLITTER_W: f32 = 4.0;

pub fn draw_vertical_splitter(ui: &mut egui::Ui, total_w: f32, split: &mut f32, min: f32, max: f32, theme: &TrackerTheme) {
    let h = ui.available_height();
    let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(V_SPLITTER_W, h));
    // Wider invisible grab strip centered on the thin divider, so the handle is
    // easy to grab even though it only takes up 4px of layout width.
    let hit_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(10.0, h));
    let mut response = ui.interact(hit_rect, ui.id().with("v_splitter"), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, theme.splitter_bg);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(0.5, theme.splitter_border), egui::StrokeKind::Outside);

    let hover_or_drag = response.hovered() || response.dragged();
    if hover_or_drag {
        painter.rect_filled(rect, 2.0, theme.splitter_active);
    }
    response = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);

    ui.allocate_space(egui::vec2(V_SPLITTER_W, h));

    if response.dragged_by(egui::PointerButton::Primary) {
        let dx = response.drag_delta().x;
        *split = (*split + dx / total_w).clamp(min, max);
    }
}

pub fn draw_horizontal_splitter(ui: &mut egui::Ui, total_h: f32, split: &mut f32, min: f32, max: f32, theme: &TrackerTheme) {
    let w = ui.available_width();
    let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(w, V_SPLITTER_W));
    let hit_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(w, 10.0));
    let mut response = ui.interact(hit_rect, ui.id().with("h_splitter"), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, theme.splitter_bg);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(0.5, theme.splitter_border), egui::StrokeKind::Outside);

    let hover_or_drag = response.hovered() || response.dragged();
    if hover_or_drag {
        painter.rect_filled(rect, 2.0, theme.splitter_active);
    }
    response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);

    ui.allocate_space(egui::vec2(w, V_SPLITTER_W));

    if response.dragged_by(egui::PointerButton::Primary) {
        let dy = response.drag_delta().y;
        *split = (*split + dy / total_h).clamp(min, max);
    }
}
