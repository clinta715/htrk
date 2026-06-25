use std::sync::Arc;

use crate::edit::{SetCellCommand, BulkSetCellsCommand, TransposeCommand};
use crate::sequencer::effect::Effect;
use crate::sequencer::module::MAX_CHANNELS;
use crate::sequencer::note::Note;
use crate::sequencer::pattern::Cell;
use crate::ui::pattern_grid::{CursorPosition, Selection, SubColumn};

use super::HtrkCore;

impl HtrkCore {
    pub fn set_cell_at_cursor(&mut self, new_cell: Cell, multichannel_channels: &[bool], multichannel_enabled: bool) {
        let cursor = self.cursor;
        let channels: Vec<usize> = if multichannel_enabled {
            multichannel_channels.iter()
                .enumerate()
                .filter(|(_, &active)| active)
                .map(|(ch, _)| ch)
                .collect()
        } else {
            vec![cursor.channel]
        };

        let old_cells: Vec<Cell> = channels.iter().map(|&ch| {
            let saved = self.cursor;
            self.cursor.channel = ch;
            let cell = self.get_cell_at_cursor();
            self.cursor = saved;
            cell
        }).collect();

        self.ensure_pattern_exists();
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                for (idx, &ch) in channels.iter().enumerate() {
                    let old_cell = old_cells[idx];
                    if old_cell == new_cell {
                        continue;
                    }
                    let cmd = Box::new(SetCellCommand {
                        order: self.selected_order,
                        row: cursor.row,
                        channel: ch,
                        old_cell,
                        new_cell: new_cell.clone(),
                    });
                    let _ = self.undo_manager.execute(cmd, arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }

    pub fn clear_cell_at_cursor(&mut self) {
        self.set_cell_at_cursor(Cell::default(), &[], false);
    }

    pub fn get_cell_at_cursor(&self) -> Cell {
        if let Some(pattern) = self.current_pattern() {
            if self.cursor.row < pattern.num_rows && self.cursor.channel < MAX_CHANNELS {
                *pattern.cell(self.cursor.row, self.cursor.channel)
            } else {
                Cell::default()
            }
        } else {
            Cell::default()
        }
    }

    pub fn copy_selection(&mut self) {
        if let Some(sel) = &self.selection {
            let (min, max) = sel.normalized();
            if let Some(pattern) = self.current_pattern() {
                let mut data = Vec::new();
                for row in min.row..=max.row {
                    let mut row_data = Vec::new();
                    for ch in min.channel..=max.channel {
                        row_data.push(*pattern.cell(row, ch));
                    }
                    data.push(row_data);
                }
                self.clipboard = Some(data);
                self.clipboard_width = max.channel - min.channel + 1;
            }
        }
    }

    pub fn delete_selection(&mut self) {
        let sel = match &self.selection {
            Some(s) => s.clone(),
            None => return,
        };
        let (min, max) = sel.normalized();
        let selected_order = self.selected_order;

        self.ensure_pattern_exists();
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;
                let pattern = &arc_module.patterns[pat_idx];
                let mut old_cells = Vec::new();
                let mut new_cells = Vec::new();
                for row in min.row..=max.row {
                    for ch in min.channel..=max.channel {
                        if row < pattern.num_rows && ch < MAX_CHANNELS {
                            let old = pattern.data[row][ch];
                            if !old.is_empty() {
                                old_cells.push((row, ch, old));
                                new_cells.push((row, ch, Cell::default()));
                            }
                        }
                    }
                }
                if !old_cells.is_empty() {
                    let cmd = Box::new(BulkSetCellsCommand {
                        order: selected_order,
                        old_cells,
                        new_cells,
                    });
                    let _ = self.undo_manager.execute(cmd, arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }

    pub fn paste_at_cursor(&mut self) {
        let clipboard_data = match &self.clipboard {
            Some(d) => d.clone(),
            None => return,
        };
        let selected_order = self.selected_order;
        let cursor_row = self.cursor.row;
        let cursor_ch = self.cursor.channel;

        self.ensure_pattern_exists();
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;
                let pattern = &arc_module.patterns[pat_idx];
                let mut old_cells = Vec::new();
                let mut new_cells = Vec::new();
                for (row_offset, row_data) in clipboard_data.iter().enumerate() {
                    let target_row = cursor_row + row_offset;
                    if target_row >= pattern.num_rows {
                        continue;
                    }
                    for (ch_offset, cell) in row_data.iter().enumerate() {
                        let target_ch = cursor_ch + ch_offset;
                        if target_ch >= MAX_CHANNELS {
                            continue;
                        }
                        if cell.is_empty() {
                            continue;
                        }
                        let old = pattern.data[target_row][target_ch];
                        old_cells.push((target_row, target_ch, old));
                        new_cells.push((target_row, target_ch, *cell));
                    }
                }
                if !old_cells.is_empty() {
                    let cmd = Box::new(BulkSetCellsCommand {
                        order: selected_order,
                        old_cells,
                        new_cells,
                    });
                    let _ = self.undo_manager.execute(cmd, arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }

    pub fn select_all(&mut self) {
        let pattern = self.current_pattern_or_default();
        let num_ch = self.num_channels();
        let sel = Selection {
            start: CursorPosition {
                row: 0,
                channel: 0,
                sub_column: SubColumn::Note,
            },
            end: CursorPosition {
                row: pattern.num_rows - 1,
                channel: num_ch - 1,
                sub_column: SubColumn::EffectParamLow,
            },
        };
        self.selection = Some(sel);
    }

    pub fn select_column(&mut self) {
        let pattern = self.current_pattern_or_default();
        let channel = self.cursor.channel.min(self.num_channels().saturating_sub(1));
        let sel = Selection {
            start: CursorPosition {
                row: 0,
                channel,
                sub_column: SubColumn::Note,
            },
            end: CursorPosition {
                row: pattern.num_rows - 1,
                channel,
                sub_column: SubColumn::EffectParamLow,
            },
        };
        self.selection = Some(sel);
        self.selection_anchor = Some(self.cursor);
    }

    pub fn transpose_selection(&mut self, delta: i8) {
        let sel = match &self.selection {
            Some(s) => s.clone(),
            None => {
                let cursor = self.cursor;
                Selection { start: cursor, end: cursor }
            }
        };
        let (min, max) = sel.normalized();
        let selected_order = self.selected_order;

        self.ensure_pattern_exists();
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;
                let mut old_notes = Vec::new();
                for row in min.row..=max.row {
                    for ch in min.channel..=max.channel {
                        let note = arc_module.patterns[pat_idx].data[row][ch].note;
                        if let Note::On(_) = note {
                            old_notes.push((row, ch, note));
                        }
                    }
                }
                let cmd = TransposeCommand {
                    order: selected_order,
                    delta,
                    old_notes,
                };
                let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
            }
        }
        self.sync_module_to_audio();
    }

    pub fn handle_context_menu_action(&mut self, action: crate::ui::pattern_grid::ContextMenuAction) {
        let selected_order = self.selected_order;

        self.ensure_pattern_exists();
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;

                // Block operations and selection-dependent actions:
                // if no selection is active, use the cursor as a
                // single-cell "selection" (row:cursor.row, ch:cursor.channel).
                let sel = match &self.selection {
                    Some(s) => s.clone(),
                    None => Selection {
                        start: self.cursor,
                        end: self.cursor,
                    },
                };
                let (min, max) = sel.normalized();

                match action {
                    crate::ui::pattern_grid::ContextMenuAction::FillInstrument => {
                        let mut old_cells = Vec::new();
                        for row in min.row..=max.row {
                            for ch in min.channel..=max.channel {
                                let cell = arc_module.patterns[pat_idx].data[row][ch];
                                if cell.note != Note::None {
                                    old_cells.push((row, ch, cell));
                                }
                            }
                        }
                        let cmd = crate::edit::FillInstrumentCommand {
                            order: selected_order,
                            old_cells,
                            instrument: self.selected_instrument as u8,
                        };
                        let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                    }
                    crate::ui::pattern_grid::ContextMenuAction::InterpolateVolume => {
                        let mut old_cells = Vec::new();
                        let mut new_cells = Vec::new();
                        for ch in min.channel..=max.channel {
                            let first_vol = arc_module.patterns[pat_idx].data[min.row][ch].volume;
                            let last_vol = arc_module.patterns[pat_idx].data[max.row][ch].volume;
                            if let (Some(fv), Some(lv)) = (first_vol, last_vol) {
                                let total = max.row - min.row;
                                for (step, row) in (min.row..=max.row).enumerate() {
                                    let old_cell = arc_module.patterns[pat_idx].data[row][ch];
                                    old_cells.push((row, ch, old_cell));
                                    let mut new_cell = old_cell;
                                    new_cell.volume = Some(crate::edit::interpolate_u8(fv, lv, step, total));
                                    new_cells.push((row, ch, new_cell));
                                }
                            }
                        }
                        if !new_cells.is_empty() {
                            let cmd = crate::edit::InterpolateCommand {
                                order: selected_order,
                                old_cells,
                                new_cells,
                            };
                            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                        }
                    }
                    crate::ui::pattern_grid::ContextMenuAction::InterpolateEffect => {
                        let mut old_cells = Vec::new();
                        let mut new_cells = Vec::new();
                        for ch in min.channel..=max.channel {
                            let first_param = crate::sequencer::effect::effect_param_value(&arc_module.patterns[pat_idx].data[min.row][ch].effect);
                            let last_param = crate::sequencer::effect::effect_param_value(&arc_module.patterns[pat_idx].data[max.row][ch].effect);
                            if let (Some(fp), Some(lp)) = (first_param, last_param) {
                                let total = max.row - min.row;
                                for (step, row) in (min.row..=max.row).enumerate() {
                                    let old_cell = arc_module.patterns[pat_idx].data[row][ch];
                                    old_cells.push((row, ch, old_cell));
                                    let new_val = crate::edit::interpolate_u8(fp, lp, step, total);
                                    let new_cell = crate::sequencer::effect::set_effect_param_value(old_cell, new_val);
                                    new_cells.push((row, ch, new_cell));
                                }
                            }
                        }
                        if !new_cells.is_empty() {
                            let cmd = crate::edit::InterpolateCommand {
                                order: selected_order,
                                old_cells,
                                new_cells,
                            };
                            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                        }
                    }
                    crate::ui::pattern_grid::ContextMenuAction::Reverse => {
                        for ch in min.channel..=max.channel {
                            let old_cells: Vec<Cell> = (min.row..=max.row)
                                .map(|r| arc_module.patterns[pat_idx].data[r][ch])
                                .collect();
                            let cmd = crate::edit::ReverseCommand {
                                order: selected_order,
                                channel: ch,
                                start_row: min.row,
                                end_row: max.row,
                                old_cells,
                            };
                            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                        }
                    }
                    crate::ui::pattern_grid::ContextMenuAction::Randomize => {
                        let mut old_cells = Vec::new();
                        let mut new_cells = Vec::new();
                        for row in min.row..=max.row {
                            for ch in min.channel..=max.channel {
                                let old_cell = arc_module.patterns[pat_idx].data[row][ch];
                                old_cells.push((row, ch, old_cell));
                                let mut new_cell = old_cell;
                                if let Note::On(key) = old_cell.note {
                                    let new_key = crate::edit::random_u8(key.saturating_sub(12).max(0), (key as u16 + 12).min(119) as u8);
                                    new_cell.note = Note::On(new_key);
                                }
                                if let Some(v) = old_cell.volume {
                                    let min_v = v.saturating_sub(16);
                                    let max_v = (v as u16 + 16).min(255) as u8;
                                    new_cell.volume = Some(crate::edit::random_u8(min_v, max_v));
                                }
                                new_cells.push((row, ch, new_cell));
                            }
                        }
                        if !new_cells.is_empty() {
                            let cmd = crate::edit::RandomizeCommand {
                                order: selected_order,
                                old_cells,
                                new_cells,
                            };
                            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                        }
                    }
                    crate::ui::pattern_grid::ContextMenuAction::SetEffect { hex } => {
                        // Set the effect on the cursor cell (or the entire
                        // selection if active). The param defaults to 0;
                        // user can fine-tune via hex typing in the param
                        // columns.
                        let effect = crate::sequencer::effect::effect_from_hex_digit(hex);
                        let mut old_cells: Vec<(usize, usize, Cell)> = Vec::new();
                        let mut new_cells: Vec<(usize, usize, Cell)> = Vec::new();
                        for row in min.row..=max.row {
                            for ch in min.channel..=max.channel {
                                let c = arc_module.patterns[pat_idx].data[row][ch];
                                old_cells.push((row, ch, c));
                                let mut c2 = c;
                                c2.effect = effect.clone();
                                new_cells.push((row, ch, c2));
                            }
                        }
                        let cmd = crate::edit::BulkSetCellsCommand {
                            order: selected_order,
                            old_cells,
                            new_cells,
                        };
                        let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                    }
                    crate::ui::pattern_grid::ContextMenuAction::SetParamEffect { command } => {
                        // Map the UI's ParamEffectCommand to a stable
                        // discriminant (0..=4) for the sequencer helper.
                        let kind: u8 = match command {
                            crate::ui::pattern_grid::ParamEffectCommand::SetSendBusParam => 0,
                            crate::ui::pattern_grid::ParamEffectCommand::SetFilterCutoff => 1,
                            crate::ui::pattern_grid::ParamEffectCommand::SetSendLevel => 2,
                            crate::ui::pattern_grid::ParamEffectCommand::SetFilterResonance => 3,
                            crate::ui::pattern_grid::ParamEffectCommand::SetFilterType => 4,
                        };
                        let effect = crate::sequencer::effect::effect_from_param_command(kind);
                        let mut old_cells: Vec<(usize, usize, Cell)> = Vec::new();
                        let mut new_cells: Vec<(usize, usize, Cell)> = Vec::new();
                        for row in min.row..=max.row {
                            for ch in min.channel..=max.channel {
                                let c = arc_module.patterns[pat_idx].data[row][ch];
                                old_cells.push((row, ch, c));
                                let mut c2 = c;
                                c2.effect = effect.clone();
                                new_cells.push((row, ch, c2));
                            }
                        }
                        let cmd = crate::edit::BulkSetCellsCommand {
                            order: selected_order,
                            old_cells,
                            new_cells,
                        };
                        let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                    }
                    crate::ui::pattern_grid::ContextMenuAction::ClearEffect => {
                        let mut old_cells: Vec<(usize, usize, Cell)> = Vec::new();
                        let mut new_cells: Vec<(usize, usize, Cell)> = Vec::new();
                        for row in min.row..=max.row {
                            for ch in min.channel..=max.channel {
                                let c = arc_module.patterns[pat_idx].data[row][ch];
                                old_cells.push((row, ch, c));
                                let mut c2 = c;
                                c2.effect = Effect::None;
                                new_cells.push((row, ch, c2));
                            }
                        }
                        let cmd = crate::edit::BulkSetCellsCommand {
                            order: selected_order,
                            old_cells,
                            new_cells,
                        };
                        let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
                    }
                    // App-level actions (Copy/Paste/Cut/SelectAll/Transpose) are
                    // handled in HtrkApp::handle_context_menu_action before this is
                    // reached. Ignore them here.
                    _ => {}
                }
            }
        }
        self.sync_module_to_audio();
    }

    pub fn skip_to_prev_pattern(&mut self) {
        let order_len = self.module.as_ref().map_or(0, |m| m.order_list.len());
        if order_len == 0 {
            return;
        }
        self.selected_order = if self.selected_order == 0 {
            order_len - 1
        } else {
            self.selected_order - 1
        };
        self.cursor.row = 0;
        self.selection = None;
        if self.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed) {
            self.send_command(crate::audio::commands::AudioCommand::PlayFrom {
                order: self.selected_order as u16,
                row: 0,
            });
        }
    }

    pub fn skip_to_next_pattern(&mut self) {
        let order_len = self.module.as_ref().map_or(0, |m| m.order_list.len());
        if order_len == 0 {
            return;
        }
        self.selected_order = if self.selected_order >= order_len - 1 {
            0
        } else {
            self.selected_order + 1
        };
        self.cursor.row = 0;
        self.selection = None;
        if self.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed) {
            self.send_command(crate::audio::commands::AudioCommand::PlayFrom {
                order: self.selected_order as u16,
                row: 0,
            });
        }
    }

