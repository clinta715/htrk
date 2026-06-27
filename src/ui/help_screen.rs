use eframe::egui;

use super::style::FONT_BODY;
use super::theme::TrackerTheme;

pub fn draw_shortcuts_window(ctx: &egui::Context, open: &mut bool, theme: &TrackerTheme) {
    egui::Window::new("Help & Reference")
        .open(open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(true)
        .default_size([820.0, 640.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("F1").monospace().strong().color(theme.fg_instrument));
                        ui.label(egui::RichText::new("close this window").color(theme.fg_dim));
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("htrk v")
                            .color(theme.fg_dim),
                    )
                    .on_hover_ui(|ui| {
                        ui.label(format!("htrk v{}", env!("CARGO_PKG_VERSION")));
                    });
                    ui.label(
                        egui::RichText::new("A modern music tracker with pattern editing, sample editing, CLAP send-bus plugin hosting, automation, and an MCP scripting server.")
                            .italics()
                            .color(theme.fg_dim),
                    );
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(4.0);

                    ui.collapsing("Getting started", |ui| {
                        ui.label("A tracker organises music as a sequence of rows. Each row is one tick of playback. Each row holds up to one cell per channel. A cell has:");
                        ui.add_space(4.0);
                        ui.label("  • A note (e.g. C-5)");
                        ui.label("  • An instrument number (0-99)");
                        ui.label("  • A per-cell volume (00-64)");
                        ui.label("  • An effect command (e.g. 4 Vibrato)");
                        ui.add_space(4.0);
                        ui.label("To start: New (Ctrl+N), pick a sample in the Instrument tab, switch to the Pattern tab (F2), click a cell in the Note column, and play a note on the QWERTY keyboard. The note is set and the cursor advances.");
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Sub-column navigation (the volume column)", |ui| {
                        ui.label("The pattern grid has four sub-columns per channel, in this order:");
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.monospace("Note | Inst | Vol | Fx");
                        });
                        ui.add_space(4.0);
                        ui.label("The cursor starts on Note. Use right-arrow to step through sub-columns (left-arrow to go back). When the cursor reaches the end, it advances to the next row.");
                        ui.add_space(4.0);
                        ui.label("Each sub-column accepts different characters:");
                        ui.add_space(4.0);
                        bullet(ui, "Note:    Z S X D C V G B H N J M (lower octave) and Q 2 W 3 E R 5 T 6 Y 7 U (upper octave). `.` = note-off.", theme);
                        bullet(ui, "Inst:    0-9 (decimal, 00-99). `.` advances to next sample. `,` goes to previous sample.", theme);
                        bullet(ui, "Vol:     0-9 (decimal, 00-64). 00 = silent, 40 = full, 64 = max. No `,` or `.` here.", theme);
                        bullet(ui, "Fx:      0-F (hex). P = send bus param, Z = filter cutoff, S = send level, R = filter resonance, X = filter type. Three hex digits: type, param-hi, param-lo.", theme);
                        ui.add_space(4.0);
                        ui.label("A column can be hidden with Ctrl+1/2/3/4. The status bar shows the current sub-column (Col:Note/Inst/Vol/Fx) and hovering it tells you what it accepts. Hover the column header in the grid for the same info plus how to make the column visible again if it's hidden.");
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Volume column (FAQ)", |ui| {
                        ui.label("Q: I can't edit the volume column.");
                        ui.add_space(4.0);
                        ui.label("A: Three common causes:");
                        bullet(ui, "The volume column is hidden. Press Ctrl+3 to show it. The status bar's Col: label and the column header in the grid will say so.", theme);
                        bullet(ui, "The cursor is on a different sub-column. Right-arrow from Note steps through Inst -> Vol. The status bar shows Col:Vol when you're there.", theme);
                        bullet(ui, "You're in view mode. Press Esc to switch to edit mode (status bar shows EDT).", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Effect commands (0-F and P-Z-S-R-X)", |ui| {
                        ui.label("Effect codes are one hex digit (0-F) followed by a two-hex-digit param. The param is interpreted differently for each effect — see the in-pattern hover popup for the current cell, or the table below.");
                        ui.add_space(6.0);
                        effect_table(ui, theme);
                        ui.add_space(4.0);
                        ui.label("Volume column (XM-style, 2-digit decimal 00-64):");
                        ui.add_space(4.0);
                        bullet(ui, "00-40     set volume (same as `C` effect, 00=silent 40=full)", theme);
                        bullet(ui, "41-FF     volume slide (up by ?0, down by 0?)", theme);
                        bullet(ui, "80-9F     fine volume slide", theme);
                        bullet(ui, "D0-EF     panning slide (D=left, E=right)", theme);
                        bullet(ui, "F0-FF     tone portamento", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("View switching", |ui| {
                        view_row(ui, "F1", "Help & Reference (this window)", theme);
                        view_row(ui, "F2", "Pattern editor", theme);
                        view_row(ui, "F3", "Sample tab", theme);
                        view_row(ui, "F4", "Instrument tab", theme);
                        view_row(ui, "F5", "Play from start of song", theme);
                        view_row(ui, "Shift+F5", "Playback tab + play from start", theme);
                        view_row(ui, "F6", "Play pattern from top", theme);
                        view_row(ui, "F7", "Play from current cursor position", theme);
                        view_row(ui, "F8", "Stop playback", theme);
                        view_row(ui, "F10", "Settings (theme, audio, paths)", theme);
                        view_row(ui, "F11", "Mixer (channel strips, sends, master)", theme);
                        view_row(ui, "F12", "Send FX (CLAP plugins on send buses)", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Menu bar (Alt navigation)", |ui| {
                        shortcut_row(ui, "Alt (tap)", "Activate / deactivate menu bar highlight", theme);
                        shortcut_row(ui, "Alt+F / E / V / A / H", "Open File / Edit / View / Audio / Help menu", theme);
                        shortcut_row(ui, "Left / Right (when active)", "Cycle between menus", theme);
                        shortcut_row(ui, "Down / Enter (when active)", "Open highlighted menu", theme);
                        shortcut_row(ui, "F / E / V / A / H (when active)", "Jump to that menu", theme);
                        shortcut_row(ui, "Escape (when active)", "Deactivate menu bar", theme);
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Note: Alt+F/E/V/A/H always open menus. Pattern-editor shortcuts that previously used those keys have been remapped: Alt+G = Fill Instrument, Alt+D = Mark Block End, Alt+P = Paste.").color(theme.fg_dim).size(FONT_BODY));
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Navigation", |ui| {
                        shortcut_row(ui, "Up / Down", "Move cursor between rows", theme);
                        shortcut_row(ui, "Left / Right", "Move cursor between channels", theme);
                        shortcut_row(ui, "Ctrl+Left / Ctrl+Right", "Step through sub-columns (Note/Inst/Vol/Fx)", theme);
                        shortcut_row(ui, "Shift+Up / Shift+Down", "Extend selection vertically", theme);
                        shortcut_row(ui, "Shift+Left / Shift+Right", "Extend selection by channel", theme);
                        shortcut_row(ui, "Alt+Up / Alt+Down", "Transpose selection ±1 semitone", theme);
                        shortcut_row(ui, "Tab / Shift+Tab", "Next / prev channel", theme);
                        shortcut_row(ui, "- / =", "Prev / next pattern in order list", theme);
                        shortcut_row(ui, "[ / ]", "Decrement / increment octave", theme);
                        shortcut_row(ui, "PgUp / PgDn", "Scroll 16 rows", theme);
                        shortcut_row(ui, "Home", "Top of column; press again to go to leftmost channel", theme);
                        shortcut_row(ui, "End", "Bottom of column", theme);
                        shortcut_row(ui, "Esc", "Toggle edit mode (EDT/VIEW), or close dialog", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Pattern editing", |ui| {
                        shortcut_row(ui, "Ctrl+Z / Ctrl+Y", "Undo / Redo", theme);
                        shortcut_row(ui, "Ctrl+X / C / V", "Block Cut / Copy / Paste", theme);
                        shortcut_row(ui, "Shift+F3 / F4 / F5", "Track Cut / Copy / Paste", theme);
                        shortcut_row(ui, "Alt+F3 / F4 / F5", "Column Cut / Copy / Paste", theme);
                        shortcut_row(ui, "Shift+Delete", "Clear entire track", theme);
                        shortcut_row(ui, "Backspace", "Clear cell", theme);
                        shortcut_row(ui, "Delete", "Clear + advance", theme);
                        shortcut_row(ui, "Insert", "Insert empty row", theme);
                        shortcut_row(ui, "Alt+Delete", "Delete row", theme);
                        shortcut_row(ui, "Ctrl+1 / 2 / 3 / 4", "Toggle Note / Inst / Vol / Fx column", theme);
                        shortcut_row(ui, "Alt+0..9", "Set cursor-skip value (row step per entry)", theme);
                        shortcut_row(ui, "Ctrl+Shift+Up / Down", "Increase / decrease octave", theme);
                        shortcut_row(ui, "Alt+M", "Toggle mute channel", theme);
                        shortcut_row(ui, "Alt+S", "Toggle solo channel", theme);
                        shortcut_row(ui, "Alt+N", "Toggle multichannel edit", theme);
                        shortcut_row(ui, "Alt+L (x2)", "Select column / select all", theme);
                        shortcut_row(ui, "Alt+B / Alt+D", "Mark block begin / end", theme);
                        shortcut_row(ui, "Alt+Z", "Reverse block", theme);
                        shortcut_row(ui, "Alt+G", "Fill instrument", theme);
                        shortcut_row(ui, "Alt+I", "Interpolate volume", theme);
                        shortcut_row(ui, "Alt+K", "Interpolate effect", theme);
                        shortcut_row(ui, "Alt+R", "Randomize notes / volume", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("File & project", |ui| {
                        shortcut_row(ui, "Ctrl+N", "New song", theme);
                        shortcut_row(ui, "Ctrl+O", "Open module...", theme);
                        shortcut_row(ui, "Ctrl+I", "Import sample...", theme);
                        shortcut_row(ui, "Ctrl+Shift+I", "Import instrument...", theme);
                        shortcut_row(ui, "Ctrl+S", "Save", theme);
                        shortcut_row(ui, "Ctrl+Shift+S", "Save As...", theme);
                        shortcut_row(ui, "Ctrl+Q", "Quit (prompts to save if dirty)", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Sample editor", |ui| {
                        shortcut_row(ui, "Ctrl+C / X / V", "Copy / Cut / Paste selection", theme);
                        shortcut_row(ui, "Ctrl+A", "Select all samples in current sample", theme);
                        shortcut_row(ui, "Delete", "Silence selection", theme);
                        shortcut_row(ui, "Right-click waveform", "Context menu (Cut/Crop/Fade/Normalize...)", theme);
                        shortcut_row(ui, "Mouse wheel", "Zoom waveform", theme);
                        shortcut_row(ui, "Fit / Sel (button bar)", "Zoom fit / zoom to selection", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Audio preview & QWERTY jam", |ui| {
                        ui.label("The QWERTY rows act as a live keyboard regardless of which sub-column the cursor is on, as long as no text widget is focused and no dialog is open.");
                        ui.add_space(4.0);
                        shortcut_row(ui, "Z S X D C V G B H N J M", "Lower octave (C-B)", theme);
                        shortcut_row(ui, "Q 2 W 3 E R 5 T 6 Y 7 U", "Upper octave (C-U)", theme);
                        shortcut_row(ui, ". (period on Note col)", "Insert note-off and advance", theme);
                        shortcut_row(ui, "▶ Preview (file browser)", "Preview selected WAV at middle C", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Send FX (CLAP plugins)", |ui| {
                        ui.label("htrk has 4 send buses (A/B/C/D) that route any channel's signal through a chain of CLAP plugins and return the wet signal to the master bus.");
                        ui.add_space(4.0);
                        bullet(ui, "F12 opens the Send FX view. There is one panel per bus; click 'Load Plugin' to pick from the discovered CLAP library.", theme);
                        bullet(ui, "Edit opens the plugin's GUI (floating by default; 'Edit (in htrk)' embeds it inside the main window).", theme);
                        bullet(ui, "Each loaded plugin shows a Parameters section with one slider per exposed param. Click a slider to tweak; the audio thread picks up the change on the next process() call.", theme);
                        bullet(ui, "A channel's send level to a bus is set by the channel header slider in the pattern editor (1 per bus, 0-64).", theme);
                        bullet(ui, "Set send levels per cell with the S effect (X = bus, Y = level) or via the per-cell volume column when paired with the P effect (Set Send Bus Param).", theme);
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Mixer (F11)", |ui| {
                        ui.label("F11 opens a conventional mixer with one strip per channel plus a master strip and one strip per send bus. Each channel strip has:");
                        ui.add_space(4.0);
                        bullet(ui, "Mute / Solo toggles.", theme);
                        bullet(ui, "Channel volume slider (0-64).", theme);
                        bullet(ui, "Channel pan slider (0-255, 128 = center).", theme);
                        bullet(ui, "Send-level sliders for buses A-D.", theme);
                        bullet(ui, "An automation-target picker (Volume / Panning / Cutoff / Resonance / Send A-D).", theme);
                        ui.add_space(4.0);
                        ui.label("Send-bus strips show the input meter, the loaded plugin chain, and the return level. The master strip shows the global volume and output meter.");
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Automation", |ui| {
                        ui.label("Right-click the pattern grid in the FX column to add an automation point. Or open the automation editor (F7) to draw full automation lanes for any per-channel target. Supported targets:");
                        ui.add_space(4.0);
                        bullet(ui, "Channel Volume (per channel)", theme);
                        bullet(ui, "Channel Panning (per channel)", theme);
                        bullet(ui, "Filter Cutoff (per channel)", theme);
                        bullet(ui, "Filter Resonance (per channel)", theme);
                        bullet(ui, "Send Level A-D (per channel)", theme);
                        bullet(ui, "Global Volume / Tempo / Speed", theme);
                        bullet(ui, "Send Return Level A-D (global)", theme);
                        bullet(ui, "Send Bus Param 0-3 for each bus (global)", theme);
                        bullet(ui, "CLAP Plugin Param (per bus, per param id)", theme);
                        ui.add_space(4.0);
                        ui.label("Click in the FX column to create a point; Shift+drag for freehand; Ctrl+click to bypass the overlay and enter an effect value.");
                    });

                    ui.add_space(4.0);
                    ui.collapsing("MCP scripting server", |ui| {
                        ui.label("htrk can run a Model Context Protocol server alongside the GUI. Agents (Claude Code, scripts, etc.) connect over TCP localhost and use the JSON-RPC tools listed below to read module state, edit patterns, scan plugin libraries, and drive transport.");
                        ui.add_space(4.0);
                        shortcut_row(ui, "Edit config.toml", "Set mcp_enabled = true, mcp_port = 18763", theme);
                        shortcut_row(ui, "htrk --mcp", "Force-enable the MCP server this session", theme);
                        shortcut_row(ui, "htrk --mcp-port 20000", "Run on a custom port", theme);
                        shortcut_row(ui, "htrk --mcp-help", "Print the full JSON-RPC reference to stdout", theme);
                        ui.add_space(4.0);
                        ui.label("Read-only tools: module.get, module.summary, pattern.get, playback.status, channel.state, plugin.list, sample_library.list_dir, sample_library.search.");
                        ui.add_space(4.0);
                        ui.label("Mutation tools (require main-thread dispatch): module.new, module.open, module.save, pattern.set_cell, pattern.bulk_set_cells, pattern.transpose, channel.set_volume, channel.set_panning, channel.set_mute, channel.set_solo, sample_library.import, send_fx.set_plugin, automation.add_point, playback.play, playback.stop.");
                    });

                    ui.add_space(4.0);
                    ui.collapsing("Command-line flags", |ui| {
                        cli_row(ui, "-h, --help", "Print full help and exit", theme);
                        cli_row(ui, "-V, --version", "Print version and exit", theme);
                        cli_row(ui, "--config-path", "Print the user's config.toml path", theme);
                        cli_row(ui, "--list-effects", "Print the 0-F / P-Z-S-R-X effect reference", theme);
                        cli_row(ui, "--mcp-help", "Print the MCP / JSON-RPC reference", theme);
                        cli_row(ui, "--reset-config", "Delete config.toml (next launch uses defaults)", theme);
                        cli_row(ui, "--debug", "Enable debug log output", theme);
                        cli_row(ui, "--theme <NAME>", "Override theme preset", theme);
                        cli_row(ui, "--mcp / --no-mcp", "Force-enable / force-disable the MCP server", theme);
                        cli_row(ui, "--mcp-port <N>", "MCP TCP port (default 18763)", theme);
                        cli_row(ui, "--mcp-http / --mcp-http-port <N>", "MCP HTTP/SSE transport (default 18764)", theme);
                        cli_row(ui, "--log-file <PATH>", "Also write tracing output to a file", theme);
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(concat!("Holofonic Tracker v", env!("CARGO_PKG_VERSION"), " — Modern Music Tracker")).italics().color(theme.fg_dim));
                });
        });
}

fn bullet(ui: &mut egui::Ui, text: &str, theme: &TrackerTheme) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("  •").color(theme.fg_dim));
        ui.label(egui::RichText::new(text).color(theme.fg_text));
    });
}

fn shortcut_row(ui: &mut egui::Ui, keys: &str, action: &str, theme: &TrackerTheme) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{: <22}", keys))
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

fn view_row(ui: &mut egui::Ui, key: &str, action: &str, theme: &TrackerTheme) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{: <14}", key))
                .monospace()
                .size(FONT_BODY)
                .color(theme.fg_instrument),
        );
        ui.label(
            egui::RichText::new(action)
                .size(FONT_BODY)
                .color(theme.fg_text),
        );
    });
}

