use eframe::egui::{self, Pos2, Rect, Stroke};

use crate::app_config::SpacingMode;
use crate::sequencer::automation::{AutomationTarget, AutomationTrack, InterpolationMode};
use crate::sequencer::effect::{Effect, FormatEffect, XmEffect, ModEffect, S3mEffect, ItEffect, C669Effect, MmdEffect, UltEffect, StmEffect};
use crate::sequencer::note::{Note, TONE_NAMES};
use crate::sequencer::pattern::Cell;

use super::theme::TrackerTheme;

pub struct AutomationOverlayInfo {
    pub target: AutomationTarget,
    pub track: Option<std::sync::Arc<AutomationTrack>>,
    pub current_order: u16,
    pub speed: u8,
}

#[derive(Debug, Clone)]
pub enum AutomationInteraction {
    PointCreated { channel: usize, order: u16, row: u16, value: f32 },
    PointMoved { channel: usize, order: u16, row: u16, value: f32 },
    FreehandDraw { channel: usize, points: Vec<(u16, u16, f32)> },
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnVisibility {
    pub note: bool,
    pub instrument: bool,
    pub volume: bool,
    pub effect: bool,
}

impl ColumnVisibility {
    pub fn all() -> Self {
        ColumnVisibility { note: true, instrument: true, volume: true, effect: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridMetrics {
    pub font_size: f32,
    pub row_height: f32,
    pub char_width: f32,
    pub row_num_width: f32,
    pub channel_width: f32,
    pub spacing_mode: SpacingMode,
    pub note_width: f32,
    pub inst_width: f32,
    pub vol_width: f32,
    pub effect_width: f32,
    pub note_to_inst_gap: f32,
    pub inst_to_vol_gap: f32,
    pub vol_to_effect_gap: f32,
    pub inst_x: f32,
    pub vol_x: f32,
    pub effect_type_x: f32,
}

impl GridMetrics {
    pub fn new(font_size: f32, spacing_mode: SpacingMode, col_vis: ColumnVisibility) -> Self {
        let char_width = font_size * 0.6;
        let (row_spacing, col_gap) = match spacing_mode {
            SpacingMode::Compact => (0.0, 0.0),
            SpacingMode::Normal => (0.3, 0.3),
            SpacingMode::Wide => (0.5, 0.6),
            SpacingMode::ExtraWide => (0.8, 1.0),
        };

        let note_width = if col_vis.note { char_width * 3.5 } else { 0.0 };
        let inst_width = if col_vis.instrument { char_width * 2.5 } else { 0.0 };
        let vol_width = if col_vis.volume { char_width * 2.5 } else { 0.0 };
        let effect_width = if col_vis.effect { char_width * 3.0 } else { 0.0 };

        let note_to_inst_gap = if col_vis.note && col_vis.instrument { char_width * col_gap } else { 0.0 };
        let inst_to_vol_gap = if col_vis.instrument && col_vis.volume { char_width * col_gap } else { 0.0 };
        let vol_to_effect_gap = if col_vis.volume && col_vis.effect { char_width * col_gap } else { 0.0 };

        let channel_width = note_width + note_to_inst_gap + inst_width + inst_to_vol_gap + vol_width + vol_to_effect_gap + effect_width;

        let inst_x = note_width + note_to_inst_gap;
        let vol_x = inst_x + inst_width + inst_to_vol_gap;
        let effect_type_x = vol_x + vol_width + vol_to_effect_gap;

        Self {
            font_size,
            row_height: font_size * 1.3 + font_size * row_spacing,
            char_width,
            row_num_width: char_width * 4.0,
            channel_width,
            spacing_mode,
            note_width,
            inst_width,
            vol_width,
            effect_width,
            note_to_inst_gap,
            inst_to_vol_gap,
            vol_to_effect_gap,
            inst_x,
            vol_x,
            effect_type_x,
        }
    }

    pub fn calculate_visible_channels(ui: &egui::Ui, metrics: GridMetrics) -> usize {
        if metrics.channel_width < 1.0 {
            return 0;
        }
        let available_size = ui.available_size();
        ((available_size.x - metrics.row_num_width) / metrics.channel_width).floor() as usize
    }

    pub fn effect_param1_x(&self) -> f32 {
        self.effect_type_x + self.char_width
    }

    pub fn effect_param2_x(&self) -> f32 {
        self.effect_param1_x() + self.char_width
    }
}

pub const VISIBLE_ROWS: usize = 32; // Kept as default/fallback

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubColumn {
    Note,
    InstrumentTens,
    InstrumentOnes,
    VolumeTens,
    VolumeOnes,
    EffectType,
    EffectParamHigh,
    EffectParamLow,
}

impl SubColumn {
    #[allow(dead_code)]
    pub fn all() -> &'static [SubColumn] {
        &[
            SubColumn::Note,
            SubColumn::InstrumentTens,
            SubColumn::InstrumentOnes,
            SubColumn::VolumeTens,
            SubColumn::VolumeOnes,
            SubColumn::EffectType,
            SubColumn::EffectParamHigh,
            SubColumn::EffectParamLow,
        ]
    }

    pub fn next(self) -> Option<SubColumn> {
        match self {
            SubColumn::Note => Some(SubColumn::InstrumentTens),
            SubColumn::InstrumentTens => Some(SubColumn::InstrumentOnes),
            SubColumn::InstrumentOnes => Some(SubColumn::VolumeTens),
            SubColumn::VolumeTens => Some(SubColumn::VolumeOnes),
            SubColumn::VolumeOnes => Some(SubColumn::EffectType),
            SubColumn::EffectType => Some(SubColumn::EffectParamHigh),
            SubColumn::EffectParamHigh => Some(SubColumn::EffectParamLow),
            SubColumn::EffectParamLow => None,
        }
    }

    pub fn prev(self) -> Option<SubColumn> {
        match self {
            SubColumn::Note => None,
            SubColumn::InstrumentTens => Some(SubColumn::Note),
            SubColumn::InstrumentOnes => Some(SubColumn::InstrumentTens),
            SubColumn::VolumeTens => Some(SubColumn::InstrumentOnes),
            SubColumn::VolumeOnes => Some(SubColumn::VolumeTens),
            SubColumn::EffectType => Some(SubColumn::VolumeOnes),
            SubColumn::EffectParamHigh => Some(SubColumn::EffectType),
            SubColumn::EffectParamLow => Some(SubColumn::EffectParamHigh),
        }
    }

    pub fn accepts_hex(self) -> bool {
        matches!(self, SubColumn::EffectType | SubColumn::EffectParamHigh | SubColumn::EffectParamLow)
    }

    pub fn accepts_decimal(self) -> bool {
        matches!(self, SubColumn::InstrumentTens | SubColumn::InstrumentOnes | SubColumn::VolumeTens | SubColumn::VolumeOnes)
    }

    pub fn accepts_note(self) -> bool {
        matches!(self, SubColumn::Note)
    }

    pub fn is_visible(self, col_vis: ColumnVisibility) -> bool {
        match self {
            SubColumn::Note => col_vis.note,
            SubColumn::InstrumentTens | SubColumn::InstrumentOnes => col_vis.instrument,
            SubColumn::VolumeTens | SubColumn::VolumeOnes => col_vis.volume,
            SubColumn::EffectType | SubColumn::EffectParamHigh | SubColumn::EffectParamLow => col_vis.effect,
        }
    }

    pub fn next_visible(self, col_vis: ColumnVisibility) -> Option<SubColumn> {
        let mut current = self.next();
        while let Some(sc) = current {
            if sc.is_visible(col_vis) {
                return Some(sc);
            }
            current = sc.next();
        }
        None
    }

    pub fn prev_visible(self, col_vis: ColumnVisibility) -> Option<SubColumn> {
        let mut current = self.prev();
        while let Some(sc) = current {
            if sc.is_visible(col_vis) {
                return Some(sc);
            }
            current = sc.prev();
        }
        None
    }
}

impl Default for SubColumn {
    fn default() -> Self {
        SubColumn::Note
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorPosition {
    pub row: usize,
    pub channel: usize,
    pub sub_column: SubColumn,
}

impl Default for CursorPosition {
    fn default() -> Self {
        CursorPosition {
            row: 0,
            channel: 0,
            sub_column: SubColumn::Note,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub start: CursorPosition,
    pub end: CursorPosition,
}

impl Selection {
    pub fn normalized(&self) -> (CursorPosition, CursorPosition) {
        let (row_min, row_max) = if self.start.row <= self.end.row {
            (self.start.row, self.end.row)
        } else {
            (self.end.row, self.start.row)
        };
        let (ch_min, ch_max) = if self.start.channel <= self.end.channel {
            (self.start.channel, self.end.channel)
        } else {
            (self.end.channel, self.start.channel)
        };
        (
            CursorPosition {
                row: row_min,
                channel: ch_min,
                sub_column: SubColumn::Note,
            },
            CursorPosition {
                row: row_max,
                channel: ch_max,
                sub_column: SubColumn::EffectParamLow,
            },
        )
    }

    pub fn contains(&self, row: usize, channel: usize) -> bool {
        let (min, max) = self.normalized();
        row >= min.row && row <= max.row && channel >= min.channel && channel <= max.channel
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextMenuAction {
    FillInstrument,
    InterpolateVolume,
    InterpolateEffect,
    Reverse,
    Randomize,
    /// Insert an effect command at the cursor position. `hex` is the
    /// effect type digit (0-15). Used by the "Set Effect" submenu.
    SetEffect { hex: u8 },
    /// Set the effect to one of the named CLAP-style commands: P, Z, S, R, X.
    /// Maps to the Effect::SetSendBusParam / SetFilterCutoff / etc.
    SetParamEffect { command: ParamEffectCommand },
    /// Clear the effect command at the cursor position (or all selected
    /// cells if a selection is active).
    ClearEffect,
}

/// Identifies one of the named parameter-style effect commands that the
/// "Set Effect" submenu exposes in addition to the standard hex effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamEffectCommand {
    /// 'P' — SetSendBusParam (param 0 by default; user can edit param high/low)
    SetSendBusParam,
    /// 'Z' — SetFilterCutoff
    SetFilterCutoff,
    /// 'S' — SetSendLevel
    SetSendLevel,
    /// 'R' — SetFilterResonance
    SetFilterResonance,
    /// 'X' — SetFilterType
    SetFilterType,
}

pub struct PatternGridResponse {
    pub visible_rows: usize,
    pub visible_channels: usize,
    pub clicked_position: Option<CursorPosition>,
    pub drag_position: Option<CursorPosition>,
    pub context_menu_action: Option<ContextMenuAction>,
    pub effect_tooltip: Option<String>,
    pub toggle_sample_length_bg: bool,
    pub automation_interaction: Option<AutomationInteraction>,
}

pub fn draw_pattern_grid(
    ui: &mut egui::Ui,
    pattern: &crate::sequencer::pattern::Pattern,
    cursor: &CursorPosition,
    selection: Option<&Selection>,
    playback_row: Option<usize>,
    playback_tick: Option<u8>,
    playback_speed: u8,
    scroll_row: usize,
    scroll_channel: usize,
    num_channels: usize,
    metrics: GridMetrics,
    theme: &TrackerTheme,
    highlight_minor: u8,
    highlight_major: u8,
    sample_length_bg: bool,
    col_vis: ColumnVisibility,
    module: Option<&crate::sequencer::module::Module>,
    automation_overlays: &[Option<AutomationOverlayInfo>],
) -> PatternGridResponse {
    let available_size = ui.available_size();
    
    let visible_rows = (available_size.y / metrics.row_height).floor() as usize;
    let visible_channels = GridMetrics::calculate_visible_channels(ui, metrics);
    
    let visible_rows = visible_rows.min(pattern.num_rows).max(1);
    let visible_channels = visible_channels.min(num_channels - scroll_channel).max(1);

    let grid_width = metrics.row_num_width + visible_channels as f32 * metrics.channel_width;
    let grid_height = visible_rows as f32 * metrics.row_height;

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(grid_width, grid_height),
        egui::Sense::click_and_drag(),
    );

    let painter = ui.painter_at(rect);
    let mut context_menu_action: Option<ContextMenuAction> = None;
    let mut effect_tooltip: Option<String> = None;
    let mut toggle_sample_length_bg = false;

    let toggle_btn_rect = Rect::from_min_size(
        Pos2::new(rect.left(), rect.top()),
        egui::vec2(metrics.row_num_width, metrics.row_height),
    );
    if toggle_btn_rect.contains(response.interact_pointer_pos().unwrap_or(Pos2::ZERO)) && response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            if pos.y < rect.top() + metrics.row_height && pos.x < rect.left() + metrics.row_num_width {
                toggle_sample_length_bg = true;
            }
        }
    }
    let toggle_rect = Rect::from_min_max(rect.min, Pos2::new(rect.min.x + metrics.row_num_width, rect.min.y + metrics.row_height));
    let toggle_resp = ui.interact(toggle_rect, egui::Id::new("sample_len_bg_toggle"), egui::Sense::hover());
    toggle_resp.on_hover_text("Sample Length BG (Ctrl+Shift+L)");
    let toggle_icon = if sample_length_bg { "▣" } else { "□" };
    let toggle_color = if sample_length_bg { theme.fg_instrument } else { theme.fg_note_empty };
    painter.text(
        Pos2::new(toggle_btn_rect.left() + 2.0, toggle_btn_rect.top() + metrics.row_height * 0.5),
        egui::Align2::LEFT_CENTER,
        toggle_icon,
        egui::FontId::monospace(metrics.font_size),
        toggle_color,
    );

    let first_row = scroll_row;
    let last_row = (first_row + visible_rows).min(pattern.num_rows);
    let first_ch = scroll_channel;
    let last_ch = (first_ch + visible_channels).min(num_channels);

    // Precompute sample length backgrounds
    let sample_len_cache: Vec<f32> = if sample_length_bg {
        if let Some(m) = module {
            let bpm = m.initial_bpm as f32;
            let speed = m.initial_speed as f32;
            m.samples.iter().map(|sample| {
                if !sample.data.is_empty() && sample.sample_rate > 0 {
                    let samples_per_row = (sample.sample_rate as f32 * 60.0 / bpm) / speed;
                    let row_duration = sample.data.len() as f32 / samples_per_row;
                    if row_duration < 1.0 {
                        0.0
                    } else if row_duration < 4.0 {
                        theme.sample_len_shift * 0.3
                    } else if row_duration < 16.0 {
                        theme.sample_len_shift * 0.6
                    } else if row_duration < 64.0 {
                        theme.sample_len_shift * 0.9
                    } else {
                        theme.sample_len_shift
                    }
                } else {
                    0.0
                }
            }).collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    for row in first_row..last_row {
        let display_row = row - first_row;
        let y = rect.top() + display_row as f32 * metrics.row_height;
        let minor = highlight_minor.max(1) as usize;
        let major = highlight_major.max(1) as usize;
        let is_highlight = row % minor == 0;
        let is_measure = row % major == 0;
        let is_playback = playback_row == Some(row);

        let bg = if is_playback {
            let t = ui.input(|i| i.time) as f32;
            let pulse = (t * std::f32::consts::PI).sin() * 0.15 + 0.85;
            let base = theme.bg_playback;
            egui::Color32::from_rgba_premultiplied(
                base.r(), base.g(), base.b(),
                ((base.a() as f32 * pulse) as u8).max(base.a()),
            )
        } else if is_measure {
            theme.bg_measure
        } else if is_highlight {
            theme.bg_highlight
        } else {
            theme.bg_default
        };
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(rect.left(), y), Pos2::new(rect.right(), y + metrics.row_height)),
            0.0,
            bg,
        );

        let row_num_color = if is_measure {
            theme.fg_note
        } else if is_highlight {
            theme.fg_instrument
        } else {
            theme.fg_note_empty
        };
        painter.text(
            Pos2::new(rect.left() + 2.0, y + metrics.row_height * 0.5),
            egui::Align2::LEFT_CENTER,
            format!("{:03}", row),
            egui::FontId::monospace(metrics.font_size),
            row_num_color,
        );

        for ch in first_ch..last_ch {
            let display_ch = ch - first_ch;
            let x = rect.left() + metrics.row_num_width + display_ch as f32 * metrics.channel_width;

            let is_ch_alt = ch % 2 == 1;
            if is_ch_alt {
                let ch_rect = Rect::from_min_max(
                    Pos2::new(x, y),
                    Pos2::new(x + metrics.channel_width - 2.0, y + metrics.row_height),
                );
                painter.rect_filled(ch_rect, 0.0, theme.bg_channel_alt);
            }

            let in_selection = selection.map_or(false, |s| s.contains(row, ch));
            if in_selection {
                let sel_rect = Rect::from_min_max(
                    Pos2::new(x, y),
                    Pos2::new(x + metrics.channel_width - 2.0, y + metrics.row_height),
                );
                painter.rect_filled(sel_rect, 0.0, theme.bg_selected);
            }

            let cell = pattern.cell(row, ch);

            if sample_length_bg {
                if let Some(inst_idx) = cell.instrument {
                    if let Some(m) = module {
                        if let Some(instrument) = m.instruments.get(inst_idx as usize) {
                            let sample_idx = instrument.sample_map[0] as usize;
                            if let Some(&shift) = sample_len_cache.get(sample_idx) {
                                if shift > 0.0 {
                                    let r = Rect::from_min_max(
                                        Pos2::new(x + metrics.channel_width - shift, y),
                                        Pos2::new(x + metrics.channel_width, y + metrics.row_height),
                                    );
                                    painter.rect_filled(r, 0.0, theme.bg_sample_len);
                                }
                            }
                        }
                    }
                }
            }

            let auto_overlay = automation_overlays.get(ch).and_then(|o| o.as_ref());
            draw_cell(&painter, x, y, cell, metrics, theme, col_vis, auto_overlay.is_some());

            if let Some(info) = auto_overlay {
                draw_automation_cell(
                    &painter, x, y, row, ch, metrics, theme, info,
                );
            }

            if let Some(hover_pos) = ui.input(|i| i.pointer.hover_pos()) {
                let cell_rect = Rect::from_min_size(
                    Pos2::new(x, y),
                    egui::vec2(metrics.channel_width - 2.0, metrics.row_height),
                );
                if cell_rect.contains(hover_pos) && cell.effect != Effect::None {
                    let sub_col_x = hover_pos.x - x;
                    let char_pos = sub_col_x / metrics.char_width;
                    if char_pos >= 10.0 {
                        effect_tooltip = Some(effect_tooltip_text(&cell.effect));
                    }
                }
            }
        }
    }

    let display_row = cursor.row.saturating_sub(first_row);
    let display_ch = cursor.channel.saturating_sub(first_ch);
    if display_row < visible_rows && display_ch < visible_channels {
        let cursor_x = rect.left() + metrics.row_num_width + display_ch as f32 * metrics.channel_width;
        let cursor_y = rect.top() + display_row as f32 * metrics.row_height;
        let cursor_rect = Rect::from_min_size(
            Pos2::new(cursor_x, cursor_y),
            egui::vec2(metrics.channel_width - 2.0, metrics.row_height),
        );
        painter.rect_filled(cursor_rect, 0.0, theme.cursor_fill);
        painter.rect_stroke(cursor_rect, 0.0, Stroke::new(1.0, theme.cursor_outline), egui::StrokeKind::Outside);
    }

    if let Some(prow) = playback_row {
        if prow >= first_row && prow < last_row {
            let display_row = prow - first_row;
            let cursor_y = rect.top() + display_row as f32 * metrics.row_height;
            let line_rect = Rect::from_min_max(
                Pos2::new(rect.left(), cursor_y),
                Pos2::new(rect.right(), cursor_y + metrics.row_height),
            );
            painter.rect_stroke(line_rect, 0.0, Stroke::new(1.5, theme.playback_cursor), egui::StrokeKind::Outside);

            if let Some(tick) = playback_tick {
                let progress = tick as f32 / playback_speed.max(1) as f32;
                let bar_h = 2.0;
                let bar_rect = Rect::from_min_max(
                    Pos2::new(rect.left(), cursor_y + metrics.row_height - bar_h),
                    Pos2::new(rect.left() + rect.width() * progress, cursor_y + metrics.row_height),
                );
                painter.rect_filled(bar_rect, 0.0, theme.playback_cursor);
            }
        }
    }

    let mut clicked_position: Option<CursorPosition> = None;
    let mut drag_position: Option<CursorPosition> = None;
    let mut automation_interaction: Option<AutomationInteraction> = None;

    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let rel_x = pos.x - rect.left();
            let rel_y = pos.y - rect.top();
            let display_row = (rel_y / metrics.row_height) as usize;
            let col_x = rel_x - metrics.row_num_width;
            let display_ch = (col_x / metrics.channel_width) as usize;

            let row = first_row + display_row.min(visible_rows.saturating_sub(1));
            let ch = first_ch + display_ch.min(visible_channels.saturating_sub(1));

            let sub_col_x = col_x - display_ch as f32 * metrics.channel_width;

            let auto_overlay = automation_overlays.get(ch).and_then(|o| o.as_ref());
            let in_fx_col = sub_col_x >= metrics.effect_type_x && sub_col_x < metrics.channel_width;

            if let Some(info) = auto_overlay {
                if in_fx_col {
                    let row_y = rel_y - display_row as f32 * metrics.row_height;
                    let value = 1.0 - (row_y / metrics.row_height).clamp(0.0, 1.0);

                    let shift_held = ui.input(|i| i.modifiers.shift);

                    if response.clicked() && !shift_held {
                        automation_interaction = Some(AutomationInteraction::PointCreated {
                            channel: ch,
                            order: info.current_order,
                            row: row as u16,
                            value,
                        });
                    } else if response.dragged() && shift_held {
                        automation_interaction = Some(AutomationInteraction::FreehandDraw {
                            channel: ch,
                            points: vec![(info.current_order, row as u16, value)],
                        });
                    }
                } else {
                    let sub_column = position_to_sub_column(sub_col_x, metrics, col_vis);
                    let cursor_pos = CursorPosition { row, channel: ch, sub_column };
                    if response.clicked() { clicked_position = Some(cursor_pos); }
                    if response.dragged() { drag_position = Some(cursor_pos); }
                }
            } else {
                let sub_column = position_to_sub_column(sub_col_x, metrics, col_vis);
                let cursor_pos = CursorPosition { row, channel: ch, sub_column };
                if response.clicked() { clicked_position = Some(cursor_pos); }
                if response.dragged() { drag_position = Some(cursor_pos); }
            }
        }
    }

    let has_selection = selection.is_some();
    response.context_menu(|ui| {
        // ── Effect commands (context-sensitive: always available) ──
        // "Set Effect" submenu lists all 16 hex effects with their standard
        // tracker names. The "Param" submenu lists the named CLAP-style
        // commands (P/Z/S/R/X). Selecting one sets the cursor cell's effect
        // to that command; with a selection, it sets the effect on all
        // selected cells (param defaults to 0 — user can fine-tune the
        // param high/low via the regular hex typing after picking).
        ui.label(egui::RichText::new("Effect").strong());
        ui.menu_button("Set Effect", |ui| {
            // Standard 0-F effects with their human-readable names
            const EFFECT_NAMES: &[&str] = &[
                "0  Arpeggio",
                "1  Portamento Up",
                "2  Portamento Down",
                "3  Tone Portamento",
                "4  Vibrato",
                "5  TPort + Vol Slide",
                "6  Vibrato + Vol Slide",
                "7  Tremolo",
                "8  Set Panning",
                "9  Set Sample Offset",
                "A  Volume Slide",
                "B  Position Jump",
                "C  Set Volume",
                "D  Pattern Break",
                "E  Extended (E0-EFF)",
                "F  Set Speed",
            ];
            for (i, name) in EFFECT_NAMES.iter().enumerate() {
                if ui.button(*name).clicked() {
                    context_menu_action = Some(ContextMenuAction::SetEffect { hex: i as u8 });
                    ui.close();
                }
            }
        });
        ui.menu_button("Set Param (P/Z/S/R/X)", |ui| {
            const PARAM_NAMES: &[(&str, ParamEffectCommand)] = &[
                ("P  Set Send Bus Param",  ParamEffectCommand::SetSendBusParam),
                ("Z  Set Filter Cutoff",   ParamEffectCommand::SetFilterCutoff),
                ("S  Set Send Level",      ParamEffectCommand::SetSendLevel),
                ("R  Set Filter Resonance",ParamEffectCommand::SetFilterResonance),
                ("X  Set Filter Type",     ParamEffectCommand::SetFilterType),
            ];
            for (label, cmd) in PARAM_NAMES {
                if ui.button(*label).clicked() {
                    context_menu_action = Some(ContextMenuAction::SetParamEffect { command: *cmd });
                    ui.close();
                }
            }
        });
        if ui.button("Clear Effect").clicked() {
            context_menu_action = Some(ContextMenuAction::ClearEffect);
            ui.close();
        }
        ui.separator();

        // ── Block operations (selection-only) ──
        ui.label(egui::RichText::new("Block Operations").strong());
        ui.separator();
        if ui.add_enabled(has_selection, egui::Button::new("Fill Instrument")).clicked() {
            context_menu_action = Some(ContextMenuAction::FillInstrument);
            ui.close();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Interpolate Volume")).clicked() {
            context_menu_action = Some(ContextMenuAction::InterpolateVolume);
            ui.close();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Interpolate Effect")).clicked() {
            context_menu_action = Some(ContextMenuAction::InterpolateEffect);
            ui.close();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Reverse")).clicked() {
            context_menu_action = Some(ContextMenuAction::Reverse);
            ui.close();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Randomize")).clicked() {
            context_menu_action = Some(ContextMenuAction::Randomize);
            ui.close();
        }
    });

    PatternGridResponse {
        visible_rows,
        visible_channels,
        clicked_position,
        drag_position,
        context_menu_action,
        effect_tooltip,
        toggle_sample_length_bg,
        automation_interaction,
    }
}

fn position_to_sub_column(x: f32, metrics: GridMetrics, col_vis: ColumnVisibility) -> SubColumn {
    let mut pos = x;

    if col_vis.note {
        if pos < metrics.note_width {
            return SubColumn::Note;
        }
        pos -= metrics.note_width + metrics.note_to_inst_gap;
    }

    if col_vis.instrument {
        if pos < metrics.inst_width + metrics.char_width * 0.5 {
            return SubColumn::InstrumentTens;
        }
        if pos < metrics.inst_width {
            return SubColumn::InstrumentOnes;
        }
        pos -= metrics.inst_width + metrics.inst_to_vol_gap;
    }

    if col_vis.volume {
        if pos < metrics.vol_width + metrics.char_width * 0.5 {
            return SubColumn::VolumeTens;
        }
        if pos < metrics.vol_width {
            return SubColumn::VolumeOnes;
        }
        pos -= metrics.vol_width + metrics.vol_to_effect_gap;
    }

    if col_vis.effect {
        if pos < metrics.effect_width * 0.33 {
            return SubColumn::EffectType;
        }
        if pos < metrics.effect_width * 0.66 {
            return SubColumn::EffectParamHigh;
        }
        return SubColumn::EffectParamLow;
    }

    SubColumn::Note
}

fn draw_cell(painter: &egui::Painter, x: f32, y: f32, cell: &Cell, metrics: GridMetrics, theme: &TrackerTheme, col_vis: ColumnVisibility, suppress_effect: bool) {
    let font = egui::FontId::monospace(metrics.font_size);
    let center_y = y + metrics.row_height * 0.5;

    if col_vis.note {
        let note_text = match cell.note {
            Note::On(key) => {
                let tone = key % 12;
                let octave = key / 12;
                format!("{}{}", TONE_NAMES[tone as usize], octave)
            }
            Note::Off => "===".to_string(),
            Note::Cut => "^^^".to_string(),
            Note::Fade => "~~~".to_string(),
            Note::None => "---".to_string(),
        };
        let note_color = match cell.note {
            Note::On(_) => theme.fg_note,
            Note::Off => theme.fg_note_off,
            Note::Cut => theme.fg_note_cut,
            Note::Fade => theme.fg_note_off,
            Note::None => theme.fg_note_empty,
        };
        painter.text(Pos2::new(x, center_y), egui::Align2::LEFT_CENTER, note_text, font.clone(), note_color);
    }

    if col_vis.instrument {
        let ins_text = match cell.instrument {
            Some(i) => format!("{:02}", i),
            None => "..".to_string(),
        };
        let ins_color = if cell.instrument.is_some() {
            theme.fg_instrument
        } else {
            theme.fg_note_empty
        };
        painter.text(
            Pos2::new(x + metrics.inst_x, center_y),
            egui::Align2::LEFT_CENTER,
            ins_text,
            font.clone(),
            ins_color,
        );
    }

    if col_vis.volume {
        let vol_text = match cell.volume {
            Some(v) => format!("{:02}", v),
            None => "..".to_string(),
        };
        let vol_color = if cell.volume.is_some() {
            theme.fg_volume
        } else {
            theme.fg_note_empty
        };
        painter.text(
            Pos2::new(x + metrics.vol_x, center_y),
            egui::Align2::LEFT_CENTER,
            vol_text,
            font.clone(),
            vol_color,
        );
    }

    if col_vis.effect && !suppress_effect {
        let (fx_type, fx_param) = format_effect(&cell.effect);
        let fx_type_color = if cell.effect != Effect::None {
            theme.fg_effect
        } else {
            theme.fg_note_empty
        };
        let fx_param_color = if cell.effect != Effect::None {
            theme.fg_effect_param
        } else {
            theme.fg_note_empty
        };
        painter.text(
            Pos2::new(x + metrics.effect_type_x, center_y),
            egui::Align2::LEFT_CENTER,
            fx_type,
            font.clone(),
            fx_type_color,
        );
        painter.text(
            Pos2::new(x + metrics.effect_param1_x(), center_y),
            egui::Align2::LEFT_CENTER,
            fx_param,
            font,
            fx_param_color,
        );
    }
}

fn draw_automation_cell(
    painter: &egui::Painter,
    x: f32,
    y: f32,
    row: usize,
    _ch: usize,
    metrics: GridMetrics,
    theme: &TrackerTheme,
    info: &AutomationOverlayInfo,
) {
    let fx_x = x + metrics.effect_type_x;
    let fx_w = metrics.char_width * 4.0;
    let center_y = y + metrics.row_height * 0.5;
    let cell_rect = Rect::from_min_size(Pos2::new(fx_x, y), egui::vec2(fx_w, metrics.row_height));

    let order = info.current_order;
    let row_u16 = row as u16;

    let (has_point, point_value, interp) = match &info.track {
        Some(track) => {
            let pt = track.points.iter().find(|p| p.order == order && p.row == row_u16);
            match pt {
                Some(p) => (true, p.value, p.interp_to_next),
                None => (false, track.evaluate(order, row_u16, 0, info.speed), InterpolationMode::Hold),
            }
        }
        None => (false, 0.5, InterpolationMode::Hold),
    };

    let normalized = point_value.clamp(0.0, 1.0);
    let dot_y = y + metrics.row_height * (1.0 - normalized);

    painter.rect_filled(cell_rect, 0.0, theme.automation_overlay_bg);

    if has_point {
        painter.circle_filled(Pos2::new(fx_x + fx_w * 0.5, dot_y), 3.0, theme.automation_point);
        let hex_val = (normalized * 255.0) as u8;
        let val_text = format!("{:02X}", hex_val);
        let val_color = theme.automation_value_text;
        painter.text(
            Pos2::new(fx_x, center_y),
            egui::Align2::LEFT_CENTER,
            val_text,
            egui::FontId::monospace(metrics.font_size * 0.85),
            val_color,
        );

        let interp_char = match interp {
            InterpolationMode::Hold => "·",
            InterpolationMode::Linear => "/",
            InterpolationMode::Smooth => "~",
            InterpolationMode::Exponential => "^",
        };
        painter.text(
            Pos2::new(fx_x + metrics.char_width * 2.5, center_y),
            egui::Align2::LEFT_CENTER,
            interp_char,
            egui::FontId::monospace(metrics.font_size * 0.7),
            theme.fg_dim,
        );
    } else {
        let line_x = fx_x + fx_w * 0.5;
        let line_color = theme.automation_guide_line;
        painter.line_segment(
            [Pos2::new(line_x, y), Pos2::new(line_x, y + metrics.row_height)],
            Stroke::new(0.5, line_color),
        );
        painter.circle_filled(Pos2::new(line_x, dot_y), 1.5, theme.automation_guide_line);
    }
}

fn format_effect(effect: &Effect) -> (String, String) {
    match effect {
        Effect::None => (".".to_string(), "..".to_string()),
        Effect::Arpeggio { note1, note2 } => ("0".to_string(), format!("{:X}{:X}", note1, note2)),
        Effect::PortamentoUp { speed } => ("1".to_string(), format!("{:02X}", speed)),
        Effect::PortamentoDown { speed } => ("2".to_string(), format!("{:02X}", speed)),
        Effect::TonePortamento { speed } => ("3".to_string(), format!("{:02X}", speed)),
        Effect::Vibrato { speed, depth } => ("4".to_string(), format!("{:X}{:X}", speed, depth)),
        Effect::TonePortamentoVolumeSlide { up } => ("5".to_string(), format!("{:02X}", *up as u8)),
        Effect::VibratoVolumeSlide { up } => ("6".to_string(), format!("{:02X}", *up as u8)),
        Effect::Tremolo { speed, depth } => ("7".to_string(), format!("{:X}{:X}", speed, depth)),
        Effect::SetPanning { pan } => ("8".to_string(), format!("{:02X}", pan)),
        Effect::SetSampleOffset { offset } => ("9".to_string(), format!("{:02X}", offset >> 8)),
        Effect::VolumeSlide { up, down } => ("A".to_string(), format!("{:X}{:X}", up, down)),
        Effect::PositionJump { order } => ("B".to_string(), format!("{:02X}", order)),
        Effect::SetVolume { volume } => ("C".to_string(), format!("{:02X}", volume)),
        Effect::PatternBreak { row } => ("D".to_string(), format!("{:02X}", row)),
        Effect::PanningSlide { speed } => (".".to_string(), format!("P{:+}", speed)),
        Effect::ExtendedEffect { param } => ("E".to_string(), format!("{:02X}", param)),
        Effect::SetSpeed { speed } => ("F".to_string(), format!("{:02X}", speed)),
        Effect::SetTempo { bpm } => ("F".to_string(), format!("{:02X}", bpm)),
        Effect::SetGlobalVolume { volume } => ("G".to_string(), format!("{:02X}", volume)),
        Effect::GlobalVolumeSlide { up, down } => ("H".to_string(), format!("{:X}{:X}", (*up).max(0) as u8, (*down).unsigned_abs().min(15) as u8)),
        Effect::SetEnvelopePosition { tick } => ("L".to_string(), format!("{:02X}", tick)),
        Effect::Panbrello { speed, depth } => ("Y".to_string(), format!("{:X}{:X}", speed, depth)),
        Effect::PatternDelay { ticks } => ("E".to_string(), format!("E{:X}", ticks)),
        Effect::SetPanPosition { pan } => ("E".to_string(), format!("8{:X}", pan >> 4)),
        Effect::GlissandoControl { on } => ("E".to_string(), if *on { "3F".to_string() } else { "30".to_string() }),
        Effect::VibratoWaveform { waveform } => ("E".to_string(), format!("4{:X}", waveform & 0x03)),
        Effect::SetFineTune { tune } => ("E".to_string(), format!("5{:X}", tune)),
        Effect::PatternLoop { count } => ("E".to_string(), format!("6{:X}", count)),
        Effect::TremoloWaveform { waveform } => ("E".to_string(), format!("7{:X}", waveform & 0x03)),
        Effect::SetPanning16 { pan } => ("E".to_string(), format!("8{:X}", pan >> 4)),
        Effect::Retrigger { interval } => ("E".to_string(), format!("9{:X}", interval)),
        Effect::NoteCutAfter { ticks } => ("E".to_string(), format!("C{:X}", ticks)),
        Effect::NoteDelay { ticks } => ("E".to_string(), format!("D{:X}", ticks)),
        Effect::ExtraFinePortamentoUp { speed } => ("F".to_string(), format!("1{:X}", speed & 0xF)),
        Effect::ExtraFinePortamentoDown { speed } => ("F".to_string(), format!("2{:X}", speed & 0xF)),
        Effect::FinePortamentoUp { speed } => ("E".to_string(), format!("1{:X}", speed >> 4)),
        Effect::FinePortamentoDown { speed } => ("E".to_string(), format!("2{:X}", speed >> 4)),
        Effect::FineVolumeSlideUp { amount } => ("E".to_string(), format!("A{:X}", amount)),
        Effect::FineVolumeSlideDown { amount } => ("E".to_string(), format!("B{:X}", amount)),
        Effect::Tremor { ontime, offtime } => ("I".to_string(), format!("{:X}{:X}", ontime, offtime)),
        Effect::VolSetVolume { vol } => (".".to_string(), format!("{:02X}", (*vol).min(64))),
        Effect::VolFineSlideUp { amount } => (".".to_string(), format!("+{:X}", amount)),
        Effect::VolFineSlideDown { amount } => (".".to_string(), format!("-{:X}", amount)),
        Effect::VolSlideUp { amount } => (".".to_string(), format!("U{:X}", amount)),
        Effect::VolSlideDown { amount } => (".".to_string(), format!("D{:X}", amount)),
        Effect::VolPortamento { speed } => (".".to_string(), format!("~{:02X}", speed)),
        Effect::VolVibrato { speed } => (".".to_string(), format!("V{:X}", speed)),
        Effect::SetFilterCutoff { cutoff } => ("Z".to_string(), format!("{:02X}", cutoff >> 8)),
        Effect::SetFilterResonance { resonance } => ("R".to_string(), format!("{:02X}", resonance)),
        Effect::SetFilterType { filter_type } => ("X".to_string(), format!("{:02X}", filter_type)),
        Effect::FilterCutoffSlide { amount } => ("Y".to_string(), format!("{:+03}", amount)),
        Effect::SetSendLevel { send_index, level } => ("S".to_string(), format!("{:X}{:X}", send_index, level)),
        Effect::SetSendBusParam { bus, param, value: _ } => ("P".to_string(), format!("{:X}{:X}", bus, param)),
        Effect::FormatSpecific(fe) => {
            match fe {
                FormatEffect::Xm(xe) => match xe {
                    XmEffect::SetSampleOffset(o) => ("9".to_string(), format!("{:02X}", o >> 8)),
                    XmEffect::KeyOff { .. } => ("K".to_string(), "00".to_string()),
                    XmEffect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                    _ => ("x".to_string(), "??".to_string()),
                },
                FormatEffect::It(ie) => match ie {
                    ItEffect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                    _ => ("i".to_string(), "??".to_string()),
                },
                FormatEffect::Mod(me) => match me {
                    ModEffect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                    ModEffect::Filter(enabled) => ("E".to_string(), if *enabled { "00".to_string() } else { "01".to_string() }),
                    ModEffect::FunkIt { speed } => ("EF".to_string(), format!("{:01X}", speed)),
                    ModEffect::KarplusStrong { param } => ("E8".to_string(), format!("{:01X}", param)),
                },
                FormatEffect::S3m(se) => match se {
                    S3mEffect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                    _ => ("s".to_string(), "??".to_string()),
                },
                FormatEffect::C669(ce) => match ce {
                    C669Effect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                    _ => ("?".to_string(), "??".to_string()),
                },
                FormatEffect::Mmd(me) => match me {
                    MmdEffect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                    _ => ("?".to_string(), "??".to_string()),
                },
                FormatEffect::Ult(ue) => match ue {
                    UltEffect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                    _ => ("?".to_string(), "??".to_string()),
                },
                FormatEffect::Stm(se) => match se {
                    StmEffect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                },
            }
        }
    }
}

fn effect_tooltip_text(effect: &Effect) -> String {
    match effect {
        Effect::Arpeggio { note1, note2 } => format!("Arpeggio: +{} +{} semitones", note1, note2),
        Effect::PortamentoUp { speed } => format!("Portamento Up: speed={}", speed),
        Effect::PortamentoDown { speed } => format!("Portamento Down: speed={}", speed),
        Effect::TonePortamento { speed } => format!("Tone Portamento: speed={}", speed),
        Effect::Vibrato { speed, depth } => format!("Vibrato: speed={} depth={}", speed, depth),
        Effect::TonePortamentoVolumeSlide { .. } => "Tone Porta + Vol Slide".to_string(),
        Effect::VibratoVolumeSlide { .. } => "Vibrato + Vol Slide".to_string(),
        Effect::Tremolo { speed, depth } => format!("Tremolo: speed={} depth={}", speed, depth),
        Effect::SetPanning { pan } => {
            let pct = (*pan as f32 / 255.0 * 100.0) as u8;
            if *pan < 85 { format!("Pan: {} (left {}%)", pan, pct) }
            else if *pan > 170 { format!("Pan: {} (right {}%)", pan, pct) }
            else { format!("Pan: {} (center {}%)", pan, pct) }
        }
        Effect::SetSampleOffset { offset } => format!("Sample Offset: {}", offset),
        Effect::VolumeSlide { up, down } => {
            if *up > 0 { format!("Vol Slide Up: {}", up) }
            else { format!("Vol Slide Down: {}", down) }
        }
        Effect::PositionJump { order } => format!("Position Jump: order {}", order),
        Effect::SetVolume { volume } => format!("Set Volume: {}/64", (*volume).min(64)),
        Effect::PatternBreak { row } => format!("Pattern Break: row {}", row),
        Effect::ExtendedEffect { param } => {
            let sub = (param >> 4) & 0x0F;
            let val = param & 0x0F;
            match sub {
                0 => "Set Filter".to_string(),
                1 => format!("Fine Porta Up: {}", val),
                2 => format!("Fine Porta Down: {}", val),
                3 => format!("Glissando: {}", if val > 0 { "On" } else { "Off" }),
                4 => format!("Vibrato Waveform: {}", match val { 0 => "Sine", 1 => "Ramp", 2 => "Square", _ => "Random" }),
                5 => format!("Set Fine Tune: {}", val),
                6 => format!("Pattern Loop: {}", if val == 0 { "Set marker".to_string() } else { format!("Loop {}x", val) }),
                7 => format!("Tremolo Waveform: {}", match val { 0 => "Sine", 1 => "Ramp", 2 => "Square", _ => "Random" }),
                8 => format!("Set Panning (fine)"),
                9 => format!("Retrigger: every {} ticks", val),
                0xA => format!("Fine Vol Up: {}", val),
                0xB => format!("Fine Vol Down: {}", val),
                0xC => format!("Note Cut after {} ticks", val),
                0xD => format!("Note Delay: {} ticks", val),
                0xE => "Pattern Delay".to_string(),
                _ => format!("Extended E{:X}{:02X}", sub, val),
            }
        }
        Effect::SetSpeed { speed } => format!("Speed: {} ticks/row", speed),
        Effect::SetTempo { bpm } => format!("Tempo: {} BPM", bpm),
        Effect::SetGlobalVolume { volume } => format!("Global Volume: {}", volume),
        Effect::GlobalVolumeSlide { .. } => "Global Volume Slide".to_string(),
        Effect::SetEnvelopePosition { tick } => format!("Envelope Position: tick {}", tick),
        Effect::Panbrello { speed, depth } => format!("Panbrello: speed={} depth={}", speed, depth),
        Effect::PatternDelay { ticks } => format!("Pattern Delay: {} ticks", ticks),
        Effect::SetPanPosition { pan } => format!("Pan Position: {}", pan),
        Effect::GlissandoControl { on } => format!("Glissando: {}", if *on { "On" } else { "Off" }),
        Effect::VibratoWaveform { waveform } => format!("Vibrato Waveform: {}", match waveform & 3 { 0 => "Sine", 1 => "Ramp", 2 => "Square", _ => "Random" }),
        Effect::SetFineTune { tune } => format!("Fine Tune: {}", tune),
        Effect::PatternLoop { count } => format!("Pattern Loop: {}", if *count == 0 { "Set marker".to_string() } else { format!("Loop {}x", count) }),
        Effect::TremoloWaveform { waveform } => format!("Tremolo Waveform: {}", match waveform & 3 { 0 => "Sine", 1 => "Ramp", 2 => "Square", _ => "Random" }),
        Effect::SetPanning16 { pan } => format!("Fine Panning: {}", pan),
        Effect::Retrigger { interval } => format!("Retrigger: every {} ticks", interval),
        Effect::NoteCutAfter { ticks } => format!("Note Cut after {} ticks", ticks),
        Effect::NoteDelay { ticks } => format!("Note Delay: {} ticks", ticks),
        Effect::ExtraFinePortamentoUp { speed } => format!("Extra Fine Porta Up: {}", speed),
        Effect::ExtraFinePortamentoDown { speed } => format!("Extra Fine Porta Down: {}", speed),
        Effect::FinePortamentoUp { speed } => format!("Fine Porta Up: {}", speed),
        Effect::FinePortamentoDown { speed } => format!("Fine Porta Down: {}", speed),
        Effect::FineVolumeSlideUp { amount } => format!("Fine Vol Up: {}", amount),
        Effect::FineVolumeSlideDown { amount } => format!("Fine Vol Down: {}", amount),
        Effect::Tremor { ontime, offtime } => format!("Tremor: on={} off={}", ontime, offtime),
        Effect::SetFilterCutoff { cutoff } => format!("Filter Cutoff: {}", cutoff),
        Effect::SetFilterResonance { resonance } => format!("Filter Resonance: {}", resonance),
        Effect::SetFilterType { filter_type } => format!("Filter Type: {}", match filter_type { 0 => "LP", 1 => "HP", 2 => "BP", _ => "Notch" }),
        Effect::FilterCutoffSlide { amount } => format!("Filter Cutoff Slide: {}", amount),
        Effect::SetSendLevel { send_index, level } => format!("Send Level: bus {} at {}%", send_index, (*level as u16) * 100 / 15),
        Effect::SetSendBusParam { bus, param, value } => format!("Send Param: bus {} param {} value {}", bus, param, value),
        _ => String::new(),
    }
}


