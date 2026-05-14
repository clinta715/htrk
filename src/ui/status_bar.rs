use eframe::egui;

use crate::sequencer::{Module, ModuleFormat};

use super::theme::TrackerTheme;

pub fn draw_status_bar(
    ui: &mut egui::Ui,
    module: Option<&Module>,
    current_pattern: usize,
    cursor_row: usize,
    total_rows: usize,
    num_channels: usize,
    cpu_pct: u8,
    current_octave: u8,
    cursor_skip: u8,
    selected_instrument: usize,
    selected_sample: usize,
    hint: &str,
    theme: &TrackerTheme,
) {
    ui.horizontal(|ui| {
        ui.set_height(20.0);

        let fg = theme.status_fg;
        let font = egui::FontId::monospace(11.0);

        ui.label(egui::RichText::new("htrk v0.4.1").font(font.clone()).color(fg));
        ui.separator();

        let format_str = match module {
            Some(m) => match m.format {
                ModuleFormat::IT => "IT",
                ModuleFormat::XM => "XM",
                ModuleFormat::S3M => "S3M",
                ModuleFormat::MOD => "MOD",
                ModuleFormat::HTK => "HTK",
            },
            None => "---",
        };
        ui.label(egui::RichText::new(format!("Fmt:{}", format_str)).font(font.clone()).color(fg));

        ui.separator();

        ui.label(
            egui::RichText::new(format!("Pat:{:03} Row:{}/{}", current_pattern, cursor_row, total_rows))
                .font(font.clone())
                .color(fg),
        );

        ui.separator();

        ui.label(egui::RichText::new(format!("Oct:{}", current_octave)).font(font.clone()).color(theme.fg_note));
        ui.label(egui::RichText::new(format!(" Skp:{}", cursor_skip)).font(font.clone()).color(theme.fg_effect));
        ui.label(egui::RichText::new(format!(" Ins:{:02}", selected_instrument)).font(font.clone()).color(theme.fg_instrument));
        ui.label(egui::RichText::new(format!(" Smp:{:02}", selected_sample)).font(font.clone()).color(theme.fg_volume));

        ui.separator();

        ui.label(egui::RichText::new(format!("{}ch", num_channels)).font(font.clone()).color(fg));

        ui.separator();

        ui.label(egui::RichText::new(format!("CPU:{}%", cpu_pct)).font(font.clone()).color(fg));

        ui.separator();

        // Use more space for the hint
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("F3: HELP ").font(font.clone()).color(theme.fg_instrument));
            ui.separator();
            ui.label(egui::RichText::new(hint).font(font).color(theme.fg_note));
        });
    });
}
