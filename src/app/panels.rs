use super::*;

impl HtrkApp {
    pub(super) fn handle_sample_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(module) = &self.core.module {
            if let Some(event) = self.sample_editor.ui(
                ui,
                module,
                &mut self.core.selected_sample,
                &self.theme,
                &self.core.playback_state,
            ) {
                if matches!(
                    event,
                    crate::ui::sample_editor::SampleEditEvent::OpenSampleLibrary
                ) {
                    self.sample_library_state.open = true;
                } else if let Some(sel_update) = crate::actions::handle_sample_edit(self, event) {
                    match sel_update {
                        crate::actions::SelectionUpdate::Clear => {
                            self.sample_editor.selection = None;
                        }
                        crate::actions::SelectionUpdate::Set(start, end) => {
                            self.sample_editor.selection = Some((start, end));
                        }
                    }
                }
            }
        }
    }

    pub(super) fn handle_mixer_tab(&mut self, ui: &mut egui::Ui) {
        let mut plugin_slots: [Option<crate::sequencer::plugin::PluginSlot>; 4] =
            Default::default();
        let mut return_levels = [0.0f32; 4];
        if let Some(ref module) = self.core.module {
            for bus in 0..4 {
                if let Some(slot) = module.send_bus_plugins[bus].clone() {
                    plugin_slots[bus] = Some(slot);
                }
                return_levels[bus] = module.send_return_levels.get(bus).copied().unwrap_or(1.0);
            }
        }
        self.mixer_state.ui(
            ui,
            &mut self.core,
            &self.theme,
            &plugin_slots,
            &return_levels,
        );
    }

    pub(super) fn handle_playback_tab(
        &mut self,
        ui: &mut egui::Ui,
        playback_pattern: Option<usize>,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
    ) {
        let num_channels = self.core.num_channels();
        let current_pattern = playback_pattern
            .and_then(|pat| self.core.module.as_ref()?.patterns.get(pat))
            .or_else(|| {
                let pat_idx = self
                    .core
                    .module
                    .as_ref()
                    .and_then(|m| m.order_list.get(self.core.selected_order))
                    .copied()
                    .unwrap_or(0) as usize;
                self.core.module.as_ref()?.patterns.get(pat_idx)
            });
        let current_module = self.core.module.as_deref();
        let grid_playback_row = playback_row;

        self.playback_view.ui(
            ui,
            &self.core.playback_state,
            &mut self.core.command_sender,
            &self.theme,
            num_channels,
            current_pattern,
            current_module,
            self.config.row_highlight_minor,
            self.config.row_highlight_major,
            self.config.get_sample_length_bg(),
            self.config.get_col_vis(),
            grid_playback_row,
            if grid_playback_row.is_some() {
                playback_tick
            } else {
                None
            },
            playback_speed,
            self.config.get_spacing_mode(),
        );
    }

    pub(super) fn handle_instrument_tab(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        #[cfg(windows)]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> =
            { crate::audio::plugins::plugin_window::get_eframe_hwnd(frame).map(|h| h as usize) };
        #[cfg(not(windows))]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> = None;

        // X-close poll and embedded editor panel rendering now happen
        // in HtrkApp::tick_plugin_editors (called from the frame-level
        // ui() pass) so editors stay alive in any tab.

        if let Some(module) = &self.core.module {
            // Live plugin-handle access: the editor reads editor state and
            // parameters directly from the handle (no per-frame cache).
            let inst_idx = self.core.selected_instrument;
            let plugin_handle: Option<&dyn crate::audio::plugins::HostedPluginHandle> = self
                .instrument_plugin_handles
                .get(inst_idx)
                .and_then(|h| h.as_deref());
            if let Some(event) = self.instrument_editor.ui(
                ui,
                module,
                &mut self.core.selected_instrument,
                &mut self.core.selected_sample,
                &self.theme,
                &self.core.playback_state,
                &mut self.config,
                plugin_handle,
                &mut self.core.command_sender,
            ) {
                match event {
                    crate::ui::instrument_editor::InstrumentEditEvent::SaveInstrument => {
                        self.browser_purpose = BrowserPurpose::SaveInstrument;
                        let inst_idx = self.core.selected_instrument;
                        if let Some(ref m) = self.core.module {
                            if let Some(inst) = m.instruments.get(inst_idx) {
                                self.file_browser.file_name = format!("{}.hti", inst.name.trim());
                            }
                        }
                        self.file_browser.open(
                            BrowserMode::Instruments,
                            crate::ui::file_browser::DialogMode::Save,
                            &mut self.config,
                        );
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::LoadInstrument => {
                        self.browser_purpose = BrowserPurpose::LoadInstrument;
                        self.file_browser.open(
                            BrowserMode::Instruments,
                            crate::ui::file_browser::DialogMode::Open,
                            &mut self.config,
                        );
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::ExportInstrument(idx) => {
                        self.browser_purpose = BrowserPurpose::ExportInstrument(idx);
                        if let Some(ref m) = self.core.module {
                            if let Some(inst) = m.instruments.get(idx) {
                                self.file_browser.file_name = format!("{}.hti", inst.name.trim());
                            }
                        }
                        self.file_browser.open(
                            BrowserMode::Instruments,
                            crate::ui::file_browser::DialogMode::Save,
                            &mut self.config,
                        );
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::ImportInstrument => {
                        self.browser_purpose = BrowserPurpose::LoadInstrument;
                        self.file_browser.open(
                            BrowserMode::Instruments,
                            crate::ui::file_browser::DialogMode::Open,
                            &mut self.config,
                        );
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::PluginUnload => {
                        // Unload the running instance but KEEP the PluginSlot
                        // (path/id/state) so the user can re-load the same
                        // plugin later with their saved state preserved.
                        let instrument_idx = self.core.selected_instrument;
                        self.unload_instrument_plugin(instrument_idx);
                        self.core.sync_module_to_audio();
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::OpenPluginEditor => {
                        let inst_idx = self.core.selected_instrument;
                        // The host HWND is passed as the transient parent so
                        // the plugin's floating window stays above the host.
                        let parent: Option<*mut std::ffi::c_void> =
                            eframe_hwnd.map(|h| h as *mut _);
                        if let Some(handle) = self.instrument_plugin_handles.get_mut(inst_idx) {
                            if let Some(ref mut h) = handle {
                                if let Err(e) = h.open_editor(
                                    crate::audio::plugins::EditorMode::Floating,
                                    parent,
                                ) {
                                    eprintln!("[plugin] instrument plugin open editor failed: {e}");
                                }
                            }
                        }
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::ClosePluginEditor => {
                        let inst_idx = self.core.selected_instrument;
                        if let Some(handle) = self.instrument_plugin_handles.get_mut(inst_idx) {
                            if let Some(ref mut h) = handle {
                                h.close_editor();
                            }
                        }
                    }
                    other => crate::actions::handle_instrument_edit(self, other),
                }
            }
        }

        // Plugin browser dialog for instrument plugins
        if self.instrument_editor.plugin_browser_open {
            let instrument_idx = self.core.selected_instrument;
            let discovered = self.discovered_plugins();
            let mut open = true;
            let (result, action) = crate::ui::plugin_browser::draw_plugin_browser(
                ui.ctx(),
                &mut open,
                instrument_idx,
                &format!("Instr {:02X}", instrument_idx),
                &self.theme,
                &discovered,
                &self.plugin_browser_status,
                &mut self.instrument_editor.plugin_browser_filter,
            );

            if action.rescan_requested {
                let summary = self.rescan_plugins();
                self.plugin_browser_status =
                    crate::ui::plugin_browser::PluginBrowserStatus::Error(summary);
            }

            match result {
                crate::ui::plugin_browser::PluginSelectResult::Selected {
                    descriptor,
                    send_index,
                } => {
                    self.plugin_browser_status =
                        crate::ui::plugin_browser::PluginBrowserStatus::Loading(
                            descriptor.name.clone(),
                        );
                    // If the user picks the same plugin that was previously
                    // loaded on this instrument, restore its saved state.
                    // A different plugin path starts fresh (state cleared).
                    let descriptor_path = descriptor.path.to_string_lossy().to_string();
                    let initial_state: Option<Vec<u8>> = self
                        .core
                        .module
                        .as_ref()
                        .and_then(|m| m.instruments.get(send_index))
                        .and_then(|i| i.plugin.as_ref())
                        .filter(|slot| slot.path == descriptor_path && !slot.state.is_empty())
                        .map(|slot| slot.state.clone());
                    match self.load_and_install_instrument_plugin(
                        &descriptor,
                        send_index,
                        initial_state.as_deref(),
                    ) {
                        Ok((handle, name)) => {
                            self.instrument_plugin_handles[send_index] = Some(handle);
                            self.instrument_editor.plugin_browser_open = false;

                            // Capture the freshly-loaded plugin's state so
                            // the very first project save persists the
                            // plugin's default patch.
                            self.save_all_instrument_plugin_states();

                            self.core.with_module_mut(|arc_module, _core| {
                                if send_index < arc_module.instruments.len() {
                                    arc_module.instruments[send_index].plugin =
                                        Some(crate::sequencer::plugin::PluginSlot::new(
                                            "clap",
                                            descriptor.path.to_string_lossy().to_string(),
                                            descriptor.plugin_id,
                                        ));
                                }
                            });

                            self.plugin_browser_status =
                                crate::ui::plugin_browser::PluginBrowserStatus::Loaded(name);
                        }
                        Err(e) => {
                            eprintln!("[plugin] instrument plugin load failed: {e}");
                            self.plugin_browser_status =
                                crate::ui::plugin_browser::PluginBrowserStatus::Error(e);
                        }
                    }
                }
                crate::ui::plugin_browser::PluginSelectResult::Cancelled => {
                    // No selection this frame — keep the dialog open.
                    // Only close when `open` becomes false (X button / Close).
                }
            }

            if !open {
                self.instrument_editor.plugin_browser_open = false;
            }
        }
    }

    pub(super) fn handle_automation_tab(&mut self, ui: &mut egui::Ui) {
        use crate::audio::plugins::ParamInfo;
        self.automation_editor.state.selected_order = self.core.selected_order as u16;
        self.core.ensure_module_ownership();
        if let Some(ref mut module) = self.core.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                // Capture the param_info lookups as closures so the
                // automation editor can enumerate plugin params without
                // needing a borrow of HtrkApp. The closures borrow
                // disjoint fields of `self` so the borrow checker
                // allows them alongside the `Arc::get_mut` borrow.
                let send_bus_handles_ref = &self.send_bus_handles;
                let instrument_plugin_handles_ref = &self.instrument_plugin_handles;
                let get_send_bus_params = |si: usize| -> Vec<ParamInfo> {
                    send_bus_handles_ref
                        .get(si)
                        .and_then(|h| h.as_ref())
                        .map(|h| h.parameter_info())
                        .unwrap_or_default()
                };
                let get_instrument_params = |inst: u8| -> Vec<ParamInfo> {
                    instrument_plugin_handles_ref
                        .get(inst as usize)
                        .and_then(|h| h.as_ref())
                        .map(|h| h.parameter_info())
                        .unwrap_or_default()
                };
                let auto_resp = self.automation_editor.ui(
                    ui,
                    arc_module,
                    &self.theme,
                    get_send_bus_params,
                    get_instrument_params,
                );
                if let Some((target, channel)) = auto_resp.track_added {
                    let id = arc_module.next_automation_id;
                    arc_module.next_automation_id += 1;
                    arc_module
                        .automation_tracks
                        .push(crate::sequencer::AutomationTrack::new(id, target, channel));
                    self.automation_editor.state.selected_track_id = Some(id);
                }
                if let Some(tid) = auto_resp.track_removed {
                    arc_module.automation_tracks.retain(|t| t.id != tid);
                    if self.automation_editor.state.selected_track_id == Some(tid) {
                        self.automation_editor.state.selected_track_id = None;
                    }
                }
                if let Some(tid) = auto_resp.track_toggled {
                    if let Some(t) = arc_module
                        .automation_tracks
                        .iter_mut()
                        .find(|t| t.id == tid)
                    {
                        t.enabled = !t.enabled;
                    }
                }
                if let Some((track_id, point)) = auto_resp.point_changed {
                    if let Some(t) = arc_module
                        .automation_tracks
                        .iter_mut()
                        .find(|t| t.id == track_id)
                    {
                        t.insert_point(point);
                    }
                }
                if let Some((track_id, order, row)) = auto_resp.point_removed {
                    if let Some(t) = arc_module
                        .automation_tracks
                        .iter_mut()
                        .find(|t| t.id == track_id)
                    {
                        t.remove_point_at(order, row);
                    }
                }
                if let Some((track_id, mode)) = auto_resp.interp_changed {
                    if let Some(t) = arc_module
                        .automation_tracks
                        .iter_mut()
                        .find(|t| t.id == track_id)
                    {
                        t.default_interp = mode;
                    }
                }
                if let Some((track_id, points)) = auto_resp.generator_points {
                    if let Some(t) = arc_module
                        .automation_tracks
                        .iter_mut()
                        .find(|t| t.id == track_id)
                    {
                        t.points = points;
                    }
                }
                self.core.sync_module_to_audio();
            }
        }
    }

    pub(super) fn handle_transport_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("transport_bar").show_inside(ui, |ui| {
            let transport_resp = crate::ui::transport::draw_transport(
                ui,
                &self.core.playback_state,
                &mut self.core.command_sender,
                &self.theme,
            );
            if transport_resp.prev_pattern_clicked {
                self.core.skip_to_prev_pattern();
                self.ensure_cursor_visible();
            }
            if transport_resp.next_pattern_clicked {
                self.core.skip_to_next_pattern();
                self.ensure_cursor_visible();
            }
        });
    }

    pub(super) fn handle_oscilloscope(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let num_ch = self.core.num_channels();
        let panel_w = ctx.content_rect().width() - 12.0;
        let scope_height = crate::ui::oscilloscope::compute_scope_height(panel_w, num_ch);
        egui::Panel::top("oscilloscope")
            .exact_size(scope_height)
            .show_inside(ui, |ui| {
                crate::ui::oscilloscope::draw_oscilloscope(
                    ui,
                    &self.core.playback_state,
                    &self.theme,
                    num_ch,
                );
            });
    }

    pub(super) fn handle_status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(22.0)
            .show_inside(ui, |ui| {
                let cpu = self
                    .core
                    .playback_state
                    .cpu_usage_pct
                    .load(std::sync::atomic::Ordering::Relaxed);
                let total_rows = self.core.current_pattern_or_default().num_rows;
                let hint = format!(
                    "Ins: {} | Smp: {}",
                    self.core.selected_instrument, self.core.selected_sample
                );
                let sample_delta = crate::ui::status_bar::draw_status_bar(
                    ui,
                    self.core.module.as_ref().map(|m| m.as_ref()),
                    self.core.selected_order,
                    self.core.cursor.row,
                    total_rows,
                    self.core.num_channels(),
                    cpu,
                    self.current_octave,
                    self.cursor_skip,
                    self.core.selected_instrument,
                    self.core.selected_sample,
                    &self.core.playback_state,
                    self.edit_mode,
                    self.core.cursor.sub_column,
                    &hint,
                    &self.theme,
                );
                if let Some(d) = sample_delta {
                    self.change_selected_sample(d);
                }
            });
    }

    pub(super) fn handle_pattern_tab(
        &mut self,
        ui: &mut egui::Ui,
        playback_pattern: Option<usize>,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
    ) {
        let events = self.pattern_view.ui(
            ui,
            &mut self.core,
            self.config.editor_font_size,
            self.config.get_spacing_mode(),
            self.config.get_col_vis(),
            self.config.row_highlight_minor,
            self.config.row_highlight_major,
            self.config.get_sample_length_bg(),
            &self.theme,
            playback_pattern,
            playback_row,
            playback_tick,
            playback_speed,
        );
        let mut cursor_changed = false;
        for event in events {
            match event {
                PanelEvent::AddChannel => {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut module) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            if arc_module.channel_panning.len() < MAX_CHANNELS {
                                arc_module
                                    .channel_panning
                                    .push(crate::sequencer::module::PANNING_CENTER);
                                arc_module
                                    .channel_volume
                                    .push(crate::sequencer::module::VOLUME_MAX);
                                self.core.sync_module_to_audio();
                                self.sync_channel_fields();
                            }
                        }
                    }
                }
                PanelEvent::RemoveChannel => {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut module) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            arc_module.channel_panning.pop();
                            arc_module.channel_volume.pop();
                            self.core.sync_module_to_audio();
                            self.sync_channel_fields();
                            if self.core.cursor.channel >= self.core.num_channels() {
                                self.core.cursor.channel =
                                    self.core.num_channels().saturating_sub(1);
                            }
                            if self.pattern_view.scroll_channel >= self.core.num_channels() {
                                self.pattern_view.scroll_channel =
                                    self.core.num_channels().saturating_sub(1);
                            }
                        }
                    }
                }
                PanelEvent::SetAutomationTarget { channel, target } => {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut module) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            let exists = arc_module
                                .automation_tracks
                                .iter()
                                .any(|tr| tr.channel == Some(channel) && tr.target == target);
                            if !exists {
                                let id = arc_module.next_automation_id;
                                arc_module.next_automation_id += 1;
                                arc_module.automation_tracks.push(
                                    crate::sequencer::AutomationTrack::new(
                                        id,
                                        target,
                                        Some(channel),
                                    ),
                                );
                            }
                        }
                    }
                    self.core.sync_module_to_audio();
                }
                PanelEvent::ContextMenuAction(action) => {
                    self.handle_context_menu_action(action);
                }
                PanelEvent::AutomationInteraction(interaction) => {
                    self.handle_automation_interaction(interaction);
                }
                PanelEvent::ToggleSampleLengthBg => {
                    self.config.toggle_sample_length_bg();
                }
                PanelEvent::SyncToAudio => {
                    cursor_changed = true;
                }
                PanelEvent::ShowPhraseGenerator => {
                    self.show_phrase_generator = true;
                }
                _ => {}
            }
        }
        if cursor_changed {
            self.ensure_cursor_visible();
        }
    }

    pub(super) fn handle_sendfx_tab(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        ctx: &egui::Context,
    ) {
        #[cfg(windows)]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> =
            { crate::audio::plugins::plugin_window::get_eframe_hwnd(frame).map(|h| h as usize) };
        #[cfg(not(windows))]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> = None;

        self.sendfx_panel.ui(
            ui,
            &mut self.core.command_sender,
            &mut self.send_bus_handles,
            eframe_hwnd,
            |send_index, state| {
                // The editor can't write to the module directly (it
                // doesn't own a &mut HtrkApp), so push to a queue that
                // gets drained immediately after the editor returns.
                self.pending_send_bus_state_writes.push((send_index, state));
            },
        );

        // Flush any pending state writes from the editor's "Remove" button.
        if !self.pending_send_bus_state_writes.is_empty() {
            self.core.ensure_module_ownership();
            if let Some(ref mut module) = self.core.module {
                if let Some(arc_module) = std::sync::Arc::get_mut(module) {
                    for (send_index, state) in self.pending_send_bus_state_writes.drain(..) {
                        if send_index < arc_module.send_bus_plugins.len() {
                            if let Some(ref mut slot) = arc_module.send_bus_plugins[send_index] {
                                slot.state = state;
                            }
                        }
                    }
                }
            }
        }

        if let Some(si) = self.sendfx_panel.plugin_browser_open_for {
            let discovered = self.discovered_plugins();
            let bus_letter = char::from(b'A' + si as u8);
            let mut open = true;
            let (result, action) = crate::ui::plugin_browser::draw_plugin_browser(
                ctx,
                &mut open,
                si,
                &bus_letter.to_string(),
                &self.theme,
                &discovered,
                &self.plugin_browser_status,
                &mut self.sendfx_panel.plugin_browser_filter,
            );
            if action.rescan_requested {
                let summary = self.rescan_plugins();
                self.plugin_browser_status =
                    crate::ui::plugin_browser::PluginBrowserStatus::Error(summary);
            }
            match result {
                crate::ui::plugin_browser::PluginSelectResult::Selected {
                    descriptor,
                    send_index,
                } => {
                    self.plugin_browser_status =
                        crate::ui::plugin_browser::PluginBrowserStatus::Loading(
                            descriptor.name.clone(),
                        );
                    let sample_rate = if self.current_sample_rate > 0 {
                        self.current_sample_rate as f64
                    } else {
                        48000.0
                    };
                    let max_block = 512;
                    // If the user picks the same plugin that was previously
                    // loaded on this bus, restore its saved state. A
                    // different plugin path starts fresh.
                    let descriptor_path = descriptor.path.to_string_lossy().to_string();
                    let initial_state: Option<Vec<u8>> = self
                        .core
                        .module
                        .as_ref()
                        .and_then(|m| m.send_bus_plugins.get(send_index))
                        .and_then(|opt| opt.as_ref())
                        .filter(|slot| slot.path == descriptor_path && !slot.state.is_empty())
                        .map(|slot| slot.state.clone());
                    match crate::ui::plugin_browser::load_and_install_plugin(
                        &descriptor,
                        send_index,
                        sample_rate,
                        max_block,
                        &mut self.core.command_sender,
                        initial_state.as_deref(),
                    ) {
                        Ok((handle, name)) => {
                            self.send_bus_handles[send_index] = Some(handle);
                            self.sendfx_panel.plugin_names[send_index] = Some(name.clone());
                            // Populate the module's send_bus_plugins slot so
                            // the .htk file persists the assignment and the
                            // plugin can auto-reload next time. Capture the
                            // freshly-loaded state so the very first project
                            // save persists the plugin's default patch.
                            self.core.ensure_module_ownership();
                            if let Some(ref mut module) = self.core.module {
                                if let Some(arc_module) = std::sync::Arc::get_mut(module) {
                                    if send_index < arc_module.send_bus_plugins.len() {
                                        arc_module.send_bus_plugins[send_index] =
                                            Some(crate::sequencer::plugin::PluginSlot::new(
                                                descriptor.format.as_str(),
                                                descriptor_path,
                                                descriptor.plugin_id,
                                            ));
                                    }
                                }
                            }
                            self.save_all_send_bus_plugin_states();
                            self.plugin_browser_status =
                                crate::ui::plugin_browser::PluginBrowserStatus::Loaded(name);
                        }
                        Err(e) => {
                            eprintln!("[plugin] load failed: {e}");
                            self.plugin_browser_status =
                                crate::ui::plugin_browser::PluginBrowserStatus::Error(e);
                        }
                    }
                }
                crate::ui::plugin_browser::PluginSelectResult::Cancelled => {
                    self.plugin_browser_status =
                        crate::ui::plugin_browser::PluginBrowserStatus::Idle;
                }
            }
            if !open {
                self.sendfx_panel.plugin_browser_open_for = None;
            }
        }
    }

    pub(super) fn handle_order_list(
        &mut self,
        ui: &mut egui::Ui,
        playback_order: Option<usize>,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
    ) {
        let order_list_width = self.config.order_list_width.unwrap_or(150.0);
        let order_panel_resp = egui::Panel::left("order_list")
            .resizable(true)
            .min_size(120.0)
            .default_size(order_list_width)
            .show_inside(ui, |ui| {
                if let Some(ref module) = self.core.module {
                    let order_resp = crate::ui::order_list::draw_order_list(
                        ui,
                        module,
                        self.core.selected_order,
                        playback_order,
                        playback_row,
                        playback_tick,
                        playback_speed,
                        &self.theme,
                    );
                    let should_insert = order_resp.insert_clicked;
                    let should_delete = order_resp.delete_clicked;
                    let should_duplicate = order_resp.duplicate_clicked;
                    let pattern_changed = order_resp.pattern_changed;
                    let pattern_resized = order_resp.pattern_resized;
                    let order_reordered = order_resp.order_reordered;
                    if let Some(idx) = order_resp.selected_order {
                        self.core.selected_order = idx;
                        self.core.cursor.row = 0;
                        self.ensure_cursor_visible();
                    }
                    let mut changed = false;
                    self.core.ensure_module_ownership();
                    if let Some(ref mut m) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(m) {
                            if let Some((order_idx, new_pat)) = pattern_changed {
                                if order_idx < arc_module.order_list.len() {
                                    arc_module.order_list[order_idx] = new_pat;
                                    changed = true;
                                }
                            }
                            if should_insert || should_delete {
                                if should_insert {
                                    let new_pat = arc_module.patterns.len() as u8;
                                    arc_module.patterns.push(crate::sequencer::Pattern::new(64));
                                    arc_module
                                        .order_list
                                        .insert(self.core.selected_order + 1, new_pat);
                                    changed = true;
                                }
                                if should_delete && arc_module.order_list.len() > 1 {
                                    if self.core.selected_order < arc_module.order_list.len() {
                                        arc_module.order_list.remove(self.core.selected_order);
                                        if self.core.selected_order >= arc_module.order_list.len() {
                                            self.core.selected_order =
                                                arc_module.order_list.len().saturating_sub(1);
                                        }
                                        changed = true;
                                    }
                                }
                            }
                            if let Some((from, to)) = order_reordered {
                                if from < arc_module.order_list.len() {
                                    let item = arc_module.order_list.remove(from);
                                    let insert_at = if to > from { to - 1 } else { to };
                                    let insert_at = insert_at.min(arc_module.order_list.len());
                                    arc_module.order_list.insert(insert_at, item);
                                    self.core.selected_order = insert_at;
                                    changed = true;
                                }
                            }
                            if should_duplicate {
                                let cur_pat_idx = *arc_module
                                    .order_list
                                    .get(self.core.selected_order)
                                    .unwrap_or(&0)
                                    as usize;
                                if cur_pat_idx < arc_module.patterns.len() {
                                    let cloned = arc_module.patterns[cur_pat_idx].clone();
                                    let new_idx = arc_module.patterns.len() as u8;
                                    arc_module.patterns.push(cloned);
                                    let insert_at = (self.core.selected_order + 1)
                                        .min(arc_module.order_list.len());
                                    arc_module.order_list.insert(insert_at, new_idx);
                                    self.core.selected_order = insert_at;
                                    changed = true;
                                }
                            }
                            if let Some((order_idx, new_rows)) = pattern_resized {
                                let pat_idx =
                                    *arc_module.order_list.get(order_idx).unwrap_or(&0) as usize;
                                if pat_idx < arc_module.patterns.len() {
                                    arc_module.patterns[pat_idx].resize_rows(new_rows);
                                    changed = true;
                                }
                            }
                        }
                    }
                    if changed {
                        self.core.sync_module_to_audio();
                    }
                } else {
                    ui.label("No module loaded");
                }
            });
        self.config.order_list_width = Some(order_panel_resp.response.rect.width());
    }

    pub(super) fn handle_menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let menu_bar_active = self.menu_nav.bar_active;
        let active_menu = self.menu_nav.active_menu;
        let force_open_menu = self.menu_nav.take_force_open();
        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            let menu_resp = crate::ui::menu_bar::draw_menu_bar(
                ui,
                self.core.undo_manager.can_undo(),
                self.core.undo_manager.can_redo(),
                self.core.selection.is_some(),
                self.follow_playback,
                self.theme_preset,
                self.config.get_spacing_mode(),
                &self.theme,
                self.current_sample_rate,
                &self.current_sample_format,
                &mut self.col_vis,
                &self.config.recent_files,
                menu_bar_active,
                active_menu,
                force_open_menu,
            );
            if menu_resp.new_song {
                self.new_song();
            }
            if menu_resp.open_file {
                self.open_file_dialog();
            }
            if let Some(ref path) = menu_resp.open_recent {
                crate::actions::load_file(self, path);
            }
            if menu_resp.import_sample {
                self.browser_purpose = BrowserPurpose::General;
                self.file_browser.open(
                    BrowserMode::Samples,
                    crate::ui::file_browser::DialogMode::Open,
                    &mut self.config,
                );
            }
            if menu_resp.import_instrument {
                self.browser_purpose = BrowserPurpose::LoadInstrument;
                self.file_browser.open(
                    BrowserMode::Instruments,
                    crate::ui::file_browser::DialogMode::Open,
                    &mut self.config,
                );
            }
            if menu_resp.save_file {
                crate::actions::save_current_file(self);
            }
            if menu_resp.save_as {
                self.save_as_dialog();
            }
            if menu_resp.export_wav {
                crate::actions::open_wav_export_dialog(self);
            }
            if menu_resp.undo {
                self.core.with_module_mut(|arc_module, core| {
                    let _ = core.undo_manager.undo(arc_module);
                });
            }
            if menu_resp.redo {
                self.core.with_module_mut(|arc_module, core| {
                    let _ = core.undo_manager.redo(arc_module);
                });
            }
            if menu_resp.cut {
                self.core.copy_selection();
                self.core.delete_selection();
            }
            if menu_resp.copy {
                self.core.copy_selection();
            }
            if menu_resp.paste {
                self.core.paste_at_cursor();
            }
            if menu_resp.select_all {
                self.core.select_all();
            }
            if menu_resp.cut_track && self.edit_mode {
                self.cut_track();
            }
            if menu_resp.copy_track {
                self.copy_track();
            }
            if menu_resp.delete_track && self.edit_mode {
                self.delete_track();
            }
            if menu_resp.cut_column && self.edit_mode {
                self.cut_column();
            }
            if menu_resp.copy_column {
                self.copy_column();
            }
            if menu_resp.follow_playback {
                self.follow_playback = !self.follow_playback;
            }
            if let Some(preset) = menu_resp.theme_changed {
                self.theme_preset = preset;
                self.theme = TrackerTheme::from_preset(preset);
                self.config.theme_preset = preset.config_key().to_string();
                self.config.save();
            }
            if let Some(mode) = menu_resp.spacing_mode_changed {
                self.config.set_spacing_mode(mode);
                self.config.save();
            }
            if let Some(col_vis) = menu_resp.col_vis {
                self.col_vis = col_vis;
                self.config.set_col_vis(col_vis);
                self.config.save();
            }
            if menu_resp.show_shortcuts {
                self.show_shortcuts = true;
            }
            if menu_resp.show_about {
                self.show_about = true;
            }
            if menu_resp.show_settings {
                self.settings_state =
                    crate::ui::settings_window::SettingsState::from_config(&self.config);
                self.settings_state.open = true;
            }
            if menu_resp.quit {
                if self.config.confirm_on_exit && self.core.module_dirty() {
                    self.show_exit_confirm = true;
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });
        if let Some(device_name) = self.pending_device_switch.take() {
            self.switch_output_device(device_name);
        }
        if self.pending_reinit {
            self.pending_reinit = false;
            if self.stream.is_some() {
                self.stream = None;
                self.core.command_sender = None;
                self.init_audio();
            }
        }
    }
}
