use super::*;

impl HtrkApp {
    pub(super) fn draw_preamble(
        &mut self,
        ctx: &egui::Context,
    ) -> (Option<usize>, Option<usize>, Option<usize>, Option<u8>, u8) {
        ctx.set_zoom_factor(self.config.zoom_factor);
        ctx.set_visuals(self.theme.to_visuals());

        if self.stream.is_none() && !self.audio_init_failed {
            self.init_audio();
        }

        // Process MCP mutation requests on the main thread.
        if let Some(ref mut server) = self.mcp_server {
            while let Ok(cmd) = server.command_rx.try_recv() {
                let result = crate::mcp::mutations::execute_mutation(
                    &mut self.core,
                    &cmd.method,
                    &cmd.params,
                );
                let _ = cmd.response_tx.send(result);
            }
        }

        crate::actions::handle_keyboard_input(self, ctx);

        let nch = self.core.num_channels();
        self.core.cursor.channel = self.core.cursor.channel.min(nch.saturating_sub(1));
        self.pattern_view.scroll_channel =
            self.pattern_view.scroll_channel.min(nch.saturating_sub(1));

        crate::actions::update_wav_export_progress(self);

        // Fixture may request module creation (empty_project / pattern_view).
        // A fixture establishes "default after launch" state, so it also
        // resets edit mode and closes any dialogs a previous test left
        // open — otherwise mode/dialog state leaks between smoketests
        // sharing one app instance (the full-suite 30/31/32 failures).
        if self.pending_new_song.swap(false, Ordering::Relaxed) {
            self.new_song();
            self.edit_mode = true;
            self.menu_nav = crate::actions::MenuNavState::default();
            while self.close_topmost_dialog() {}
        }

        let pending = self.pending_view_switch.swap(0, Ordering::Relaxed);
        if pending > 0 {
            self.current_view = match pending {
                1 => AppView::Pattern,
                2 => AppView::Sample,
                3 => AppView::Instrument,
                4 => AppView::SendFx,
                5 => AppView::Playback,
                6 => AppView::Automation,
                7 => AppView::Mixer,
                _ => self.current_view,
            };
        }

        let (playback_row, playback_order, playback_pattern, playback_tick, playback_speed) = {
            let playing = self
                .core
                .playback_state
                .playing
                .load(std::sync::atomic::Ordering::Relaxed);
            if playing {
                let order =
                    self.core
                        .playback_state
                        .current_order
                        .load(std::sync::atomic::Ordering::Relaxed) as usize;
                let row = self
                    .core
                    .playback_state
                    .current_row
                    .load(std::sync::atomic::Ordering::Relaxed) as usize;
                let pat = self
                    .core
                    .playback_state
                    .current_pattern
                    .load(std::sync::atomic::Ordering::Relaxed) as usize;
                let tick = self
                    .core
                    .playback_state
                    .current_tick
                    .load(std::sync::atomic::Ordering::Relaxed);
                let speed = self
                    .core
                    .playback_state
                    .speed
                    .load(std::sync::atomic::Ordering::Relaxed);
                (Some(row), Some(order), Some(pat), Some(tick), speed)
            } else {
                (None, None, None, None, 6)
            }
        };

        if let Some(order) = playback_order {
            if self.follow_playback && order != self.core.selected_order {
                if let Some(ref module) = self.core.module {
                    if order < module.order_list.len() {
                        self.core.selected_order = order;
                        self.core.cursor.row = 0;
                        self.ensure_cursor_visible();
                    }
                }
            }
        }

        if let Some(row) = playback_row {
            if self.follow_playback {
                if let (Some(active_pat), Some(module)) =
                    (playback_pattern, self.core.module.as_ref())
                {
                    if !module.order_list.is_empty() {
                        let order_idx = self
                            .core
                            .selected_order
                            .min(module.order_list.len().saturating_sub(1));
                        let displayed_pat = module.order_list[order_idx] as usize;
                        if displayed_pat == active_pat {
                            if row < self.pattern_view.scroll_row {
                                self.pattern_view.scroll_row = row;
                            }
                            if row
                                >= self.pattern_view.scroll_row
                                    + self.pattern_view.last_visible_rows
                            {
                                self.pattern_view.scroll_row =
                                    row - self.pattern_view.last_visible_rows + 1;
                            }
                        }
                    }
                }
            }
        }

        // Update MCP module snapshot for the MCP thread.
        if let Some(ref mut server) = self.mcp_server {
            if let Some(ref module) = self.core.module {
                // The module-derived snapshots (module/patterns/instruments/
                // samples JSON) are expensive to build — they serialize the
                // entire module every frame. Gate them behind a pointer-
                // identity check: the `Arc<Module>` only gets a new pointer
                // when `ensure_module_ownership()` deep-clones during an edit,
                // so when the pointer is unchanged we can skip the rebuild
                // entirely.
                let module_ptr = std::sync::Arc::as_ptr(module) as usize;
                let module_dirty = self.mcp_last_module_ptr != Some(module_ptr);

                if module_dirty {
                    self.mcp_last_module_ptr = Some(module_ptr);

                    let patterns_json: Vec<(usize, serde_json::Value)> = module
                        .patterns
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let rows: Vec<Vec<crate::sequencer::pattern::Cell>> = p
                                .data
                                .iter()
                                .map(|row| row[..module.channel_panning.len()].to_vec())
                                .collect();
                            (i, serde_json::json!({"num_rows": p.num_rows, "data": rows}))
                        })
                        .collect();
                    let instruments_json: Vec<(usize, serde_json::Value)> = module
                        .instruments
                        .iter()
                        .enumerate()
                        .map(|(i, inst)| (i, serde_json::to_value(inst).unwrap_or_default()))
                        .collect();
                    let samples_json: Vec<(usize, serde_json::Value)> = module
                        .samples
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let info = serde_json::json!({
                                "name": s.name,
                                "sample_rate": s.sample_rate,
                                "bits_per_sample": s.bits_per_sample,
                                "length": s.data.len(),
                                "loop_type": format!("{:?}", s.loop_type),
                                "loop_start": s.loop_start,
                                "loop_end": s.loop_end,
                                "default_volume": s.default_volume,
                                "default_panning": s.default_panning,
                                "global_volume": s.global_volume,
                                "relative_note": s.relative_note,
                                "fine_tune": s.fine_tune,
                            });
                            (i, info)
                        })
                        .collect();

