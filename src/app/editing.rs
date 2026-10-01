use super::*;

impl HtrkApp {
    #[allow(dead_code)]
    pub(super) fn current_pattern_mut(&mut self) -> Option<&mut crate::sequencer::Pattern> {
        self.core.current_pattern_mut()
    }

    pub(crate) fn change_selected_sample(&mut self, delta: i32) {
        let len = self.core.module.as_ref().map_or(0, |m| m.samples.len());
        if len <= 1 {
            return;
        }
        let next = (self.core.selected_sample as i32 + delta).clamp(1, (len - 1) as i32);
        self.core.selected_sample = next as usize;
        self.sample_editor.selection = None;
    }

    /// Load a WAV file from the sample library into the currently
    /// selected sample slot and map the current instrument to use it.
    pub(crate) fn import_sample_from_library(&mut self, path: &str) {
        use crate::formats::wav::import_wav;
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                self.sample_library_state.status_message = Some(format!("Read error: {e}"));
                return;
            }
        };
        match import_wav(&data) {
            Ok(mut sample) => {
                if let Some(name) = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                {
                    sample.name = name.to_string();
                }
                let sample_idx = self.core.selected_sample;
                self.core.import_wav_to_sample(sample_idx, sample);
                self.core.with_module_mut(|arc_module, core| {
                    let inst_idx = core.selected_instrument;
                    if inst_idx > 0 && inst_idx < arc_module.instruments.len() {
                        for i in 0..120 {
                            arc_module.instruments[inst_idx].sample_map[i] = sample_idx as u8;
                        }
                    }
                });
                self.sample_library_state.status_message = Some("Imported".to_string());
            }
            Err(e) => {
                self.sample_library_state.status_message = Some(format!("Failed: {e}"));
            }
        }
    }

    pub(crate) fn change_selected_instrument(&mut self, delta: i32) {
        let len = self.core.module.as_ref().map_or(0, |m| m.instruments.len());
        if len <= 1 {
            return;
        }
        let next = (self.core.selected_instrument as i32 + delta).clamp(1, (len - 1) as i32);
        self.core.selected_instrument = next as usize;
    }

    pub(crate) fn set_cell_at_cursor(&mut self, new_cell: Cell) {
        self.core.set_cell_at_cursor(
            new_cell,
            &self.multichannel_channels,
            self.multichannel_enabled,
        );
    }

    pub(crate) fn advance_cursor_down(&mut self, step: usize) {
        let max_row = self.core.current_pattern_or_default().num_rows.max(1);
        self.core.cursor.row = (self.core.cursor.row + step).min(max_row - 1);
        self.ensure_cursor_visible();
    }

    pub(crate) fn advance_cursor_up(&mut self, step: usize) {
        self.core.cursor.row = self.core.cursor.row.saturating_sub(step);
        self.ensure_cursor_visible();
    }

    pub(crate) fn ensure_cursor_visible(&mut self) {
        if self.core.cursor.row < self.pattern_view.scroll_row {
            self.pattern_view.scroll_row = self.core.cursor.row;
        }
        if self.core.cursor.row
            >= self.pattern_view.scroll_row + self.pattern_view.last_visible_rows
        {
            self.pattern_view.scroll_row =
                self.core.cursor.row - self.pattern_view.last_visible_rows + 1;
        }

        if self.core.cursor.channel < self.pattern_view.scroll_channel {
            self.pattern_view.scroll_channel = self.core.cursor.channel;
        }
        if self.core.cursor.channel
            >= self.pattern_view.scroll_channel + self.pattern_view.last_visible_channels
        {
            self.pattern_view.scroll_channel =
                self.core.cursor.channel - self.pattern_view.last_visible_channels + 1;
        }
    }

    pub(crate) fn move_cursor_right(&mut self) {
        let num_ch = self.core.num_channels();
        if self.core.cursor.channel < num_ch - 1 {
            self.core.cursor.channel += 1;
            self.core.cursor.sub_column = SubColumn::Note;
        }
        self.ensure_cursor_visible();
    }

    pub(crate) fn move_cursor_left(&mut self) {
        if self.core.cursor.channel > 0 {
            self.core.cursor.channel -= 1;
            self.core.cursor.sub_column = SubColumn::Note;
        }
        self.ensure_cursor_visible();
    }

    pub(crate) fn first_visible_sub_column(
        col_vis: crate::ui::pattern_grid::ColumnVisibility,
    ) -> crate::ui::pattern_grid::SubColumn {
        use crate::ui::pattern_grid::SubColumn;
        if col_vis.note {
            return SubColumn::Note;
        }
        if col_vis.instrument {
            return SubColumn::InstrumentTens;
        }
        if col_vis.volume {
            return SubColumn::VolumeTens;
        }
        if col_vis.effect {
            return SubColumn::EffectType;
        }
        SubColumn::Note
    }

    pub(crate) fn last_visible_sub_column(
        col_vis: crate::ui::pattern_grid::ColumnVisibility,
    ) -> crate::ui::pattern_grid::SubColumn {
        use crate::ui::pattern_grid::SubColumn;
        if col_vis.effect {
            return SubColumn::EffectParamLow;
        }
        if col_vis.volume {
            return SubColumn::VolumeOnes;
        }
        if col_vis.instrument {
            return SubColumn::InstrumentOnes;
        }
        if col_vis.note {
            return SubColumn::Note;
        }
        SubColumn::Note
    }

    pub(crate) fn step_sub_column_forward(&mut self) {
        let col_vis = self.config.get_col_vis();
        if let Some(next) = self.core.cursor.sub_column.next_visible(col_vis) {
            self.core.cursor.sub_column = next;
        } else {
            self.core.cursor.sub_column = Self::first_visible_sub_column(col_vis);
            self.advance_cursor_down(1);
        }
    }

    pub(crate) fn step_sub_column_backward(&mut self) {
        let col_vis = self.config.get_col_vis();
        if let Some(prev) = self.core.cursor.sub_column.prev_visible(col_vis) {
            self.core.cursor.sub_column = prev;
        } else {
            self.core.cursor.sub_column = Self::last_visible_sub_column(col_vis);
            self.advance_cursor_up(1);
        }
    }

    pub(crate) fn cycle_spacing_mode(&mut self) {
        use crate::app_config::SpacingMode;
        let modes = [
            SpacingMode::Compact,
            SpacingMode::Normal,
            SpacingMode::Wide,
            SpacingMode::ExtraWide,
        ];
        let current = self.config.get_spacing_mode();
        let idx = modes.iter().position(|&m| m == current).unwrap_or(1);
        let next = modes[(idx + 1) % modes.len()];
        self.config.set_spacing_mode(next);
    }

    pub(crate) fn extend_selection_down(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.advance_cursor_down(1);
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_up(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.advance_cursor_up(1);
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_right(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.move_cursor_right();
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn extend_selection_left(&mut self) {
        if self.core.selection.is_none() {
            self.core.selection_anchor = Some(self.core.cursor);
        }
        self.move_cursor_left();
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn mark_block_begin(&mut self) {
        let cur = self.core.cursor;
        self.core.selection_anchor = Some(cur);
        self.core.selection = Some(Selection {
            start: cur,
            end: cur,
        });
    }

    pub(crate) fn mark_block_end(&mut self) {
        if let Some(anchor) = self.core.selection_anchor {
            self.core.selection = Some(Selection {
                start: anchor,
                end: self.core.cursor,
            });
        }
    }

    pub(crate) fn cut_selection(&mut self) {
        self.core.copy_selection();
        self.core.delete_selection();
    }

    pub(crate) fn handle_context_menu_action(
        &mut self,
        action: crate::ui::pattern_grid::ContextMenuAction,
    ) {
        if !self.edit_mode {
            return;
        }
        use crate::ui::pattern_grid::ContextMenuAction;
        match action {
            ContextMenuAction::CopySelection => self.core.copy_selection(),
            ContextMenuAction::PasteClipboard => self.core.paste_at_cursor(),
            ContextMenuAction::CutSelection => self.cut_selection(),
            ContextMenuAction::SelectAll => self.core.select_all(),
            ContextMenuAction::TransposeUp => self.core.transpose_selection(1),
            ContextMenuAction::TransposeDown => self.core.transpose_selection(-1),
            other => self.core.handle_context_menu_action(other),
        }
    }

    pub(super) fn handle_automation_interaction(
        &mut self,
        interaction: crate::ui::pattern_grid::AutomationInteraction,
    ) {
        self.core.handle_automation_interaction(interaction);
    }

    #[allow(dead_code)] // exposed for future automation UI / MCP tools
    pub(crate) fn enter_automation_hex(&mut self, channel: usize, row: usize, digit: u8) {
        self.core.enter_automation_hex(channel, row, digit);
    }

    pub(crate) fn delete_automation_point(&mut self, channel: usize, row: usize) {
        self.core.delete_automation_point(channel, row);
    }

    pub(crate) fn open_file_dialog(&mut self) {
        self.file_browser.open(
            BrowserMode::Modules,
            crate::ui::file_browser::DialogMode::Open,
            &mut self.config,
        );
    }

    pub(crate) fn save_as_dialog(&mut self) {
        self.browser_purpose = BrowserPurpose::SaveProject;
        self.file_browser.open(
            BrowserMode::Projects,
            crate::ui::file_browser::DialogMode::Save,
            &mut self.config,
        );
    }

    pub(crate) fn copy_track(&mut self) {
        self.core.copy_channel(self.core.cursor.channel);
    }

    pub(crate) fn cut_track(&mut self) {
        let ch = self.core.cursor.channel;
        self.core.copy_channel(ch);
        self.core.clear_channel(ch);
    }

    pub(crate) fn delete_track(&mut self) {
        self.core.clear_channel(self.core.cursor.channel);
    }

    pub(crate) fn copy_column(&mut self) {
        let ch = self.core.cursor.channel;
        let sc = self.core.cursor.sub_column;
        self.core.copy_column(ch, sc);
    }

    pub(crate) fn cut_column(&mut self) {
        let ch = self.core.cursor.channel;
        let sc = self.core.cursor.sub_column;
        self.core.copy_column(ch, sc);
        self.core.clear_channel(ch);
    }

    pub(crate) fn preview_browser_sample(&mut self, note_key: u8) -> bool {
        use crate::ui::file_browser::BrowserMode;
        if !self.file_browser.show || self.file_browser.mode != BrowserMode::Samples {
            return false;
        }
        let entry = match self.file_browser.selected_entry() {
            Some(e) => e.clone(),
            None => return false,
        };
        if entry.is_dir {
            return false;
        }
        let audio_exts = ["wav", "mp3", "ogg", "flac", "it", "xm", "s3m", "mod", "669"];
        if !audio_exts.contains(&entry.extension.as_str()) {
            return false;
        }
        let (data, sample_rate) = match self.file_browser.get_preview_data(&entry.path) {
            Some(d) => d,
            None => return false,
        };
        self.core
            .send_command(crate::audio::commands::AudioCommand::PreviewBuffer {
                data,
                sample_rate,
                note_key,
                volume: 0.75,
                panning: 0.5,
            });
        true
    }

    pub fn any_dialog_open(&self) -> bool {
        // Derived from the dialog registry (src/app/dialog_registry.rs) —
        // the single list of dialogs. A new dialog registered there is
        // automatically gated; see AGENTS.md §11.
        self.topmost_dialog().is_some()
    }
}
