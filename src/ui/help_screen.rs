use eframe::egui;

pub fn draw_shortcuts_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Keyboard Shortcuts")
        .open(open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(500.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("F3 — Close this window")
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.separator();
                    ui.add_space(4.0);

                    section_header(ui, "NAVIGATION");
                    shortcut_row(ui, "Arrow Keys", "Move cursor");
                    shortcut_row(ui, "Shift + Arrow", "Extend selection");
                    shortcut_row(ui, "Alt + Arrow Up/Down", "Transpose selection ±1 semitone");
                    shortcut_row(ui, "Tab / Shift+Tab", "Next / previous channel");
                    shortcut_row(ui, "[ / ]", "Previous / next pattern");
                    shortcut_row(ui, "Page Up / Page Down", "Scroll 16 rows");
                    shortcut_row(ui, "Home / End", "Jump to first / last row");
                    ui.add_space(6.0);

                    section_header(ui, "NOTE ENTRY");
                    shortcut_row(ui, "Z S X D C V G B H N J M ,", "Lower octave notes (C–C)");
                    shortcut_row(ui, "Q 2 W 3 E R 5 T 6 Y 7 U", "Upper octave notes (C–B)");
                    shortcut_row(ui, ". (period)", "Note off (^^^)");
                    shortcut_row(ui, "0–9 A–F", "Hex digit entry (instrument/vol/effect)");
                    ui.add_space(6.0);

                    section_header(ui, "EDITING");
                    shortcut_row(ui, "Backspace", "Clear cell at cursor");
                    shortcut_row(ui, "Delete", "Clear cell + advance cursor");
                    shortcut_row(ui, "Insert", "Insert empty row");
                    shortcut_row(ui, "Alt + Delete", "Delete row");
                    shortcut_row(ui, "Ctrl+Z / Ctrl+Y", "Undo / Redo");
                    shortcut_row(ui, "Ctrl+C / Ctrl+X", "Copy / Cut selection");
                    shortcut_row(ui, "Ctrl+V", "Paste at cursor");
                    shortcut_row(ui, "Ctrl+A", "Select all");
                    shortcut_row(ui, "Escape", "Clear selection");
                    ui.add_space(6.0);

                    section_header(ui, "TRANSPORT");
                    shortcut_row(ui, "Space", "Play / Stop toggle");
                    shortcut_row(ui, "F5", "Play from start");
                    shortcut_row(ui, "F8", "Stop");
                    ui.add_space(6.0);

                    section_header(ui, "OTHER");
                    shortcut_row(ui, "F1 / F2", "Octave down / up");
                    shortcut_row(ui, "F3", "Toggle this help window");
                    shortcut_row(ui, "Ctrl+N / Ctrl+O", "New song / Open file");
                    shortcut_row(ui, "Ctrl+S / Ctrl+Shift+S", "Save / Save As");
                });
        });
}

fn section_header(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(100, 200, 255)),
    );
}

fn shortcut_row(ui: &mut egui::Ui, keys: &str, action: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(keys)
                .monospace()
                .size(12.0)
                .color(egui::Color32::from_rgb(255, 200, 100)),
        );
        ui.label(
            egui::RichText::new(action)
                .size(12.0)
                .color(egui::Color32::from_rgb(200, 200, 220)),
        );
    });
}
