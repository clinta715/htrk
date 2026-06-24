use std::sync::Arc;

use eframe::egui;

use crate::app::HtrkApp;
use crate::app::AppView;
use crate::edit::InsertRowCommand;
use crate::edit::DeleteRowCommand;
use crate::sequencer::automation::InterpolationMode;
use crate::sequencer::effect::Effect;
use crate::ui::sample_editor::SampleEditEvent;
use crate::sequencer::pattern::Cell;
use crate::sequencer::Note;
use crate::sequencer::player::PlayMode;
use crate::ui::file_browser::BrowserMode;
use crate::ui::pattern_grid::{ContextMenuAction, SubColumn};

const NOTE_KEYS_LOWER: [(egui::Key, u8); 12] = [
    (egui::Key::Z, 0),
    (egui::Key::S, 1),
    (egui::Key::X, 2),
    (egui::Key::D, 3),
    (egui::Key::C, 4),
    (egui::Key::V, 5),
    (egui::Key::G, 6),
    (egui::Key::B, 7),
    (egui::Key::H, 8),
    (egui::Key::N, 9),
    (egui::Key::J, 10),
    (egui::Key::M, 11),
];

const NOTE_KEYS_UPPER: [(egui::Key, u8); 12] = [
    (egui::Key::Q, 0),
    (egui::Key::Num2, 1),
    (egui::Key::W, 2),
    (egui::Key::Num3, 3),
    (egui::Key::E, 4),
    (egui::Key::R, 5),
    (egui::Key::Num5, 6),
    (egui::Key::T, 7),
    (egui::Key::Num6, 8),
    (egui::Key::Y, 9),
    (egui::Key::Num7, 10),
    (egui::Key::U, 11),
];

