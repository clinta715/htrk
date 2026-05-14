use eframe::egui;

pub fn draw_shortcuts_window(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Help & Keyboard Shortcuts")
        .open(open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(false)
        .default_width(600.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(600.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("F3 — Close this window")
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.separator();
                    ui.add_space(4.0);

                    ui.columns(2, |columns| {
                        columns[0].vertical(|ui| {
                            section_header(ui, "NAVIGATION");
                            shortcut_row(ui, "Arrow Keys", "Move cursor");
                            shortcut_row(ui, "Shift + Arrow", "Extend selection");
                            shortcut_row(ui, "Alt + Up/Down", "Transpose ±1 semitone");
                            shortcut_row(ui, "Tab / Shift+Tab", "Next / prev channel");
                            shortcut_row(ui, "Alt+Left/Right", "Skip to prev/next channel");
                            shortcut_row(ui, "- / =", "Prev / next pattern");
                            shortcut_row(ui, "[ / ]", "Prev / next pattern");
                            shortcut_row(ui, "PgUp / PgDn", "Scroll 16 rows");
                            shortcut_row(ui, "Home / End", "Jump to first / last row");
                            ui.add_space(8.0);

                            section_header(ui, "PATTERN EDITING");
                            shortcut_row(ui, "Ctrl+X / C / V", "Block Cut / Copy / Paste");
                            shortcut_row(ui, "Shift+F3 / F4 / F5", "Track Cut / Copy / Paste");
                            shortcut_row(ui, "Alt+F3 / F4 / F5", "Column Cut / Copy / Paste");
                            shortcut_row(ui, "Shift+Delete", "Clear entire track");
                            shortcut_row(ui, "Backspace", "Clear cell");
                            shortcut_row(ui, "Delete", "Clear + advance");
                            shortcut_row(ui, "Insert", "Insert empty row");
                            shortcut_row(ui, "Alt+Delete", "Delete row");
                            ui.add_space(8.0);

                            section_header(ui, "FILE");
                            shortcut_row(ui, "Ctrl+N", "New song");
                            shortcut_row(ui, "Ctrl+O", "Open module...");
                            shortcut_row(ui, "Ctrl+I", "Import sample...");
                            shortcut_row(ui, "Ctrl+Shift+I", "Import instrument...");
                            shortcut_row(ui, "Ctrl+S", "Save");
                            shortcut_row(ui, "Ctrl+Shift+S", "Save As...");
                            ui.add_space(8.0);

                            section_header(ui, "EDITING");
                            shortcut_row(ui, "Ctrl+Z / Y", "Undo / Redo");
                            shortcut_row(ui, "Ctrl+A", "Select all");
                            shortcut_row(ui, "Escape", "Clear selection");
                            ui.add_space(8.0);

                            section_header(ui, "TRANSPORT");
                            shortcut_row(ui, "Space", "Repeat last entry / Stop");
                            shortcut_row(ui, "F5", "Play from start");
                            shortcut_row(ui, "F6", "Play pattern");
                            shortcut_row(ui, "F7", "Play through order");
                            shortcut_row(ui, "F8", "Stop all");
                            shortcut_row(ui, "F9", "Play from current pos");
                            ui.add_space(8.0);

                            section_header(ui, "CHANNEL");
                            shortcut_row(ui, "F2", "Toggle record mode");
                            shortcut_row(ui, "Alt+M", "Toggle mute channel");
                            shortcut_row(ui, "Alt+S", "Toggle solo channel");
                            shortcut_row(ui, "Alt+Left/Right", "Skip to prev/next channel");
                            ui.add_space(8.0);

                            section_header(ui, "IT-STYLE FEATURES");
                            shortcut_row(ui, "Alt+0..9", "Set cursor skip value");
                            shortcut_row(ui, ", (comma)", "Toggle edit mask (Instr+Vol)");
                            shortcut_row(ui, "Space (stopped)", "Repeat last cell");
                            shortcut_row(ui, "Alt+N", "Toggle multichannel edit");
                            shortcut_row(ui, "Ctrl+Shift+Up/Dn", "Increase / Decrease octave");
                            ui.add_space(4.0);
                            section_header(ui, "BLOCK OPERATIONS");
                            shortcut_row(ui, "Alt+C", "Copy block to clipboard");
                            shortcut_row(ui, "Alt+P", "Paste clipboard");
                            shortcut_row(ui, "Alt+Z", "Reverse block");
                            shortcut_row(ui, "Alt+F", "Fill instrument");
                            shortcut_row(ui, "Alt+I", "Interpolate volume");
                            shortcut_row(ui, "Alt+K", "Interpolate effect");
                            shortcut_row(ui, "Alt+R", "Randomize notes/volume");
                            ui.add_space(4.0);
                            section_header(ui, "PATTERN");
                            shortcut_row(ui, "- / =", "Prev / next pattern");
                        });

                        columns[1].vertical(|ui| {
                            section_header(ui, "NOTE ENTRY");
                            shortcut_row(ui, "Z S X D...", "Lower octave (C–B)");
                            shortcut_row(ui, "Q 2 W 3...", "Upper octave (C–U)");
                            shortcut_row(ui, "Ctrl+Up/Down", "Decrease / Increase octave");
                            shortcut_row(ui, ". (period)", "Note Off on Note col");
                            shortcut_row(ui, ". (other cols)", "Clear field value");
                            shortcut_row(ui, "0-9 on Instr/Vol", "Decimal entry");
                            shortcut_row(ui, "0-9 A-F on Fx", "Hex entry");
                            ui.add_space(8.0);

                            section_header(ui, "EFFECT COMMANDS");
                            effect_row(ui, "0", "Arpeggio (XY: x,y notes)");
                            effect_row(ui, "1", "Portamento Up (XX: speed)");
                            effect_row(ui, "2", "Portamento Down (XX: speed)");
                            effect_row(ui, "3", "Tone Portamento (XX: speed)");
                            effect_row(ui, "4", "Vibrato (XY: speed, depth)");
                            effect_row(ui, "8", "Set Panning (XX: 00-FF)");
                            effect_row(ui, "9", "Set Offset (XX: high byte)");
                            effect_row(ui, "A", "Volume Slide (XY: up, down)");
                            effect_row(ui, "B", "Position Jump (XX: order)");
                            effect_row(ui, "C", "Set Volume (XX: 00-40)");
                            effect_row(ui, "D", "Pattern Break (XX: row)");
                            effect_row(ui, "E", "Extended Effects (E1, E2...)");
                            effect_row(ui, "F", "Set Speed (XX < 20) / Tempo");
                        });
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("HTRK - A Modern Music Tracker").italics().color(egui::Color32::GRAY));
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
            egui::RichText::new(format!("{: <16}", keys))
                .monospace()
                .size(11.0)
                .color(egui::Color32::from_rgb(255, 200, 100)),
        );
        ui.label(
            egui::RichText::new(action)
                .size(11.0)
                .color(egui::Color32::from_rgb(200, 200, 220)),
        );
    });
}

fn effect_row(ui: &mut egui::Ui, code: &str, description: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{: >2} ", code))
                .monospace()
                .strong()
                .size(11.0)
                .color(egui::Color32::from_rgb(150, 255, 150)),
        );
        ui.label(
            egui::RichText::new(description)
                .size(11.0)
                .color(egui::Color32::from_rgb(200, 200, 220)),
        );
    });
}
