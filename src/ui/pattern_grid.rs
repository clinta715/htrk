use eframe::egui::{self, Pos2, Rect, Stroke};

use crate::sequencer::effect::{Effect, FormatEffect, XmEffect, ModEffect, S3mEffect, ItEffect};
use crate::sequencer::note::{Note, TONE_NAMES};
use crate::sequencer::pattern::Cell;

use super::theme::TrackerTheme;

pub const FONT_SIZE: f32 = 13.0;
pub const ROW_HEIGHT: f32 = 17.0;
pub const ROW_NUM_WIDTH: f32 = 32.0;
pub const CHAR_WIDTH: f32 = 7.8;
pub const CHANNEL_WIDTH: f32 = 14.0 * CHAR_WIDTH;
pub const VISIBLE_ROWS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubColumn {
    Note,
    InstrumentHigh,
    InstrumentLow,
    VolumeHigh,
    VolumeLow,
    EffectType,
    EffectParamHigh,
    EffectParamLow,
}

impl SubColumn {
    #[allow(dead_code)]
    pub fn all() -> &'static [SubColumn] {
        &[
            SubColumn::Note,
            SubColumn::InstrumentHigh,
            SubColumn::InstrumentLow,
            SubColumn::VolumeHigh,
            SubColumn::VolumeLow,
            SubColumn::EffectType,
            SubColumn::EffectParamHigh,
            SubColumn::EffectParamLow,
        ]
    }

    pub fn next(self) -> Option<SubColumn> {
        match self {
            SubColumn::Note => Some(SubColumn::InstrumentHigh),
            SubColumn::InstrumentHigh => Some(SubColumn::InstrumentLow),
            SubColumn::InstrumentLow => Some(SubColumn::VolumeHigh),
            SubColumn::VolumeHigh => Some(SubColumn::VolumeLow),
            SubColumn::VolumeLow => Some(SubColumn::EffectType),
            SubColumn::EffectType => Some(SubColumn::EffectParamHigh),
            SubColumn::EffectParamHigh => Some(SubColumn::EffectParamLow),
            SubColumn::EffectParamLow => None,
        }
    }

    pub fn prev(self) -> Option<SubColumn> {
        match self {
            SubColumn::Note => None,
            SubColumn::InstrumentHigh => Some(SubColumn::Note),
            SubColumn::InstrumentLow => Some(SubColumn::InstrumentHigh),
            SubColumn::VolumeHigh => Some(SubColumn::InstrumentLow),
            SubColumn::VolumeLow => Some(SubColumn::VolumeHigh),
            SubColumn::EffectType => Some(SubColumn::VolumeLow),
            SubColumn::EffectParamHigh => Some(SubColumn::EffectType),
            SubColumn::EffectParamLow => Some(SubColumn::EffectParamHigh),
        }
    }

    pub fn accepts_hex(self) -> bool {
        !matches!(self, SubColumn::Note)
    }

    pub fn accepts_note(self) -> bool {
        matches!(self, SubColumn::Note)
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
}

pub struct PatternGridResponse {
    pub _cursor_moved: bool,
    pub _cell_edited: bool,
    pub _selection_changed: bool,
    pub clicked_position: Option<CursorPosition>,
    pub drag_position: Option<CursorPosition>,
    pub context_menu_action: Option<ContextMenuAction>,
    pub effect_tooltip: Option<String>,
}

