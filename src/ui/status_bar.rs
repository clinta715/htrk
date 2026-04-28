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
    theme: &TrackerTheme,
) {
    ui.horizontal(|ui| {
        ui.set_height(20.0);

        let fg = theme.status_fg;
        let font = egui::FontId::monospace(11.0);

        ui.label(egui::RichText::new("htrk v0.1").font(font.clone()).color(fg));
        ui.separator();

        let format_str = match module {
            Some(m) => match m.format {
                ModuleFormat::IT => "IT",
                ModuleFormat::XM => "XM",
                ModuleFormat::S3M => "S3M",
                ModuleFormat::MOD => "MOD",
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

        ui.label(egui::RichText::new(format!("{}ch", num_channels)).font(font.clone()).color(fg));

        ui.separator();

        ui.label(egui::RichText::new(format!("CPU:{}%", cpu_pct)).font(font).color(fg));
    });
}
