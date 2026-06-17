use eframe::egui;
use eguidev::DevUiExt;

use crate::app_config::SpacingMode;
use crate::core::HtrkCore;
use crate::ui::panel_event::PanelEvent;
use crate::ui::pattern_grid::{ColumnVisibility, GridMetrics, Selection};
use crate::ui::channel_headers::ChannelRenameState;
use crate::ui::theme::TrackerTheme;

pub struct PatternView {
    pub scroll_row: usize,
    pub scroll_channel: usize,
    pub last_visible_rows: usize,
    pub last_visible_channels: usize,
    pub channel_names: Vec<String>,
    pub channel_rename_state: ChannelRenameState,
    pub prev_channel_notes: [u16; 64],
}

impl Default for PatternView {
    fn default() -> Self {
        use crate::sequencer::DEFAULT_CHANNELS;
        use crate::ui::pattern_grid::VISIBLE_ROWS;
        PatternView {
            scroll_row: 0,
            scroll_channel: 0,
            last_visible_rows: VISIBLE_ROWS,
            last_visible_channels: 16,
            channel_names: (0..DEFAULT_CHANNELS).map(|i| format!("Ch{}", i + 1)).collect(),
            channel_rename_state: ChannelRenameState::default(),
            prev_channel_notes: [0; 64],
        }
    }
}

impl PatternView {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        core: &mut HtrkCore,
        config_editor_font_size: u32,
        config_spacing_mode: SpacingMode,
        config_col_vis: ColumnVisibility,
        config_row_highlight_minor: u8,
        config_row_highlight_major: u8,
        config_sample_length_bg: bool,
        theme: &TrackerTheme,
        playback_pattern: Option<usize>,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
    ) -> Vec<PanelEvent> {
        let mut events = Vec::new();
        let num_channels = core.num_channels();

        let metrics = GridMetrics::new(
            config_editor_font_size as f32,
            config_spacing_mode,
            config_col_vis,
        );
        let visible_channels = GridMetrics::calculate_visible_channels(ui, metrics);
        let visible_channels = visible_channels.min(num_channels - self.scroll_channel).max(1);

        let mut cursor_changed = false;

        ui.horizontal(|ui| {
            ui.set_min_height(0.0);
            if ui.dev_button("pattern.add_channel", "+").clicked() {
                events.push(PanelEvent::AddChannel);
            }
            let can_remove = core.module.as_ref()
                .map(|m| m.channel_panning.len() > 1).unwrap_or(false);
            if ui.dev_button("pattern.remove_channel", "−").clicked() && can_remove {
                events.push(PanelEvent::RemoveChannel);
            }
            if ui.dev_button("pattern.generate", "Generate").clicked() {
                events.push(PanelEvent::ShowPhraseGenerator);
            }
        });

        let note_on_flash = {
            let mut flash = [false; 64];
            let playing = core.playback_state.playing.load(std::sync::atomic::Ordering::Relaxed);
            if playing {
                for ch in 0..64 {
                    let current = core.playback_state.channel_note(ch);
                    let prev = self.prev_channel_notes[ch];
                    flash[ch] = current > 0 && current < 0xFD && (prev == 0 || prev >= 0xFD);
                    self.prev_channel_notes[ch] = current;
                }
            }
            flash
        };

        let ch_resp = crate::ui::channel_headers::draw_channel_headers(
            ui,
            num_channels,
            self.scroll_channel,
            visible_channels,
            &core.muted_channels,
            &core.solo_channels,
            &self.channel_names,
            &core.module.as_ref().map(|m| m.channel_panning.clone()).unwrap_or_default(),
            &core.send_levels,
            &mut self.channel_rename_state,
            theme,
            &core.playback_state,
            metrics,
            &core.automation_targets,
            &note_on_flash,
        );

        if let Some(ch) = ch_resp.toggle_mute {
            core.toggle_mute(ch);
        }
        if let Some(ch) = ch_resp.toggle_solo {
            core.toggle_solo(ch);
        }
        if let Some((ch, si, level)) = ch_resp.send_changed {
            core.set_send_level(ch, si, level);
        }
        if let Some((ch, name)) = ch_resp.rename_channel {
            if ch < self.channel_names.len() {
                self.channel_names[ch] = name;
            }
        }
        if let Some((ch, target)) = ch_resp.automation_target_changed {
            if ch < core.automation_targets.len() {
                core.automation_targets[ch] = target;
                if let Some(ref t) = target {
                    events.push(PanelEvent::SetAutomationTarget { channel: ch, target: *t });
                }
            }
        }

        if let Some(module) = &core.module {
            if !module.order_list.is_empty() {
                let order_idx = core.selected_order.min(module.order_list.len().saturating_sub(1));
                let pat_idx = module.order_list[order_idx] as usize;
                let grid_playback_row = if playback_pattern == Some(pat_idx) { playback_row } else { None };
                let pattern = core.current_pattern_or_default();
                let auto_overlays: Vec<Option<crate::ui::pattern_grid::AutomationOverlayInfo>> = (0..num_channels).map(|ch| {
                    core.automation_targets.get(ch).and_then(|t| t.as_ref()).map(|target| {
                        let track = module.automation_tracks.iter()
                            .find(|tr| tr.channel == Some(ch) && tr.target == *target)
                            .map(|tr| std::sync::Arc::new(tr.clone()));
                        crate::ui::pattern_grid::AutomationOverlayInfo {
                            target: *target,
                            track,
                            current_order: core.selected_order as u16,
                            speed: module.initial_speed,
                        }
                    })
                }).collect();

                let grid_resp = crate::ui::pattern_grid::draw_pattern_grid(
                    ui,
                    pattern,
                    &core.cursor,
                    core.selection.as_ref(),
                    grid_playback_row,
                    if grid_playback_row.is_some() { playback_tick } else { None },
                    playback_speed,
                    self.scroll_row,
                    self.scroll_channel,
                    num_channels,
                    metrics,
                    theme,
                    config_row_highlight_minor,
                    config_row_highlight_major,
                    config_sample_length_bg,
                    config_col_vis,
                    core.module.as_ref().map(|v| &**v),
                    &auto_overlays,
                );

                self.last_visible_rows = grid_resp.visible_rows;
                self.last_visible_channels = grid_resp.visible_channels;

                if let Some(pos) = grid_resp.clicked_position {
                    core.cursor = pos;
                    core.selection = None;
                    core.selection_anchor = None;
                    cursor_changed = true;
                }
                if let Some(pos) = grid_resp.drag_position {
                    if core.selection_anchor.is_none() {
                        core.selection_anchor = Some(core.cursor);
                    }
                    core.cursor = pos;
                    if let Some(anchor) = core.selection_anchor {
                        core.selection = Some(Selection {
                            start: anchor,
                            end: core.cursor,
                        });
                    }
                    cursor_changed = true;
                }
                if let Some(action) = grid_resp.context_menu_action {
                    events.push(PanelEvent::ContextMenuAction(action));
                }
                if let Some(interaction) = grid_resp.automation_interaction {
                    events.push(PanelEvent::AutomationInteraction(interaction));
                }
                if grid_resp.toggle_sample_length_bg {
                    events.push(PanelEvent::ToggleSampleLengthBg);
                }
                if let Some(tooltip) = grid_resp.effect_tooltip {
                    ui.label(egui::RichText::new(&tooltip).size(10.0).color(egui::Color32::GRAY));
                }
            }
        }

        if cursor_changed {
            events.push(PanelEvent::SyncToAudio);
        }

        events
    }
}