pub(crate) fn handle_keyboard_input(app: &mut HtrkApp, ctx: &egui::Context) {
    let is_pattern = app.current_view == AppView::Pattern;
    let is_sample = app.current_view == AppView::Sample;
    let modifiers = ctx.input(|i| i.modifiers);
    let has_focus = ctx.memory(|m| m.focused().is_some());
    let any_dialog_open = app.file_browser.show
        || app.settings_state.open
        || app.wav_export_state.open
        || app.sample_export_dialog.is_some()
        || app.show_about
        || app.show_shortcuts
        || app.show_exit_confirm
        || app.show_phrase_generator
        || app.slice_dialog_open
        || app.sendfx_panel.plugin_browser_open_for.is_some();

    // Text events: processed unconditionally so note preview works even during dialog input.
    // When a widget has focus or a dialog is open, only play audio; skip cell editing.
    ctx.input(|i| {
        for event in &i.events {
            if let egui::Event::Text(text) = event {
                for ch in text.chars() {
                    if has_focus || any_dialog_open {
                        note_key_preview_only(app, ch);
                    } else {
                        handle_text_input(app, ch);
                    }
                }
            }
        }
    });

    // Tab interception: in the pattern editor, Tab always changes columns — it must never
    // escape to egui's focus-navigation. Handle it before the focus gate.
    //
    // Two interrelated egui bugs to work around:
    //
    // 1. Focus::begin_pass() sets self.focus_direction = Next/Previous from raw Tab events
    //    *before* our handler runs. We surrender focus, but end_pass() then uses the stale
    //    focus_direction to move focus to another widget. Fix: call move_focus(None) after
    //    surrendering.
    //
    // 2. consume_key() uses matches_logically() which ignores extra modifiers, so
    //    Shift+Tab matches the plain-Tab branch. Fix: inspect raw events directly,
    //    matching !modifiers.any() vs modifiers.shift_only() (same semantics begin_pass
    //    uses internally).
    if is_pattern && !any_dialog_open {
        let mut tab_pressed = false;
        let mut shift_pressed = false;
        ctx.input_mut(|i| {
            let mut tab_idx = None;
            let mut shift_tab_idx = None;
            for (idx, event) in i.events.iter().enumerate() {
                if let egui::Event::Key { key: egui::Key::Tab, pressed: true, modifiers, .. } = event {
                    if !modifiers.any() {
                        tab_idx = Some(idx);
                        break;
                    } else if modifiers.shift_only() {
                        shift_tab_idx = Some(idx);
                        break;
                    }
                }
            }
            if let Some(idx) = tab_idx {
                tab_pressed = true;
                shift_pressed = false;
                i.events.remove(idx);
            } else if let Some(idx) = shift_tab_idx {
                tab_pressed = true;
                shift_pressed = true;
                i.events.remove(idx);
            }
        });
        if tab_pressed {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
                m.move_focus(egui::FocusDirection::None);
            });
            app.core.selection = None;
            if shift_pressed {
                app.core.cursor.channel = app.core.cursor.channel.saturating_sub(1);
            } else {
                app.core.cursor.channel += 1;
                app.core.cursor.channel = app.core.cursor.channel.min(app.core.num_channels_checked() - 1);
            }
            app.ensure_cursor_visible();
        }
    }

    // Focus gate: if a widget has focus, skip all key events.
    if has_focus {
        return;
    }

    if modifiers.ctrl && !modifiers.shift {
        let mut handled = false;
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Key { key, pressed: true, .. } = event {
                    match key {
                        egui::Key::Z if app.edit_mode && !any_dialog_open => {
                            app.core.ensure_module_ownership();
                            if let Some(ref mut module) = app.core.module {
                                if let Some(arc_module) = Arc::get_mut(module) {
                                    let _ = app.core.undo_manager.undo(arc_module);
                                }
                            }
                            app.core.sync_module_to_audio();
                            handled = true;
                        }
                        egui::Key::Y if app.edit_mode && !any_dialog_open => {
                            app.core.ensure_module_ownership();
                            if let Some(ref mut module) = app.core.module {
                                if let Some(arc_module) = Arc::get_mut(module) {
                                    let _ = app.core.undo_manager.redo(arc_module);
                                }
                            }
                            app.core.sync_module_to_audio();
                            handled = true;
                        }
                        egui::Key::C if is_pattern && !any_dialog_open => {
                            app.core.copy_selection();
                            handled = true;
                        }
                        egui::Key::C if is_sample && !any_dialog_open => {
                            if let Some((s, e)) = app.sample_editor.selection {
                                let start = s.min(e);
                                let end = s.max(e);
                                crate::actions::sample_edit::handle_sample_edit(app, SampleEditEvent::CopyRegion(start, end));
                            }
                            handled = true;
                        }
                        egui::Key::X if app.edit_mode && is_pattern && !any_dialog_open => {
                            app.core.copy_selection();
                            app.core.delete_selection();
                            handled = true;
                        }
                        egui::Key::X if is_sample && !any_dialog_open => {
                            if let Some((s, e)) = app.sample_editor.selection {
                                let start = s.min(e);
                                let end = s.max(e);
                                crate::actions::sample_edit::handle_sample_edit(app, SampleEditEvent::CutRegion(start, end));
                            }
                            handled = true;
                        }
                        egui::Key::V if app.edit_mode && is_pattern && !any_dialog_open => {
                            app.core.paste_at_cursor();
                            handled = true;
                        }
                        egui::Key::V if is_sample && !any_dialog_open => {
                            if app.sample_editor.clipboard.is_some() {
                                if let Some(pos) = app.sample_editor.cursor_pos {
                                    crate::actions::sample_edit::handle_sample_edit(app, SampleEditEvent::PasteRegion(pos));
                                }
                            }
                            handled = true;
                        }
                        egui::Key::A if is_pattern && !any_dialog_open => {
                            app.core.select_all();
                            handled = true;
                        }
                        egui::Key::A if is_sample && !any_dialog_open => {
                            if let Some(ref module) = app.core.module {
                                let idx = app.core.selected_sample;
                                if let Some(sample) = module.samples.get(idx) {
                                    app.sample_editor.selection = Some((0, sample.data.len().saturating_sub(1)));
                                }
                            }
                            handled = true;
                        }
                        egui::Key::N => {
                            app.new_song();
                            handled = true;
                        }
                        egui::Key::O => {
                            match app.current_view {
                                AppView::Sample => app.file_browser.open(BrowserMode::Samples, crate::ui::file_browser::DialogMode::Open, &mut app.config),
                                AppView::Instrument => app.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut app.config),
                                _ => app.open_file_dialog(),
                            }
                            handled = true;
                        }
                        egui::Key::I => {
                            app.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut app.config);
                            handled = true;
                        }
                        egui::Key::S => {
                            crate::actions::save_current_file(app);
                            handled = true;
                        }
                        egui::Key::Q => {
                            if app.config.confirm_on_exit && app.core.module_dirty() {
                                app.show_exit_confirm = true;
                            } else {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            handled = true;
                        }
                        egui::Key::ArrowRight if is_pattern && !any_dialog_open => {
                            app.step_sub_column_forward();
                            handled = true;
                        }
                        egui::Key::ArrowLeft if is_pattern && !any_dialog_open => {
                            app.step_sub_column_backward();
                            handled = true;
                        }
                        egui::Key::Num1 => {
                            let mut col_vis = app.config.get_col_vis();
                            col_vis.note = !col_vis.note;
                            app.config.set_col_vis(col_vis);
                            app.col_vis = app.config.get_col_vis();
                            app.config.save();
                            handled = true;
                        }
                        egui::Key::Num2 => {
                            let mut col_vis = app.config.get_col_vis();
                            col_vis.instrument = !col_vis.instrument;
                            app.config.set_col_vis(col_vis);
                            app.col_vis = app.config.get_col_vis();
                            app.config.save();
                            handled = true;
                        }
                        egui::Key::Num3 => {
                            let mut col_vis = app.config.get_col_vis();
                            col_vis.volume = !col_vis.volume;
                            app.config.set_col_vis(col_vis);
                            app.col_vis = app.config.get_col_vis();
                            app.config.save();
                            handled = true;
                        }
                        egui::Key::Num4 => {
                            let mut col_vis = app.config.get_col_vis();
                            col_vis.effect = !col_vis.effect;
                            app.config.set_col_vis(col_vis);
                            app.col_vis = app.config.get_col_vis();
                            app.config.save();
                            handled = true;
                        }
                        _ => {}
                    }
                }
            }
        });
        if handled {
            return;
        }
    }

    if modifiers.ctrl && modifiers.shift {
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Key { key, pressed: true, .. } = event {
                    match key {
                        egui::Key::S => app.save_as_dialog(),
                        egui::Key::I => app.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut app.config),
                        egui::Key::ArrowUp => {
                            if app.current_octave < 9 { app.current_octave += 1; }
                        }
                        egui::Key::ArrowDown => {
                            if app.current_octave > 0 { app.current_octave -= 1; }
                        }
                        egui::Key::ArrowLeft => {
                            app.change_selected_sample(-1);
                        }
                        egui::Key::ArrowRight => {
                            app.change_selected_sample(1);
                        }
                        egui::Key::Space if !any_dialog_open => {
                            app.cycle_spacing_mode();
                        }
                        egui::Key::L => {
                            app.config.toggle_sample_length_bg();
                        }
                        _ => {}
                    }
                }
            }
        });

        if app.current_view == AppView::Automation {
            if let Some(tid) = app.automation_editor.state.selected_track_id {
                let mode = ctx.input(|i| {
                    for event in &i.events {
                        if let egui::Event::Key { key, pressed: true, .. } = event {
                            match key {
                                egui::Key::Num5 => return Some(InterpolationMode::Hold),
                                egui::Key::Num6 => return Some(InterpolationMode::Linear),
                                egui::Key::Num7 => return Some(InterpolationMode::Smooth),
                                egui::Key::Num8 => return Some(InterpolationMode::Exponential),
                                _ => {}
                            }
                        }
                    }
                    None
                });
                if let Some(mode) = mode {
                    app.core.ensure_module_ownership();
                    if let Some(ref mut module) = app.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == tid) {
                                t.default_interp = mode;
                                app.core.sync_module_to_audio();
                            }
                        }
                    }
                }
            }
        }

        return;
    }

    if modifiers.ctrl {
        return;
    }

    ctx.input(|i| {
        for event in &i.events {
            if let egui::Event::Key { key, pressed: true, .. } = event {
                match key {
                        egui::Key::ArrowDown => {
                            if !any_dialog_open && is_pattern {
                                if modifiers.ctrl {
                                    if app.current_octave > 0 {
                                        app.current_octave -= 1;
                                    }
                                } else if modifiers.shift {
                                    app.extend_selection_down();
                                } else if modifiers.alt && app.edit_mode {
                                    app.core.transpose_selection(-1);
                                } else {
                                    app.core.selection = None;
                                    app.advance_cursor_down(1);
                                }
                            }
                        }
                        egui::Key::ArrowUp => {
                            if !any_dialog_open && is_pattern {
                                if modifiers.ctrl {
                                    if app.current_octave < 9 {
                                        app.current_octave += 1;
                                    }
                                } else if modifiers.shift {
                                    app.extend_selection_up();
                                } else if modifiers.alt && app.edit_mode {
                                    app.core.transpose_selection(1);
                                } else {
                                    app.core.selection = None;
                                    app.advance_cursor_up(1);
                                }
                            }
                        }
                    egui::Key::ArrowRight => {
                        if !any_dialog_open && is_pattern {
                            if modifiers.alt {
                                app.core.selection = None;
                                let num_ch = app.core.num_channels();
                                if app.core.cursor.channel < num_ch - 1 {
                                    app.core.cursor.channel += 1;
                                    app.core.cursor.sub_column = SubColumn::Note;
                                    app.ensure_cursor_visible();
                                }
                            } else if modifiers.shift {
                                app.extend_selection_right();
                            } else {
                                app.core.selection = None;
                                app.move_cursor_right();
                            }
                        }
                    }
                    egui::Key::ArrowLeft => {
                        if !any_dialog_open && is_pattern {
                            if modifiers.alt {
                                app.core.selection = None;
                                if app.core.cursor.channel > 0 {
                                    app.core.cursor.channel -= 1;
                                    app.core.cursor.sub_column = SubColumn::Note;
                                    app.ensure_cursor_visible();
                                }
                            } else if modifiers.shift {
                                app.extend_selection_left();
                            } else {
                                app.core.selection = None;
                                app.move_cursor_left();
                            }
                        }
                    }
                    egui::Key::M if modifiers.alt && !any_dialog_open => {
                        app.core.toggle_mute(app.core.cursor.channel);
                    }
                    egui::Key::S if modifiers.alt && !any_dialog_open => {
                        app.core.toggle_solo(app.core.cursor.channel);
                    }
                    egui::Key::N if modifiers.alt && !any_dialog_open => {
                        let ch = app.core.cursor.channel;
                        if ch < app.multichannel_channels.len() {
                            app.multichannel_channels[ch] = !app.multichannel_channels[ch];
                            app.multichannel_enabled = app.multichannel_channels.iter().any(|&v| v);
                        }
                    }
                    egui::Key::PageUp if is_pattern && !any_dialog_open => {
                        app.core.selection = None;
                        app.advance_cursor_up(16);
                    }
                    egui::Key::PageDown if is_pattern && !any_dialog_open => {
                        app.core.selection = None;
                        app.advance_cursor_down(16);
                    }
                    egui::Key::Home if is_pattern && !any_dialog_open => {
                        app.core.selection = None;
                        if app.core.cursor.row == 0 && app.core.cursor.channel > 0 {
                            app.core.cursor.channel = 0;
                        } else {
                            app.core.cursor.row = 0;
                        }
                        app.ensure_cursor_visible();
                    }
                    egui::Key::End if is_pattern && !any_dialog_open => {
                        app.core.selection = None;
                        app.core.cursor.row = app.core.current_pattern_or_default().num_rows - 1;
                        app.ensure_cursor_visible();
                    }
                    egui::Key::Backspace if app.edit_mode && is_pattern && !any_dialog_open => {
                        app.core.clear_cell_at_cursor();
                    }
                    egui::Key::Delete if modifiers.shift && app.edit_mode && is_pattern && !any_dialog_open => {
                        app.delete_track();
                    }
                    egui::Key::Delete if app.edit_mode && is_pattern && !any_dialog_open => {
                        if modifiers.alt {
                            delete_row(app);
                        } else {
                            delete_cell_or_automation(app);
                        }
                    }
                    egui::Key::Delete if is_sample && !any_dialog_open => {
                        if let Some((s, e)) = app.sample_editor.selection {
                            let start = s.min(e);
                            let end = s.max(e);
                            crate::actions::sample_edit::handle_sample_edit(app, SampleEditEvent::SilenceRegion(start, end));
                        }
                    }
                    egui::Key::Insert if app.edit_mode && is_pattern && !any_dialog_open => {
                        let selected_order = app.core.selected_order;
                        let row = app.core.cursor.row;
                        app.core.ensure_pattern_exists();
                        app.core.ensure_module_ownership();
                        if let Some(ref mut module) = app.core.module {
                            let pat_idx = *module.order_list.get(selected_order).unwrap_or(&0) as usize;
                            if let Some(arc_module) = Arc::get_mut(module) {
                                let cmd = Box::new(InsertRowCommand {
                                    pattern_index: pat_idx,
                                    row,
                                    _channel: None,
                                });
                                let _ = app.core.undo_manager.execute(cmd, arc_module);
                            }
                        }
                        app.core.sync_module_to_audio();
                    }
                    egui::Key::Space => {
                        if !any_dialog_open {
                            if app.core.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed) {
                                app.core.send_command(crate::audio::commands::AudioCommand::Stop);
                            } else if app.edit_mode && is_pattern {
                                if let Some(last_cell) = app.core.last_entered_cell.clone() {
                                    app.set_cell_at_cursor(last_cell);
                                    app.advance_cursor_down(app.cursor_skip as usize);
                                }
                            }
                        }
                    }
                    egui::Key::F1 => {
                        app.show_shortcuts = !app.show_shortcuts;
                    }
                    egui::Key::F2 if !any_dialog_open => {
                        app.current_view = AppView::Pattern;
                    }
                    egui::Key::F3 if !any_dialog_open => {
                        app.current_view = AppView::Sample;
                    }
                    egui::Key::F4 if !any_dialog_open => {
                        app.current_view = AppView::Instrument;
                    }
                    egui::Key::F6 if !any_dialog_open => {
                        app.current_view = AppView::SendFx;
                    }
                    egui::Key::F7 if !any_dialog_open => {
                        app.current_view = AppView::Automation;
                    }
                    egui::Key::F5 if modifiers.shift => {
                        app.current_view = AppView::Playback;
                        app.core.send_command(crate::audio::commands::AudioCommand::Play);
                    }
                    egui::Key::F5 => {
                        app.core.send_command(crate::audio::commands::AudioCommand::Play);
                    }
                    egui::Key::F6 => {
                        app.core.send_command(crate::audio::commands::AudioCommand::SetPlayMode(PlayMode::Pattern));
                        app.core.send_command(crate::audio::commands::AudioCommand::Play);
                    }
                    egui::Key::F7 => {
                        app.core.send_command(crate::audio::commands::AudioCommand::SetPlayMode(PlayMode::Order));
                    }
                    egui::Key::F8 => {
                        app.core.send_command(crate::audio::commands::AudioCommand::Stop);
                    }
                    egui::Key::F9 => {
                        let order = app.core.playback_state.current_order.load(std::sync::atomic::Ordering::Relaxed);
                        let row = app.core.playback_state.current_row.load(std::sync::atomic::Ordering::Relaxed);
                        app.core.send_command(crate::audio::commands::AudioCommand::PlayFrom { order, row });
                    }
                    egui::Key::F10 => {
                        let should_open = !app.settings_state.open;
                        if should_open {
                            app.settings_state = crate::ui::settings_window::SettingsState::from_config(&app.config);
                            app.settings_state.open = true;
                        } else {
                            app.settings_state.open = false;
                        }
                    }
                    egui::Key::Escape => {
                        if any_dialog_open {
                            if app.show_exit_confirm { app.show_exit_confirm = false; }
                            else if app.show_shortcuts { app.show_shortcuts = false; }
                            else if app.show_about { app.show_about = false; }
                            else if app.settings_state.open { app.settings_state.open = false; }
                            else if app.show_phrase_generator { app.show_phrase_generator = false; }
                            else if app.slice_dialog_open { app.slice_dialog_open = false; }
                            else if app.file_browser.show { app.file_browser.show = false; }
                            else if app.wav_export_state.open { app.wav_export_state.open = false; }
                            else if app.sample_export_dialog.is_some() { app.sample_export_dialog = None; }
                        } else {
                            app.edit_mode = !app.edit_mode;
                        }
                    }
                    egui::Key::OpenBracket if !any_dialog_open => {
                        if app.current_octave > 0 { app.current_octave -= 1; }
                    }
                    egui::Key::CloseBracket if !any_dialog_open => {
                        if app.current_octave < 9 { app.current_octave += 1; }
                    }
                    egui::Key::Num0 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 0; }
                    egui::Key::Num1 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 1; }
                    egui::Key::Num2 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 2; }
                    egui::Key::Num3 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 3; }
                    egui::Key::Num4 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 4; }
                    egui::Key::Num5 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 5; }
                    egui::Key::Num6 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 6; }
                    egui::Key::Num7 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 7; }
                    egui::Key::Num8 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 8; }
                    egui::Key::Num9 if modifiers.alt && !any_dialog_open => { app.cursor_skip = 9; }
                    egui::Key::Minus if is_pattern && !modifiers.alt && !any_dialog_open => { app.core.skip_to_prev_pattern(); }
                    egui::Key::Equals if is_pattern && !modifiers.alt && !any_dialog_open => { app.core.skip_to_next_pattern(); }
                    egui::Key::Plus if is_pattern && !any_dialog_open => { app.core.skip_to_next_pattern(); }
                    egui::Key::C if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => { app.core.copy_selection(); }
                    egui::Key::P if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => { app.core.paste_at_cursor(); }
                    egui::Key::V if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => { app.core.paste_at_cursor(); }
                    egui::Key::X if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => { app.cut_selection(); }
                    egui::Key::B if modifiers.alt && is_pattern && !any_dialog_open => { app.mark_block_begin(); }
                    egui::Key::E if modifiers.alt && is_pattern && !any_dialog_open => { app.mark_block_end(); }
                    egui::Key::L if modifiers.alt && is_pattern && !any_dialog_open => {
                        let now = std::time::Instant::now();
                        let within = app.alt_l_last.map_or(false, |t| now.duration_since(t) < std::time::Duration::from_millis(600));
                        app.alt_l_count = if within { 2 } else { 1 };
                        app.alt_l_last = Some(now);
                        match app.alt_l_count {
                            2 => app.core.select_all(),
                            _ => app.core.select_column(),
                        }
                    }
                    egui::Key::Z if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => {
                        if app.core.selection.is_some() {
                            app.handle_context_menu_action(ContextMenuAction::Reverse);
                        }
                    }
                    egui::Key::F if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => {
                        if app.core.selection.is_some() {
                            app.handle_context_menu_action(ContextMenuAction::FillInstrument);
                        }
                    }
                    egui::Key::I if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => {
                        if app.core.selection.is_some() {
                            app.handle_context_menu_action(ContextMenuAction::InterpolateVolume);
                        }
                    }
                    egui::Key::K if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => {
                        if app.core.selection.is_some() {
                            app.handle_context_menu_action(ContextMenuAction::InterpolateEffect);
                        }
                    }
                    egui::Key::R if modifiers.alt && app.edit_mode && is_pattern && !any_dialog_open => {
                        if app.core.selection.is_some() {
                            app.handle_context_menu_action(ContextMenuAction::Randomize);
                        }
                    }
                    _ => {}
                }
            }
        }
    });
}

