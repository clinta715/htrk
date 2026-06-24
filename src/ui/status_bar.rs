use eframe::egui;
use eguidev::DevUiExt;

use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::ModuleFormat;
use crate::ui::pattern_grid::SubColumn;

use super::sample_palette::draw_waveform_thumbnail;
use super::theme::TrackerTheme;

/// Human-readable name for each sub-column. The status-bar breadcrumb
/// and column-header tooltips both use this.
pub fn sub_column_name(sub: SubColumn) -> &'static str {
    match sub {
        SubColumn::Note => "Note",
        SubColumn::InstrumentTens | SubColumn::InstrumentOnes => "Inst",
        SubColumn::VolumeTens | SubColumn::VolumeOnes => "Vol",
        SubColumn::EffectType | SubColumn::EffectParamHigh | SubColumn::EffectParamLow => "Fx",
    }
}

/// Short hint shown next to the sub-column breadcrumb. Tells the user
/// what characters are accepted and how to navigate.
pub fn sub_column_hint(sub: SubColumn) -> &'static str {
    match sub {
        SubColumn::Note => "Z S X D ... / Q 2 W ... (preview)",
        SubColumn::InstrumentTens | SubColumn::InstrumentOnes => "0-9 (decimal)",
        SubColumn::VolumeTens | SubColumn::VolumeOnes => "0-9 (00-64)",
        SubColumn::EffectType => "0-F / P Z S R X (hex)",
        SubColumn::EffectParamHigh | SubColumn::EffectParamLow => "0-F (hex)",
    }
}

pub fn draw_status_bar(
    ui: &mut egui::Ui,
    module: Option<&crate::sequencer::Module>,
    current_pattern: usize,
    cursor_row: usize,
    total_rows: usize,
    num_channels: usize,
    cpu_pct: u8,
    current_octave: u8,
    cursor_skip: u8,
    selected_instrument: usize,
    selected_sample: usize,
    playback_state: &AtomicPlaybackState,
    edit_mode: bool,
    sub_column: SubColumn,
    hint: &str,
    theme: &TrackerTheme,
) -> Option<i32> {
    let mut sample_delta: Option<i32> = None;
    ui.horizontal(|ui| {
        ui.set_height(20.0);

        let fg = theme.status_fg;
        let font = egui::FontId::monospace(11.0);

        ui.dev_label("status.version", egui::RichText::new(concat!("htrk v", env!("CARGO_PKG_VERSION"))).font(font.clone()).color(fg));
        ui.dev_separator("status.sep1");

        let mode_color = if edit_mode { theme.fg_note } else { theme.fg_effect };
        let mode_text = if edit_mode { "EDT" } else { "VIEW" };
        ui.dev_label("status.mode", egui::RichText::new(mode_text).font(font.clone()).color(mode_color).strong());
        ui.dev_separator("status.sep2");

        let format_str = match module {
            Some(m) => match m.format {
                ModuleFormat::IT => "IT",
                ModuleFormat::XM => "XM",
                ModuleFormat::S3M => "S3M",
                ModuleFormat::MOD => "MOD",
                ModuleFormat::HTK => "HTK",
                ModuleFormat::C669 => "669",
                ModuleFormat::Mmd => "MMD",
                ModuleFormat::Ult => "ULT",
                ModuleFormat::Stm => "STM",
            },
            None => "---",
        };
        ui.dev_label("status.format", egui::RichText::new(format!("Fmt:{}", format_str)).font(font.clone()).color(fg));

        ui.dev_separator("status.sep3");

        ui.label(
            egui::RichText::new(format!("Pat:{:03} Row:{}/{}", current_pattern, cursor_row, total_rows))
                .font(font.clone())
                .color(fg),
        );

        ui.separator();

        ui.label(egui::RichText::new(format!("Oct:{}", current_octave)).font(font.clone()).color(theme.fg_note));
        ui.label(egui::RichText::new(format!(" Skp:{}", cursor_skip)).font(font.clone()).color(theme.fg_effect));
        let mapped_smp = module.and_then(|m| {
            m.instruments.get(selected_instrument).and_then(|inst| {
                (0..120).find_map(|i| {
                    let s = inst.sample_map[i];
                    if s > 0 { Some(s as usize) } else { None }
                })
            })
        });
        let inst_str = match mapped_smp {
            Some(s) => format!(" Ins:{:02}[{:02}]", selected_instrument, s),
            None => format!(" Ins:{:02}", selected_instrument),
        };
        ui.label(egui::RichText::new(inst_str).font(font.clone()).color(theme.fg_instrument));
        ui.label(egui::RichText::new(format!(" Smp:{:02}", selected_sample)).font(font.clone()).color(theme.fg_volume));

        if ui.available_width() > 300.0 {
            if ui.small_button("<").clicked() {
                sample_delta = Some(-1);
            }
            let (thumb_rect, _) = ui.allocate_exact_size(egui::vec2(70.0, 18.0), egui::Sense::hover());
            if let Some(sample) = module.and_then(|m| m.samples.get(selected_sample)) {
                if !sample.data.is_empty() {
                    let positions = playback_state.sample_positions_for(selected_sample);
                    draw_waveform_thumbnail(ui.painter(), thumb_rect, &sample.data, true, &positions, theme);
                }
            }
            if ui.small_button(">").clicked() {
                sample_delta = Some(1);
            }
        }

        ui.separator();

        ui.label(egui::RichText::new(format!("{}ch", num_channels)).font(font.clone()).color(fg));

        ui.separator();

        ui.label(egui::RichText::new(format!("CPU:{}%", cpu_pct)).font(font.clone()).color(fg));

        ui.separator();

        // Sub-column breadcrumb. Shows where the cursor is sitting and
        // what characters that column accepts. Acts as a status-bar
        // substitute for the (easy to miss) cursor position when the
        // user can't tell why their typing isn't being accepted.
        let col_color = match sub_column {
            SubColumn::Note => theme.fg_note,
            SubColumn::InstrumentTens | SubColumn::InstrumentOnes => theme.fg_instrument,
            SubColumn::VolumeTens | SubColumn::VolumeOnes => theme.fg_volume,
            SubColumn::EffectType | SubColumn::EffectParamHigh | SubColumn::EffectParamLow => theme.fg_effect,
        };
        ui.label(
            egui::RichText::new(format!("Col:{}", sub_column_name(sub_column)))
                .font(font.clone())
                .color(col_color)
                .strong(),
        )
        .on_hover_text(sub_column_hint(sub_column));

        ui.separator();

        // Use more space for the hint
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("F1: HELP ").font(font.clone()).color(theme.fg_instrument));
            ui.separator();
            ui.label(egui::RichText::new(hint).font(font).color(theme.fg_note));
        });
    });
    sample_delta
}