                    let module_json = serde_json::to_value(&**module).unwrap_or_default();

                    let snapshot = crate::mcp::protocol::ModuleSnapshot {
                        module_json: Some(module_json),
                        patterns_json,
                        instruments_json,
                        samples_json,
                    };
                    if let Ok(mut lock) = server.snapshot.write() {
                        *lock = snapshot;
                    }

                    // The channels snapshot is derived from the module too
                    // (panning/volume), so only refresh it when the module
                    // changes. `muted`/`solo` are app-level and could in
                    // principle change without a module sync, but those
                    // mutations are rare and the next module edit will
                    // pick them up; keeping this gated avoids per-frame
                    // Vec clones.
                    let ch_snapshot = crate::mcp::protocol::ChannelsSnapshot {
                        panning: module.channel_panning.clone(),
                        volume: module.channel_volume.clone(),
                        muted: self.core.muted_channels.clone(),
                        solo: self.core.solo_channels.clone(),
                    };
                    if let Ok(mut lock) = server.channels_snapshot.write() {
                        *lock = ch_snapshot;
                    }
                }

                // The playback snapshot is cheap (atomic reads) and must
                // stay live every frame so MCP clients see current transport
                // position even when the module isn't being edited.
                let pb = &self.core.playback_state;
                let playing = pb.playing.load(std::sync::atomic::Ordering::Relaxed);
                let pb_snapshot = crate::mcp::protocol::PlaybackSnapshot {
                    playing,
                    current_order: pb.current_order.load(std::sync::atomic::Ordering::Relaxed),
                    current_row: pb.current_row.load(std::sync::atomic::Ordering::Relaxed),
                    current_pattern: pb
                        .current_pattern
                        .load(std::sync::atomic::Ordering::Relaxed),
                    current_tick: pb.current_tick.load(std::sync::atomic::Ordering::Relaxed),
                    bpm: pb.bpm.load(std::sync::atomic::Ordering::Relaxed),
                    speed: pb.speed.load(std::sync::atomic::Ordering::Relaxed),
                    active_voices: pb.active_voices.load(std::sync::atomic::Ordering::Relaxed),
                    cpu_usage_pct: pb.cpu_usage_pct.load(std::sync::atomic::Ordering::Relaxed),
                };
                if let Ok(mut lock) = server.playback_snapshot.write() {
                    *lock = pb_snapshot;
                }
            }
        }

        (
            playback_row,
            playback_order,
            playback_pattern,
            playback_tick,
            playback_speed,
        )
    }
}