/// Preview-only note playback (no cell editing). Called when a widget has focus
/// so the user can still hear notes while typing into a TextEdit.
fn note_key_preview_only(app: &mut HtrkApp, ch: char) {
    let up = ch.to_ascii_uppercase();
    let note_key = NOTE_KEYS_LOWER
        .iter()
        .find_map(|(key, tone)| {
            let kc = key.name();
            (kc.len() == 1 && kc.chars().next() == Some(up))
                .then(|| app.current_octave as u8 * 12 + tone)
        })
        .or_else(|| {
            NOTE_KEYS_UPPER.iter().find_map(|(key, tone)| {
                let kc = key.name();
                (kc.len() == 1 && kc.chars().next() == Some(up))
                    .then(|| (app.current_octave as u8 + 1) * 12 + tone)
            })
        });
    if let Some(nk) = note_key {
        if !app.preview_browser_sample(nk) {
            preview_note(app, nk);
        }
    }
}

fn delete_row(app: &mut HtrkApp) {
    let selected_order = app.core.selected_order;
    let row = app.core.cursor.row;
    app.core.ensure_pattern_exists();
    let pattern = app.core.current_pattern_or_default();
    let can_delete = pattern.num_rows > 1;
    if !can_delete {
        return;
    }
    let deleted_data: Vec<Cell> = pattern.data[row].to_vec();
    let pat_idx = app.core.module.as_ref()
        .and_then(|m| m.order_list.get(selected_order).copied())
        .unwrap_or(0) as usize;
    app.core.ensure_module_ownership();
    if let Some(ref mut module) = app.core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let cmd = Box::new(DeleteRowCommand {
                pattern_index: pat_idx,
                row,
                _channel: None,
                deleted_data,
            });
            let _ = app.core.undo_manager.execute(cmd, arc_module);
        }
    }
    app.core.sync_module_to_audio();
}

