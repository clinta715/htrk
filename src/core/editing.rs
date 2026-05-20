use std::sync::Arc;

use crate::audio::commands::AudioCommand;
use crate::edit::{SetCellCommand, UndoManager};
use crate::sequencer::module::{Module, DEFAULT_CHANNELS, MAX_CHANNELS};
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
        self.sync_to_audio();
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

    pub fn set_cursor(&mut self, cursor: CursorPosition) {
        self.cursor = cursor;
    }

    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    pub fn set_selection_anchor(&mut self, anchor: Option<CursorPosition>) {
        self.selection_anchor = anchor;
    }

    pub fn set_selected_order(&mut self, order: usize) {
        self.selected_order = order;
    }

    pub fn set_selected_sample(&mut self, sample: usize) {
        self.selected_sample = sample;
    }

    pub fn set_selected_instrument(&mut self, instrument: usize) {
        self.selected_instrument = instrument;
    }

    pub fn set_last_entered_cell(&mut self, cell: Option<Cell>) {
        self.last_entered_cell = cell;
    }

    pub fn last_entered_cell(&self) -> Option<Cell> {
        self.last_entered_cell
    }

    pub fn set_muted_channel(&mut self, channel: usize, muted: bool) {
        if channel < self.muted_channels.len() {
            self.muted_channels[channel] = muted;
        }
    }

    pub fn set_solo_channel(&mut self, channel: usize, solo: bool) {
        if channel < self.solo_channels.len() {
            self.solo_channels[channel] = solo;
        }
    }
}