use eframe::egui;

use super::style::FONT_BODY;
use super::theme::TrackerTheme;

pub fn draw_shortcuts_window(ctx: &egui::Context, open: &mut bool, theme: &TrackerTheme) {
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
                    .size(FONT_BODY)
                    .color(theme.fg_dim),
                );
                ui.separator();
                    ui.add_space(4.0);

                    ui.columns(2, |columns| {
                        columns[0].vertical(|ui| {
                            super::style::section_header(ui, "NAVIGATION", theme);
                            shortcut_row(ui, "Up/Down", "Move cursor between rows", theme);
                            shortcut_row(ui, "Left/Right", "Move cursor between channels", theme);
                            shortcut_row(ui, "Ctrl+Left/Right", "Step through sub-columns", theme);
                            shortcut_row(ui, "Shift+Up/Down", "Extend selection vertically", theme);
                            shortcut_row(ui, "Shift+Left/Right", "Extend selection by channel", theme);
                            shortcut_row(ui, "Alt+Up/Down", "Transpose ±1 semitone", theme);
                            shortcut_row(ui, "Tab / Shift+Tab", "Next / prev channel", theme);
                            shortcut_row(ui, "- / =  (Numpad -/+)", "Prev / next pattern", theme);
                            shortcut_row(ui, "[ / ]", "Decrement / increment octave", theme);
                            shortcut_row(ui, "PgUp / PgDn", "Scroll 16 rows", theme);
                            shortcut_row(ui, "Home", "Top of column; again = leftmost", theme);
                            shortcut_row(ui, "End", "Bottom of column", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "PATTERN EDITING", theme);
                            shortcut_row(ui, "Ctrl+X / C / V", "Block Cut / Copy / Paste", theme);
                            shortcut_row(ui, "Shift+F3 / F4 / F5", "Track Cut / Copy / Paste", theme);
                            shortcut_row(ui, "Alt+F3 / F4 / F5", "Column Cut / Copy / Paste", theme);
                            shortcut_row(ui, "Shift+Delete", "Clear entire track", theme);
                            shortcut_row(ui, "Backspace", "Clear cell", theme);
                            shortcut_row(ui, "Delete", "Clear + advance", theme);
                            shortcut_row(ui, "Insert", "Insert empty row", theme);
                            shortcut_row(ui, "Alt+Delete", "Delete row", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "FILE", theme);
                            shortcut_row(ui, "Ctrl+N", "New song", theme);
                            shortcut_row(ui, "Ctrl+O", "Open module...", theme);
                            shortcut_row(ui, "Ctrl+I", "Import sample...", theme);
                            shortcut_row(ui, "Ctrl+Shift+I", "Import instrument...", theme);
                            shortcut_row(ui, "Ctrl+S", "Save", theme);
                            shortcut_row(ui, "Ctrl+Shift+S", "Save As...", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "EDITING", theme);
                            shortcut_row(ui, "Ctrl+Z / Y", "Undo / Redo", theme);
                            shortcut_row(ui, "Ctrl+A", "Select all", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "TRANSPORT", theme);
                            shortcut_row(ui, "F6", "Play pattern", theme);
                            shortcut_row(ui, "F7", "Play through order", theme);
                            shortcut_row(ui, "F8", "Stop all", theme);
                            shortcut_row(ui, "F9", "Play from current pos", theme);
                            shortcut_row(ui, "Space", "Repeat last entry / Stop", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "VIEW SWITCHING (IT-style)", theme);
                            shortcut_row(ui, "F1", "Help / Shortcuts", theme);
                            shortcut_row(ui, "F2", "Pattern editor", theme);
                            shortcut_row(ui, "F3", "Sample tab", theme);
                            shortcut_row(ui, "F4", "Instrument tab", theme);
                            shortcut_row(ui, "F5", "Play from start", theme);
                            shortcut_row(ui, "Shift+F5", "Playback tab + Play from start", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "CHANNEL", theme);
                            shortcut_row(ui, "Esc", "Toggle edit mode (EDT / VIEW)", theme);
                            shortcut_row(ui, "Alt+M", "Toggle mute channel", theme);
                            shortcut_row(ui, "Alt+S", "Toggle solo channel", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "VIEW", theme);
                            shortcut_row(ui, "Ctrl+Shift+Space", "Cycle spacing mode", theme);
                            shortcut_row(ui, "Ctrl+Shift+L", "Toggle sample length background", theme);
                            shortcut_row(ui, "Ctrl+1", "Toggle Note column", theme);
                            shortcut_row(ui, "Ctrl+2", "Toggle Instrument column", theme);
                            shortcut_row(ui, "Ctrl+3", "Toggle Volume column", theme);
                            shortcut_row(ui, "Ctrl+4", "Toggle Effect column", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "IT-STYLE FEATURES", theme);
                            shortcut_row(ui, "Alt+0..9", "Set cursor skip value", theme);
                            shortcut_row(ui, ", / .", "Prev / next sample (inst col: inst)", theme);
                            shortcut_row(ui, ". (note col)", "Note Off (edit mode)", theme);
                            shortcut_row(ui, "Ctrl+Shift+Left/Right", "Prev / next sample", theme);
                            shortcut_row(ui, "Space (stopped)", "Repeat last cell", theme);
                            shortcut_row(ui, "Alt+N", "Toggle multichannel edit", theme);
                            shortcut_row(ui, "Ctrl+Shift+Up/Dn", "Increase / Decrease octave", theme);
                            ui.add_space(4.0);
                            super::style::section_header(ui, "BLOCK OPERATIONS", theme);
                            shortcut_row(ui, "Alt+B / Alt+E", "Mark block begin / end", theme);
                            shortcut_row(ui, "Alt+L (x2)", "Select column / whole pattern", theme);
                            shortcut_row(ui, "Alt+C / X / V", "Copy / Cut / Paste block", theme);
                            shortcut_row(ui, "Alt+Z", "Reverse block", theme);
                            shortcut_row(ui, "Alt+F", "Fill instrument", theme);
                            shortcut_row(ui, "Alt+I", "Interpolate volume", theme);
                            shortcut_row(ui, "Alt+K", "Interpolate effect", theme);
                            shortcut_row(ui, "Alt+R", "Randomize notes/volume", theme);
                            ui.add_space(4.0);
                            super::style::section_header(ui, "PATTERN", theme);
                            shortcut_row(ui, "- / =  (Numpad -/+)", "Prev / next pattern", theme);
                        });

                        columns[1].vertical(|ui| {
                            super::style::section_header(ui, "NOTE ENTRY", theme);
                            shortcut_row(ui, "Z S X D...", "Lower octave (C–B)", theme);
                            shortcut_row(ui, "Q 2 W 3...", "Upper octave (C–U)", theme);
                            shortcut_row(ui, "Ctrl+Up/Down", "Decrease / Increase octave", theme);
                            shortcut_row(ui, ". (period)", "Note Off on Note col", theme);
                            shortcut_row(ui, "0-9 on Instr/Vol", "Decimal entry", theme);
                            shortcut_row(ui, "0-9 A-F on Fx", "Hex entry", theme);
                            ui.add_space(4.0);
                            super::style::section_header(ui, "AUDIO PREVIEW", theme);
                            shortcut_row(ui, "Z S X... / Q 2 W...", "Jam: play sample / preview browser file", theme);
                            shortcut_row(ui, "Ctrl+Shift+Left/Right", "Prev / next sample", theme);
                            shortcut_row(ui, "▶ Preview (file browser)", "Preview selected WAV at middle C", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "SAMPLE EDITOR", theme);
                            shortcut_row(ui, "Ctrl+C", "Copy selection", theme);
                            shortcut_row(ui, "Ctrl+X", "Cut selection", theme);
                            shortcut_row(ui, "Ctrl+V", "Paste at cursor", theme);
                            shortcut_row(ui, "Ctrl+A", "Select all", theme);
                            shortcut_row(ui, "Delete", "Silence selection", theme);
                            shortcut_row(ui, "Right-click", "Context menu (Cut/Crop/Fade...)", theme);
                            shortcut_row(ui, "Mouse wheel", "Zoom waveform", theme);
                            shortcut_row(ui, "Fit / Sel", "Zoom fit / zoom to selection", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "SETTINGS (F10)", theme);
                            shortcut_row(ui, "Paths", "Default sample/directory paths", theme);
                            shortcut_row(ui, "Editor", "Font, zoom, highlights, etc.", theme);
                            shortcut_row(ui, "Audio", "Device, interpolation, limiter, sample rate", theme);
                            shortcut_row(ui, "Backup", "Auto-backup interval & directory", theme);
                            shortcut_row(ui, "Theme", "Visual theme selection", theme);
                            shortcut_row(ui, "Advanced", "Debug logging toggle", theme);
                            ui.add_space(8.0);

                            super::style::section_header(ui, "EFFECT COMMANDS", theme);
                            effect_row(ui, "0", "Arpeggio (XY: x,y notes)", theme);
                            effect_row(ui, "1", "Portamento Up (XX: speed)", theme);
                            effect_row(ui, "2", "Portamento Down (XX: speed)", theme);
                            effect_row(ui, "3", "Tone Portamento (XX: speed)", theme);
                            effect_row(ui, "4", "Vibrato (XY: speed, depth)", theme);
                            effect_row(ui, "5", "Tone Porta + Vol Slide (XY)", theme);
                            effect_row(ui, "6", "Vibrato + Vol Slide (XY)", theme);
                            effect_row(ui, "7", "Tremolo (XY: speed, depth)", theme);
                            effect_row(ui, "8", "Set Panning (XX: 00-FF)", theme);
                            effect_row(ui, "9", "Set Offset (XX: high byte)", theme);
                            effect_row(ui, "A", "Volume Slide (XY: up, down)", theme);
                            effect_row(ui, "B", "Position Jump (XX: order)", theme);
                            effect_row(ui, "C", "Set Volume (XX: 00-40)", theme);
                            effect_row(ui, "D", "Pattern Break (XX: row)", theme);
                            effect_row(ui, "E", "Extended Effects (E1, E2...)", theme);
                            effect_row(ui, "F", "Set Speed (XX < 20) / Tempo", theme);
                            effect_row(ui, "G", "Global Volume (XX)", theme);
                            effect_row(ui, "H", "Global Vol Slide (XY)", theme);
                            effect_row(ui, "I", "Tremor (XY: on, off)", theme);
                            effect_row(ui, "K", "Key Off (XM)", theme);
                            effect_row(ui, "L", "Envelope Position (XX)", theme);
                            effect_row(ui, "P", "Panning Slide (XX)", theme);
                            effect_row(ui, "R", "Filter Resonance (XX)", theme);
                            effect_row(ui, "S", "Set Send Level (XY: bus, level)", theme);
                            effect_row(ui, "X", "Filter Type (XX)", theme);
                            effect_row(ui, "Y", "Panbrello (XY)", theme);
                            effect_row(ui, "Z", "Filter Cutoff (XX)", theme);
                        });
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(concat!("Holofonic Tracker v", env!("CARGO_PKG_VERSION"), " — A Modern Music Tracker")).italics().color(theme.fg_dim));
                });
        });
}



fn shortcut_row(ui: &mut egui::Ui, keys: &str, action: &str, theme: &TrackerTheme) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{: <16}", keys))
                .monospace()
                .size(FONT_BODY)
                .color(theme.fg_effect),
        );
        ui.label(
            egui::RichText::new(action)
                .size(FONT_BODY)
                .color(theme.fg_text),
        );
    });
}

fn effect_row(ui: &mut egui::Ui, code: &str, description: &str, theme: &TrackerTheme) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{: >2} ", code))
                .monospace()
                .strong()
                .size(FONT_BODY)
                .color(theme.fg_volume),
        );
        ui.label(
            egui::RichText::new(description)
                .size(FONT_BODY)
                .color(theme.fg_text),
        );
    });
}