fn delete_cell_or_automation(app: &mut HtrkApp) {
    let auto_target = app.core.automation_targets.get(app.core.cursor.channel).copied().flatten();
    if auto_target.is_some()
        && matches!(app.core.cursor.sub_column,
            SubColumn::EffectType | SubColumn::EffectParamHigh | SubColumn::EffectParamLow)
    {
        app.delete_automation_point(app.core.cursor.channel, app.core.cursor.row);
        app.advance_cursor_down(1);
    } else {
        app.core.clear_cell_at_cursor();
        app.advance_cursor_down(1);
    }
}

fn is_value_char(sub: SubColumn, up: char) -> bool {
    if sub.accepts_decimal() {
        up.is_ascii_digit()
    } else if sub == SubColumn::EffectType {
        up.is_ascii_hexdigit() || matches!(up, 'P' | 'Z' | 'S' | 'R' | 'X')
    } else if sub.accepts_hex() {
        up.is_ascii_hexdigit()
    } else {
        false
    }
}

fn handle_text_input(app: &mut HtrkApp, ch: char) {
    let is_pattern = app.current_view == AppView::Pattern;
    let up = ch.to_ascii_uppercase();
    let sub = app.core.cursor.sub_column;
    let on_note = sub.accepts_note();
    let has_pattern = app.core.module.is_some();

    // Tracker piano keyboard (Impulse/Scream Tracker style). The lower row
    // (Z S X D C V G B H N J M) is the current octave and the upper row
    // (Q 2 W 3 E R 5 T 6 Y 7 U) is one octave up. A note key plays the selected
    // sample whenever it is pressed — regardless of cursor column or edit/view
    // mode — so the keyboard works as a live "jam" instrument.
    let note_key = NOTE_KEYS_LOWER
        .iter()
        .find_map(|(key, tone)| {
            let kc = key.name();
            (kc.len() == 1 && kc.chars().next() == Some(up))
                .then(|| app.current_octave as u8 * 12 + tone)
        })
        .or_else(|| {
            NOTE_KEYS_UPPER.iter().find_map(|(key, tone)| {
                let kc = key.name();
                (kc.len() == 1 && kc.chars().next() == Some(up))
                    .then(|| (app.current_octave as u8 + 1) * 12 + tone)
            })
        });

    let value_consumed = app.edit_mode && has_pattern && is_pattern && !on_note && is_value_char(sub, up);

    // Note keys always play a preview sound, regardless of edit mode or cursor column.
    // When value_consumed is true, the key also enters a value into the cell.
    if let Some(nk) = note_key {
        if !app.preview_browser_sample(nk) {
            preview_note(app, nk);
        }
        if !value_consumed {
            if is_pattern && on_note && app.edit_mode && has_pattern {
                let note = Note::On(nk);
                let mut new_cell = app.core.get_cell_at_cursor();
                new_cell.note = note;
                new_cell.instrument = Some(app.core.selected_instrument as u8);
                app.set_cell_at_cursor(new_cell);
                app.core.last_entered_cell = Some(new_cell);
                app.advance_cursor_down(app.cursor_skip as usize);
            }
        }
        if value_consumed {
            // Fall through to value entry below
        } else {
            return;
        }
    } else if is_pattern && on_note && ch == '.' && app.edit_mode && has_pattern {
        let mut new_cell = app.core.get_cell_at_cursor();
        new_cell.note = Note::Off;
        app.set_cell_at_cursor(new_cell);
        app.core.last_entered_cell = Some(new_cell);
        app.advance_cursor_down(app.cursor_skip as usize);
        return;
    }

    if ch == ',' || ch == '.' {
        let delta = if ch == ',' { -1 } else { 1 };
        if matches!(sub, SubColumn::InstrumentTens | SubColumn::InstrumentOnes) {
            app.change_selected_instrument(delta);
        } else {
            app.change_selected_sample(delta);
        }
        return;
    }

    if !has_pattern || !app.edit_mode || !is_pattern {
        return;
    }

    let col_vis = app.config.get_col_vis();
    let first_sub = crate::app::HtrkApp::first_visible_sub_column(col_vis);

    if app.core.cursor.sub_column.accepts_decimal() {
        if let Some(d) = ch.to_digit(10) {
            let d = d as u8;
            let mut cell = app.core.get_cell_at_cursor();

            match app.core.cursor.sub_column {
                SubColumn::InstrumentTens => {
                    let current = cell.instrument.unwrap_or(0);
                    cell.instrument = Some(d * 10 + (current % 10));
                }
                SubColumn::InstrumentOnes => {
                    let current = cell.instrument.unwrap_or(0);
                    cell.instrument = Some((current / 10 * 10) + d);
                }
                SubColumn::VolumeTens => {
                    let current = cell.volume.unwrap_or(0);
                    let val = d * 10 + (current % 10);
                    cell.volume = Some(val.min(64));
                }
                SubColumn::VolumeOnes => {
                    let current = cell.volume.unwrap_or(0);
                    let val = (current / 10 * 10) + d;
                    cell.volume = Some(val.min(64));
                }
                SubColumn::Note | SubColumn::EffectType | SubColumn::EffectParamHigh | SubColumn::EffectParamLow => return,
            }

            app.set_cell_at_cursor(cell);

            if let Some(next) = app.core.cursor.sub_column.next_visible(col_vis) {
                app.core.cursor.sub_column = next;
            } else {
                app.core.cursor.sub_column = first_sub;
                app.advance_cursor_down(app.cursor_skip as usize);
            }
            return;
        }
    }

    if app.core.cursor.sub_column == SubColumn::EffectType {
        let auto_target = app.core.automation_targets.get(app.core.cursor.channel).copied().flatten();
        if auto_target.is_some() {
            if let Some(d) = ch.to_ascii_uppercase().to_digit(16) {
                app.enter_automation_hex(app.core.cursor.channel, app.core.cursor.row, d as u8);
                app.advance_cursor_down(app.cursor_skip as usize);
                return;
            }
        }
        let mut cell = app.core.get_cell_at_cursor();
        let changed = if let Some(d) = ch.to_ascii_uppercase().to_digit(16) {
            cell.effect = hex_to_effect(d as u8);
            true
        } else {
            match ch.to_ascii_uppercase() {
                'P' => { cell.effect = Effect::SetSendBusParam { bus: 0, param: 0, value: 0 }; true }
                'Z' => { cell.effect = Effect::SetFilterCutoff { cutoff: 0 }; true }
                'S' => { cell.effect = Effect::SetSendLevel { send_index: 0, level: 0 }; true }
                'R' => { cell.effect = Effect::SetFilterResonance { resonance: 0 }; true }
                'X' => { cell.effect = Effect::SetFilterType { filter_type: 0 }; true }
                _ => false,
            }
        };
        if changed {
            app.set_cell_at_cursor(cell);
            if let Some(next) = app.core.cursor.sub_column.next_visible(col_vis) {
                app.core.cursor.sub_column = next;
            } else {
                app.core.cursor.sub_column = first_sub;
                app.advance_cursor_down(app.cursor_skip as usize);
            }
            return;
        }
    }

    if app.core.cursor.sub_column == SubColumn::EffectParamHigh
        || app.core.cursor.sub_column == SubColumn::EffectParamLow
    {
        let auto_target = app.core.automation_targets.get(app.core.cursor.channel).copied().flatten();
        if auto_target.is_some() {
            if let Some(d) = ch.to_ascii_uppercase().to_digit(16) {
                app.enter_automation_hex(app.core.cursor.channel, app.core.cursor.row, d as u8);
                app.advance_cursor_down(app.cursor_skip as usize);
                return;
            }
        }
        if let Some(d) = ch.to_ascii_uppercase().to_digit(16) {
            let d = d as u8;
            let mut cell = app.core.get_cell_at_cursor();
            match app.core.cursor.sub_column {
                SubColumn::EffectParamHigh => {
                    let param = effect_param(&cell.effect);
                    let new_param = (d << 4) | (param & 0x0F);
                    cell.effect = set_effect_param(&cell.effect, new_param);
                }
                SubColumn::EffectParamLow => {
                    let param = effect_param(&cell.effect);
                    let new_param = (param & 0xF0) | d;
                    cell.effect = set_effect_param(&cell.effect, new_param);
                }
                _ => unreachable!(),
            }
            app.set_cell_at_cursor(cell);
            if let Some(next) = app.core.cursor.sub_column.next_visible(col_vis) {
                app.core.cursor.sub_column = next;
            } else {
                app.core.cursor.sub_column = first_sub;
                app.advance_cursor_down(app.cursor_skip as usize);
            }
        }
    }
}