pub fn draw_pattern_grid(
    ui: &mut egui::Ui,
    pattern: &crate::sequencer::pattern::Pattern,
    cursor: &CursorPosition,
    selection: Option<&Selection>,
    playback_row: Option<usize>,
    scroll_row: usize,
    scroll_channel: usize,
    num_channels: usize,
    theme: &TrackerTheme,
) -> PatternGridResponse {
    let visible_channels = num_channels.min(16);
    let visible_rows = VISIBLE_ROWS.min(pattern.num_rows);

    let grid_width = ROW_NUM_WIDTH + visible_channels as f32 * CHANNEL_WIDTH;
    let grid_height = visible_rows as f32 * ROW_HEIGHT;

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(grid_width, grid_height),
        egui::Sense::click_and_drag(),
    );

    let painter = ui.painter_at(rect);
    let mut context_menu_action: Option<ContextMenuAction> = None;
    let mut effect_tooltip: Option<String> = None;

    let _grid_response = PatternGridResponse {
        _cursor_moved: false,
        _cell_edited: false,
        _selection_changed: false,
        clicked_position: None,
        drag_position: None,
        context_menu_action: None,
        effect_tooltip: None,
    };

    let first_row = scroll_row;
    let last_row = (first_row + visible_rows).min(pattern.num_rows);
    let first_ch = scroll_channel;
    let last_ch = (first_ch + visible_channels).min(num_channels);

    for row in first_row..last_row {
        let display_row = row - first_row;
        let y = rect.top() + display_row as f32 * ROW_HEIGHT;
        let is_highlight = row % 4 == 0;
        let is_measure = row % 16 == 0;
        let is_playback = playback_row == Some(row);

        let bg = if is_playback {
            theme.bg_playback
        } else if is_measure {
            theme.bg_measure
        } else if is_highlight {
            theme.bg_highlight
        } else {
            theme.bg_default
        };
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(rect.left(), y), Pos2::new(rect.right(), y + ROW_HEIGHT)),
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
            Pos2::new(rect.left() + 2.0, y + ROW_HEIGHT * 0.5),
            egui::Align2::LEFT_CENTER,
            format!("{:03}", row),
            egui::FontId::monospace(FONT_SIZE),
            row_num_color,
        );

        for ch in first_ch..last_ch {
            let display_ch = ch - first_ch;
            let x = rect.left() + ROW_NUM_WIDTH + display_ch as f32 * CHANNEL_WIDTH;

            let is_ch_alt = ch % 2 == 1;
            if is_ch_alt {
                let ch_rect = Rect::from_min_max(
                    Pos2::new(x, y),
                    Pos2::new(x + CHANNEL_WIDTH - 2.0, y + ROW_HEIGHT),
                );
                painter.rect_filled(ch_rect, 0.0, theme.bg_channel_alt);
            }

            let in_selection = selection.map_or(false, |s| s.contains(row, ch));
            if in_selection {
                let sel_rect = Rect::from_min_max(
                    Pos2::new(x, y),
                    Pos2::new(x + CHANNEL_WIDTH - 2.0, y + ROW_HEIGHT),
                );
                painter.rect_filled(sel_rect, 0.0, theme.bg_selected);
            }

            let cell = pattern.cell(row, ch);
            draw_cell(&painter, x, y, cell, theme);

            if let Some(hover_pos) = ui.input(|i| i.pointer.hover_pos()) {
                let cell_rect = Rect::from_min_size(
                    Pos2::new(x, y),
                    egui::vec2(CHANNEL_WIDTH - 2.0, ROW_HEIGHT),
                );
                if cell_rect.contains(hover_pos) && cell.effect != Effect::None {
                    let sub_col_x = hover_pos.x - x;
                    let char_pos = sub_col_x / CHAR_WIDTH;
                    if char_pos >= 10.0 {
                        effect_tooltip = Some(effect_tooltip_text(&cell.effect));
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
        Effect::TonePortamentoVolumeSlide { up } => format!("Tone Porta + Vol Slide"),
        Effect::VibratoVolumeSlide { up } => format!("Vibrato + Vol Slide"),
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
        Effect::FinePortamentoUp { speed } => format!("Fine Porta Up: {}", speed),
        Effect::FinePortamentoDown { speed } => format!("Fine Porta Down: {}", speed),
        Effect::FineVolumeSlideUp { amount } => format!("Fine Vol Up: {}", amount),
        Effect::FineVolumeSlideDown { amount } => format!("Fine Vol Down: {}", amount),
        Effect::Tremor { ontime, offtime } => format!("Tremor: on={} off={}", ontime, offtime),
        Effect::SetFilterCutoff { cutoff } => format!("Filter Cutoff: {}", cutoff),
        Effect::SetFilterResonance { resonance } => format!("Filter Resonance: {}", resonance),
        Effect::SetFilterType { filter_type } => format!("Filter Type: {}", match filter_type { 0 => "LP", 1 => "HP", 2 => "BP", _ => "Notch" }),
        Effect::FilterCutoffSlide { amount } => format!("Filter Cutoff Slide: {}", amount),
        _ => String::new(),
    }
}
        }
    }

    if let Some(cursor) = Some(cursor) {
        let display_row = cursor.row.saturating_sub(first_row);
        let display_ch = cursor.channel.saturating_sub(first_ch);
        if display_row < visible_rows && display_ch < visible_channels {
            let cursor_x = rect.left() + ROW_NUM_WIDTH + display_ch as f32 * CHANNEL_WIDTH;
            let cursor_y = rect.top() + display_row as f32 * ROW_HEIGHT;
            let cursor_rect = Rect::from_min_size(
                Pos2::new(cursor_x, cursor_y),
                egui::vec2(CHANNEL_WIDTH - 2.0, ROW_HEIGHT),
            );
            painter.rect_filled(cursor_rect, 0.0, theme.cursor_fill);
            painter.rect_stroke(cursor_rect, 0.0, Stroke::new(1.0, theme.cursor_outline), egui::StrokeKind::Outside);
        }
    }

    if let Some(prow) = playback_row {
        if prow >= first_row && prow < last_row {
            let display_row = prow - first_row;
            let cursor_y = rect.top() + display_row as f32 * ROW_HEIGHT;
            let line_rect = Rect::from_min_max(
                Pos2::new(rect.left(), cursor_y),
                Pos2::new(rect.right(), cursor_y + ROW_HEIGHT),
            );
            painter.rect_stroke(line_rect, 0.0, Stroke::new(1.5, theme.playback_cursor), egui::StrokeKind::Outside);
        }
    }

    let mut clicked_position: Option<CursorPosition> = None;
    let mut drag_position: Option<CursorPosition> = None;

    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let rel_x = pos.x - rect.left();
            let rel_y = pos.y - rect.top();
            let display_row = (rel_y / ROW_HEIGHT) as usize;
            let col_x = rel_x - ROW_NUM_WIDTH;
            let display_ch = (col_x / CHANNEL_WIDTH) as usize;

            let row = first_row + display_row.min(visible_rows.saturating_sub(1));
            let ch = first_ch + display_ch.min(visible_channels.saturating_sub(1));

            let sub_col_x = col_x - display_ch as f32 * CHANNEL_WIDTH;
            let sub_column = position_to_sub_column(sub_col_x);

            let cursor_pos = CursorPosition {
                row,
                channel: ch,
                sub_column,
            };

            if response.clicked() {
                clicked_position = Some(cursor_pos);
            }
            if response.dragged() {
                drag_position = Some(cursor_pos);
            }
        }
    }

    let has_selection = selection.is_some();
    response.context_menu(|ui| {
        ui.label(egui::RichText::new("Block Operations").strong());
        ui.separator();
        if ui.add_enabled(has_selection, egui::Button::new("Fill Instrument")).clicked() {
            context_menu_action = Some(ContextMenuAction::FillInstrument);
            ui.close_menu();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Interpolate Volume")).clicked() {
            context_menu_action = Some(ContextMenuAction::InterpolateVolume);
            ui.close_menu();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Interpolate Effect")).clicked() {
            context_menu_action = Some(ContextMenuAction::InterpolateEffect);
            ui.close_menu();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Reverse")).clicked() {
            context_menu_action = Some(ContextMenuAction::Reverse);
            ui.close_menu();
        }
        if ui.add_enabled(has_selection, egui::Button::new("Randomize")).clicked() {
            context_menu_action = Some(ContextMenuAction::Randomize);
            ui.close_menu();
        }
    });

    PatternGridResponse {
        _cursor_moved: clicked_position.is_some(),
        _cell_edited: false,
        _selection_changed: drag_position.is_some(),
        clicked_position,
        drag_position,
        context_menu_action,
        effect_tooltip,
    }
}