    pub fn undo(&mut self) {
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let _ = self.undo_manager.undo(arc_module);
            }
        }
        self.sync_module_to_audio();
    }

    pub fn redo(&mut self) {
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let _ = self.undo_manager.redo(arc_module);
            }
        }
        self.sync_module_to_audio();
    }

    pub fn copy_channel(&mut self, channel: usize) {
        let pattern = self.current_pattern_or_default();
        let mut data = Vec::new();
        for row in 0..pattern.num_rows {
            data.push(vec![*pattern.cell(row, channel)]);
        }
        self.clipboard = Some(data);
        self.clipboard_width = 1;
    }

    pub fn clear_channel(&mut self, channel: usize) {
        let selected_order = self.selected_order;
        self.ensure_pattern_exists();
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let pat_idx = *arc_module.order_list.get(selected_order).unwrap_or(&0) as usize;
                let pattern = &arc_module.patterns[pat_idx];
                let mut old_cells = Vec::new();
                let mut new_cells = Vec::new();
                for row in 0..pattern.num_rows {
                    let old = pattern.data[row][channel];
                    if !old.is_empty() {
                        old_cells.push((row, channel, old));
                        new_cells.push((row, channel, Cell::default()));
                    }
                }
                if !old_cells.is_empty() {
                    let cmd = Box::new(BulkSetCellsCommand {
                        order: selected_order,
                        old_cells,
                        new_cells,
                    });
                    let _ = self.undo_manager.execute(cmd, arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }

    pub fn copy_column(&mut self, channel: usize, sub_column: SubColumn) {
        let pattern = self.current_pattern_or_default();
        let mut data = Vec::new();
        for row in 0..pattern.num_rows {
            let raw = *pattern.cell(row, channel);
            let cell = match sub_column {
                SubColumn::Note => {
                    let mut c = Cell::default(); c.note = raw.note; c
                }
                SubColumn::InstrumentTens | SubColumn::InstrumentOnes => {
                    let mut c = Cell::default(); c.instrument = raw.instrument; c
                }
                SubColumn::VolumeTens | SubColumn::VolumeOnes => {
                    let mut c = Cell::default(); c.volume = raw.volume; c
                }
                SubColumn::EffectType | SubColumn::EffectParamHigh | SubColumn::EffectParamLow => {
                    let mut c = Cell::default(); c.effect = raw.effect; c
                }
            };
            data.push(vec![cell]);
        }
        self.clipboard = Some(data);
        self.clipboard_width = 1;
    }

    pub fn execute_edit_command(&mut self, cmd: Box<dyn crate::edit::EditCommand>) {
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let _ = self.undo_manager.execute(cmd, arc_module);
            }
        }
        self.sync_module_to_audio();
    }

    pub fn execute_edit_commands(&mut self, cmds: Vec<Box<dyn crate::edit::EditCommand>>) {
        self.ensure_module_ownership();
        if let Some(ref mut module) = self.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                for cmd in cmds {
                    let _ = self.undo_manager.execute(cmd, arc_module);
                }
            }
        }
        self.sync_module_to_audio();
    }
}