fn preview_note(app: &mut HtrkApp, note_key: u8) {
    let vol = 0.75;
    let sample_idx = if let Some(ref module) = app.core.module {
        let inst_idx = app.core.selected_instrument;
        if inst_idx > 0 && inst_idx < module.instruments.len() {
            let mapped = module.instruments[inst_idx].sample_map[note_key as usize];
            if mapped > 0 && (mapped as usize) < module.samples.len() {
                mapped as usize
            } else if let Some(first_mapped) = module.instruments[inst_idx].sample_map.iter().find(|&&s| s > 0) {
                if (*first_mapped as usize) < module.samples.len() {
                    *first_mapped as usize
                } else {
                    app.core.selected_sample
                }
            } else {
                app.core.selected_sample
            }
        } else {
            app.core.selected_sample
        }
    } else {
        app.core.selected_sample
    };
    app.core.send_command(crate::audio::commands::AudioCommand::TriggerPreviewNote {
        sample_index: sample_idx,
        note_key,
        volume: vol,
        panning: 0.5,
    });
}

fn hex_to_effect(d: u8) -> Effect {
    match d {
        0 => Effect::Arpeggio { note1: 0, note2: 0 },
        1 => Effect::PortamentoUp { speed: 0 },
        2 => Effect::PortamentoDown { speed: 0 },
        3 => Effect::TonePortamento { speed: 0 },
        4 => Effect::Vibrato { speed: 0, depth: 0 },
        5 => Effect::TonePortamentoVolumeSlide { up: 0 },
        6 => Effect::VibratoVolumeSlide { up: 0 },
        7 => Effect::Tremolo { speed: 0, depth: 0 },
        8 => Effect::SetPanning { pan: 0 },
        9 => Effect::SetSampleOffset { offset: 0 },
        0xA => Effect::VolumeSlide { up: 0, down: 0 },
        0xB => Effect::PositionJump { order: 0 },
        0xC => Effect::SetVolume { volume: 0 },
        0xD => Effect::PatternBreak { row: 0 },
        0xE => Effect::ExtendedEffect { param: 0 },
        0xF => Effect::SetSpeed { speed: 0 },
        _ => Effect::None,
    }
}

fn effect_param(effect: &Effect) -> u8 {
    crate::sequencer::effect::effect_param_value(effect).unwrap_or(0)
}

fn set_effect_param(effect: &Effect, param: u8) -> Effect {
    let mut fake_cell = Cell::default();
    fake_cell.effect = *effect;
    crate::sequencer::effect::set_effect_param_value(fake_cell, param).effect
}