fn cli_row(ui: &mut egui::Ui, flag: &str, action: &str, theme: &TrackerTheme) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{: <28}", flag))
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

/// Compact per-effect reference table. Same data as the FX column
/// hover popups, but laid out as a table for scanning. Abbreviates
/// params where space is tight; the popup is the canonical source.
fn effect_table(ui: &mut egui::Ui, theme: &TrackerTheme) {
    let entries: &[(&str, &str, &str)] = &[
        ("0",  "Arpeggio",              "XY: +semitones"),
        ("1",  "Portamento Up",         "XX: speed"),
        ("2",  "Portamento Down",       "XX: speed"),
        ("3",  "Tone Portamento",       "XX: speed"),
        ("4",  "Vibrato",               "X: speed, Y: depth"),
        ("5",  "Tone Porta + Vol Slide","X: porta, Y: slide"),
        ("6",  "Vibrato + Vol Slide",   "X: vib, Y: slide"),
        ("7",  "Tremolo",               "X: speed, Y: depth"),
        ("8",  "Set Panning",           "XX: 00-FF (80=center)"),
        ("9",  "Set Sample Offset",     "XX: high byte"),
        ("A",  "Volume Slide",          "X: up, Y: down"),
        ("B",  "Position Jump",         "XX: order"),
        ("C",  "Set Volume",            "XX: 00-40"),
        ("D",  "Pattern Break",         "XX: row"),
        ("E",  "Extended (E0-EF)",      "see sub-effect table"),
        ("F",  "Set Speed / Tempo",     "XX<20: ticks, XX>=20: BPM"),
        ("G",  "Global Volume",         "XX: 00-80"),
        ("H",  "Global Vol Slide",      "X: up, Y: down"),
        ("I",  "Tremor",                "X: on, Y: off"),
        ("L",  "Envelope Position",     "XX: tick"),
        ("P",  "Panning Slide",         "XX: signed speed"),
        ("R",  "Filter Resonance",      "XX: 00-FF"),
        ("S",  "Set Send Level",        "X: bus, Y: level"),
        ("X",  "Filter Type",           "00=LP 01=HP 02=BP 03=Notch"),
        ("Y",  "Panbrello",             "X: speed, Y: depth"),
        ("Z",  "Filter Cutoff",         "XX: 00-FF"),
    ];

    egui::Grid::new("effect_table")
        .num_columns(3)
        .spacing([16.0, 2.0])
        .striped(true)
        .min_col_width(40.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Code").strong().color(theme.fg_volume));
            ui.label(egui::RichText::new("Name").strong().color(theme.fg_volume));
            ui.label(egui::RichText::new("Param").strong().color(theme.fg_volume));
            ui.end_row();
            for (code, name, param) in entries {
                ui.label(egui::RichText::new(*code).monospace().strong().color(theme.fg_effect));
                ui.label(egui::RichText::new(*name).color(theme.fg_text));
                ui.label(egui::RichText::new(*param).color(theme.fg_dim));
                ui.end_row();
            }
        });
}