fn position_to_sub_column(x: f32) -> SubColumn {
    let char_pos = x / CHAR_WIDTH;
    if char_pos < 4.0 {
        SubColumn::Note
    } else if char_pos < 6.0 {
        if char_pos < 5.0 {
            SubColumn::InstrumentHigh
        } else {
            SubColumn::InstrumentLow
        }
    } else if char_pos < 9.0 {
        if char_pos < 8.0 {
            SubColumn::VolumeHigh
        } else {
            SubColumn::VolumeLow
        }
    } else if char_pos < 10.0 {
        SubColumn::EffectType
    } else if char_pos < 11.0 {
        SubColumn::EffectParamHigh
    } else {
        SubColumn::EffectParamLow
    }
}

fn draw_cell(painter: &egui::Painter, x: f32, y: f32, cell: &Cell, theme: &TrackerTheme) {
    let font = egui::FontId::monospace(FONT_SIZE);
    let center_y = y + ROW_HEIGHT * 0.5;

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

    let ins_text = match cell.instrument {
        Some(i) => format!("{:02X}", i),
        None => "..".to_string(),
    };
    let ins_color = if cell.instrument.is_some() {
        theme.fg_instrument
    } else {
        theme.fg_note_empty
    };
    painter.text(
        Pos2::new(x + CHAR_WIDTH * 4.0, center_y),
        egui::Align2::LEFT_CENTER,
        ins_text,
        font.clone(),
        ins_color,
    );

    let vol_text = match cell.volume {
        Some(v) => format!("{:02X}", v),
        None => "..".to_string(),
    };
    let vol_color = if cell.volume.is_some() {
        theme.fg_volume
    } else {
        theme.fg_note_empty
    };
    painter.text(
        Pos2::new(x + CHAR_WIDTH * 7.0, center_y),
        egui::Align2::LEFT_CENTER,
        vol_text,
        font.clone(),
        vol_color,
    );

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
        Pos2::new(x + CHAR_WIDTH * 10.0, center_y),
        egui::Align2::LEFT_CENTER,
        fx_type,
        font.clone(),
        fx_type_color,
    );
    painter.text(
        Pos2::new(x + CHAR_WIDTH * 11.0, center_y),
        egui::Align2::LEFT_CENTER,
        fx_param,
        font,
        fx_param_color,
    );
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
                    _ => ("m".to_string(), "??".to_string()),
                },
                FormatEffect::S3m(se) => match se {
                    S3mEffect::Raw { effect, param } => (format!("{:X}", effect), format!("{:02X}", param)),
                    _ => ("s".to_string(), "??".to_string()),
                },
            }
        }
    }
}
