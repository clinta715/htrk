# UI Shrink Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract 12 sections of `app.rs::ui()` (693 lines) into named methods, shrinking it to ~50 lines of orchestration.

**Architecture:** Pure extraction — each logical section becomes a `fn handle_xxx(&mut self, …)` method on `impl HtrkApp`. No behavioral changes, no new types, no file moves.

**Tech Stack:** Rust, egui, eframe

**Files:**
- Modify: `src/app.rs` — add 12 methods, trim `ui()`

---

### Task 1: Extract `handle_sample_tab`

**Files:** Modify `src/app.rs`

- [ ] **Read current code to confirm exact boundaries**

Lines 2055-2076. Verify the code before extracting.

- [ ] **Add `handle_sample_tab` method** (insert before `on_exit` at line 2318 or after the epilogue)

```rust
    fn handle_sample_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(module) = &self.core.module {
            if let Some(event) = self.sample_editor.ui(
                ui,
                module,
                &mut self.core.selected_sample,
                &self.theme,
                &self.core.playback_state,
            ) {
                if let Some(sel_update) = crate::actions::handle_sample_edit(self, event) {
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
```

- [ ] **Replace inline code with method call**

Replace lines 2055-2076:
```rust
                AppView::Sample => {
                    if let Some(module) = &self.core.module {
                        if let Some(event) = self.sample_editor.ui(
                            ui,
                            module,
                            &mut self.core.selected_sample,
                            &self.theme,
                            &self.core.playback_state,
                        ) {
                            if let Some(sel_update) = crate::actions::handle_sample_edit(self, event) {
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
```
with:
```rust
                AppView::Sample => self.handle_sample_tab(ui),
```

- [ ] **Build and test**

Run: `cargo build --lib 2>&1 | Select-String -Pattern "error|warning.*generated|Finished" -Context 0,2`
Expected: No errors, build succeeds.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_sample_tab method from ui()"
```

---

### Task 2: Extract `handle_mixer_tab`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_mixer_tab` method**

```rust
    fn handle_mixer_tab(&mut self, ui: &mut egui::Ui) {
        let mut plugin_slots: [Option<crate::sequencer::plugin::PluginSlot>; 4] = Default::default();
        let mut return_levels = [0.0f32; 4];
        if let Some(ref module) = self.core.module {
            for bus in 0..4 {
                if let Some(slot) = module.send_bus_plugins[bus].clone() {
                    plugin_slots[bus] = Some(slot);
                }
                return_levels[bus] = module
                    .send_return_levels
                    .get(bus)
                    .copied()
                    .unwrap_or(1.0);
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
```

- [ ] **Replace inline** (lines 2282-2307)

Replace:
```rust
                AppView::Mixer => {
                    let mut plugin_slots: [Option<crate::sequencer::plugin::PluginSlot>; 4] = Default::default();
                    let mut return_levels = [0.0f32; 4];
                    if let Some(ref module) = self.core.module {
                        for bus in 0..4 {
                            if let Some(slot) = module.send_bus_plugins[bus].clone() {
                                plugin_slots[bus] = Some(slot);
                            }
                            return_levels[bus] = module
                                .send_return_levels
                                .get(bus)
                                .copied()
                                .unwrap_or(1.0);
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
```
with:
```rust
                AppView::Mixer => self.handle_mixer_tab(ui),
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_mixer_tab method from ui()"
```

---

### Task 3: Extract `handle_playback_tab`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_playback_tab` method**

```rust
    fn handle_playback_tab(
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
                let pat_idx = self.core.module.as_ref()
                    .and_then(|m| m.order_list.get(self.core.selected_order))
                    .copied().unwrap_or(0) as usize;
                self.core.module.as_ref()?.patterns.get(pat_idx)
            });
        let current_module = self.core.module.as_ref().map(|m| &**m);
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
            if grid_playback_row.is_some() { playback_tick } else { None },
            playback_speed,
            self.config.get_spacing_mode(),
        );
    }
```

- [ ] **Replace inline** (lines 2198-2228)

Replace `AppView::Playback => { … }` block with:
```rust
                AppView::Playback => self.handle_playback_tab(
                    ui,
                    playback_pattern,
                    playback_row,
                    playback_tick,
                    playback_speed,
                ),
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_playback_tab method from ui()"
```

---

### Task 4: Extract `handle_instrument_tab`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_instrument_tab` method**

```rust
    fn handle_instrument_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(module) = &self.core.module {
            if let Some(event) = self.instrument_editor.ui(
                ui,
                module,
                &mut self.core.selected_instrument,
                &mut self.core.selected_sample,
                &self.theme,
                &self.core.playback_state,
                &mut self.config,
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
                        self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Save, &mut self.config);
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::LoadInstrument => {
                        self.browser_purpose = BrowserPurpose::LoadInstrument;
                        self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut self.config);
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::ExportInstrument(idx) => {
                        self.browser_purpose = BrowserPurpose::ExportInstrument(idx);
                        if let Some(ref m) = self.core.module {
                            if let Some(inst) = m.instruments.get(idx) {
                                self.file_browser.file_name = format!("{}.hti", inst.name.trim());
                            }
                        }
                        self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Save, &mut self.config);
                    }
                    crate::ui::instrument_editor::InstrumentEditEvent::ImportInstrument => {
                        self.browser_purpose = BrowserPurpose::LoadInstrument;
                        self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut self.config);
                    }
                    other => crate::actions::handle_instrument_edit(self, other),
                }
            }
        }
    }
```

- [ ] **Replace inline** (lines 2077-2120)

Replace `AppView::Instrument => { … }` block with:
```rust
                AppView::Instrument => self.handle_instrument_tab(ui),
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_instrument_tab method from ui()"
```

---

### Task 5: Extract `handle_automation_tab`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_automation_tab` method**

```rust
    fn handle_automation_tab(&mut self, ui: &mut egui::Ui) {
        self.automation_editor.state.selected_order = self.core.selected_order as u16;
        self.core.ensure_module_ownership();
        if let Some(ref mut module) = self.core.module {
            if let Some(arc_module) = Arc::get_mut(module) {
                let auto_resp = self.automation_editor.ui(
                    ui,
                    arc_module,
                    &self.theme,
                );
                if let Some((target, channel)) = auto_resp.track_added {
                    let id = arc_module.next_automation_id;
                    arc_module.next_automation_id += 1;
                    arc_module.automation_tracks.push(
                        crate::sequencer::AutomationTrack::new(id, target, channel)
                    );
                    self.automation_editor.state.selected_track_id = Some(id);
                }
                if let Some(tid) = auto_resp.track_removed {
                    arc_module.automation_tracks.retain(|t| t.id != tid);
                    if self.automation_editor.state.selected_track_id == Some(tid) {
                        self.automation_editor.state.selected_track_id = None;
                    }
                }
                if let Some(tid) = auto_resp.track_toggled {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == tid) {
                        t.enabled = !t.enabled;
                    }
                }
                if let Some((track_id, point)) = auto_resp.point_changed {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                        t.insert_point(point);
                    }
                }
                if let Some((track_id, order, row)) = auto_resp.point_removed {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                        t.remove_point_at(order, row);
                    }
                }
                if let Some((track_id, mode)) = auto_resp.interp_changed {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                        t.default_interp = mode;
                    }
                }
                if let Some((track_id, points)) = auto_resp.generator_points {
                    if let Some(t) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                        t.points = points;
                    }
                }
                self.core.sync_module_to_audio();
            }
        }
    }
```

- [ ] **Replace inline** (lines 2229-2281)

Replace `AppView::Automation => { … }` block with:
```rust
                AppView::Automation => self.handle_automation_tab(ui),
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_automation_tab method from ui()"
```

---

### Task 6: Extract `handle_transport_bar`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_transport_bar` method**

```rust
    fn handle_transport_bar(&mut self, ui: &mut egui::Ui) {
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
```

- [ ] **Replace inline** (lines 1788-1803)

Replace:
```rust
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
```
with:
```rust
        self.handle_transport_bar(ui);
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_transport_bar method from ui()"
```

---

### Task 7: Extract `handle_oscilloscope`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_oscilloscope` method**

```rust
    fn handle_oscilloscope(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
```

- [ ] **Replace inline** (lines 1805-1817)

Replace:
```rust
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
```
with:
```rust
        self.handle_oscilloscope(ui, &ctx);
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_oscilloscope method from ui()"
```

---

### Task 8: Extract `handle_status_bar`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_status_bar` method**

```rust
    fn handle_status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(22.0)
            .show_inside(ui, |ui| {
                let cpu = self.core.playback_state.cpu_usage_pct.load(std::sync::atomic::Ordering::Relaxed);
                let total_rows = self.core.current_pattern_or_default().num_rows;
                let hint = format!("Ins: {} | Smp: {}", self.core.selected_instrument, self.core.selected_sample);
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
```

- [ ] **Replace inline** (lines 1819-1846)

Replace the full `egui::Panel::bottom("status_bar")…` block with:
```rust
        self.handle_status_bar(ui);
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_status_bar method from ui()"
```

---

### Task 9: Extract `handle_pattern_tab`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_pattern_tab` method**

```rust
    fn handle_pattern_tab(
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
                                arc_module.channel_panning.push(crate::sequencer::module::PANNING_CENTER);
                                arc_module.channel_volume.push(crate::sequencer::module::VOLUME_MAX);
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
                                self.core.cursor.channel = self.core.num_channels().saturating_sub(1);
                            }
                            if self.pattern_view.scroll_channel >= self.core.num_channels() {
                                self.pattern_view.scroll_channel = self.core.num_channels().saturating_sub(1);
                            }
                        }
                    }
                }
                PanelEvent::SetAutomationTarget { channel, target } => {
                    self.core.ensure_module_ownership();
                    if let Some(ref mut module) = self.core.module {
                        if let Some(arc_module) = Arc::get_mut(module) {
                            let exists = arc_module.automation_tracks.iter().any(
                                |tr| tr.channel == Some(channel) && tr.target == target
                            );
                            if !exists {
                                let id = arc_module.next_automation_id;
                                arc_module.next_automation_id += 1;
                                arc_module.automation_tracks.push(
                                    crate::sequencer::AutomationTrack::new(id, target, Some(channel))
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
```

- [ ] **Replace inline** (lines 1966-2054)

Replace `AppView::Pattern => { … }` block with:
```rust
                AppView::Pattern => self.handle_pattern_tab(
                    ui,
                    playback_pattern,
                    playback_row,
                    playback_tick,
                    playback_speed,
                ),
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_pattern_tab method from ui()"
```

---

### Task 10: Extract `handle_sendfx_tab`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_sendfx_tab` method**

```rust
    fn handle_sendfx_tab(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        ctx: &egui::Context,
    ) {
        #[cfg(windows)]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> = {
            crate::audio::plugins::plugin_window::get_eframe_hwnd(frame)
                .map(|h| h as usize)
        };
        #[cfg(not(windows))]
        let eframe_hwnd: Option<crate::ui::sendfx_panel::EframeHwnd> = None;

        self.sendfx_panel.ui(
            ui,
            &mut self.core.command_sender,
            &mut self.send_bus_handles,
            eframe_hwnd,
        );

        if let Some(si) = self.sendfx_panel.plugin_browser_open_for {
            let discovered = self.discovered_plugins();
            let bus_letter = char::from(b'A' + si as u8);
            let mut open = true;
            let (result, action) = crate::ui::plugin_browser::draw_plugin_browser(
                &ctx,
                &mut open,
                si,
                &bus_letter.to_string(),
                &self.theme,
                &discovered,
                &self.plugin_browser_status,
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
                    self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Loading(descriptor.name.clone());
                    let sample_rate = if self.current_sample_rate > 0 {
                        self.current_sample_rate as f64
                    } else {
                        48000.0
                    };
                    let max_block = 512;
                    match crate::ui::plugin_browser::load_and_install_plugin(
                        &descriptor,
                        send_index,
                        sample_rate,
                        max_block,
                        &mut self.core.command_sender,
                    ) {
                        Ok((handle, name)) => {
                            self.send_bus_handles[send_index] = Some(handle);
                            self.sendfx_panel.plugin_names[send_index] = Some(name.clone());
                            self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Loaded(name);
                        }
                        Err(e) => {
                            eprintln!("[plugin] load failed: {e}");
                            self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Error(e);
                        }
                    }
                }
                crate::ui::plugin_browser::PluginSelectResult::Cancelled => {
                    self.plugin_browser_status = crate::ui::plugin_browser::PluginBrowserStatus::Idle;
                }
            }
            if !open {
                self.sendfx_panel.plugin_browser_open_for = None;
            }
        }
    }
```

- [ ] **Replace inline** (lines 2121-2197)

Replace `AppView::SendFx => { … }` block with:
```rust
                AppView::SendFx => self.handle_sendfx_tab(ui, frame, &ctx),
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_sendfx_tab method from ui()"
```

---

### Task 11: Extract `handle_order_list`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_order_list` method**

```rust
    fn handle_order_list(
        &mut self,
        ui: &mut egui::Ui,
        playback_order: Option<u16>,
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
                                    arc_module.order_list.insert(self.core.selected_order + 1, new_pat);
                                    changed = true;
                                }
                                if should_delete && arc_module.order_list.len() > 1 {
                                    if self.core.selected_order < arc_module.order_list.len() {
                                        arc_module.order_list.remove(self.core.selected_order);
                                        if self.core.selected_order >= arc_module.order_list.len() {
                                            self.core.selected_order = arc_module.order_list.len().saturating_sub(1);
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
                                let cur_pat_idx = *arc_module.order_list.get(self.core.selected_order).unwrap_or(&0) as usize;
                                if cur_pat_idx < arc_module.patterns.len() {
                                    let cloned = arc_module.patterns[cur_pat_idx].clone();
                                    let new_idx = arc_module.patterns.len() as u8;
                                    arc_module.patterns.push(cloned);
                                    let insert_at = (self.core.selected_order + 1).min(arc_module.order_list.len());
                                    arc_module.order_list.insert(insert_at, new_idx);
                                    self.core.selected_order = insert_at;
                                    changed = true;
                                }
                            }
                            if let Some((order_idx, new_rows)) = pattern_resized {
                                let pat_idx = *arc_module.order_list.get(order_idx).unwrap_or(&0) as usize;
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
```

- [ ] **Replace inline** (lines 1848-1941)

Replace the full block (lines 1848-1941) with:
```rust
        self.handle_order_list(ui, playback_order, playback_row, playback_tick, playback_speed);
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_order_list method from ui()"
```

---

### Task 12: Extract `handle_menu_bar`

**Files:** Modify `src/app.rs`

- [ ] **Add `handle_menu_bar` method**

```rust
    fn handle_menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
            );
            if menu_resp.new_song { self.new_song(); }
            if menu_resp.open_file { self.open_file_dialog(); }
            if let Some(ref path) = menu_resp.open_recent {
                crate::actions::load_file(self, path);
            }
            if menu_resp.import_sample {
                self.browser_purpose = BrowserPurpose::General;
                self.file_browser.open(BrowserMode::Samples, crate::ui::file_browser::DialogMode::Open, &mut self.config);
            }
            if menu_resp.import_instrument {
                self.browser_purpose = BrowserPurpose::LoadInstrument;
                self.file_browser.open(BrowserMode::Instruments, crate::ui::file_browser::DialogMode::Open, &mut self.config);
            }
            if menu_resp.save_file { crate::actions::save_current_file(self); }
            if menu_resp.save_as { self.save_as_dialog(); }
            if menu_resp.export_wav { crate::actions::open_wav_export_dialog(self); }
            if menu_resp.undo {
                self.core.ensure_module_ownership();
                if let Some(ref mut module) = self.core.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let _ = self.core.undo_manager.undo(arc_module);
                    }
                }
                self.core.sync_module_to_audio();
            }
            if menu_resp.redo {
                self.core.ensure_module_ownership();
                if let Some(ref mut module) = self.core.module {
                    if let Some(arc_module) = Arc::get_mut(module) {
                        let _ = self.core.undo_manager.redo(arc_module);
                    }
                }
                self.core.sync_module_to_audio();
            }
            if menu_resp.cut { self.core.copy_selection(); self.core.delete_selection(); }
            if menu_resp.copy { self.core.copy_selection(); }
            if menu_resp.paste { self.core.paste_at_cursor(); }
            if menu_resp.select_all { self.core.select_all(); }
            if menu_resp.cut_track && self.edit_mode { self.cut_track(); }
            if menu_resp.copy_track { self.copy_track(); }
            if menu_resp.delete_track && self.edit_mode { self.delete_track(); }
            if menu_resp.cut_column && self.edit_mode { self.cut_column(); }
            if menu_resp.copy_column { self.copy_column(); }
            if menu_resp.follow_playback { self.follow_playback = !self.follow_playback; }
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
            if menu_resp.show_shortcuts { self.show_shortcuts = true; }
            if menu_resp.show_about { self.show_about = true; }
            if menu_resp.show_settings {
                self.settings_state = crate::ui::settings_window::SettingsState::from_config(&self.config);
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
```

- [ ] **Replace inline** (lines 1652-1786)

Replace the full block (lines 1652-1786) with:
```rust
        self.handle_menu_bar(ui, &ctx);
```

- [ ] **Build and verify**

Run: `cargo build --lib`
Expected: No errors.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: extract handle_menu_bar method from ui()"
```

---

### Task 13: Final trim of `ui()` to orchestration shape

**Files:** Modify `src/app.rs`

After all 12 extractions, `ui()` will have 12 method calls in place of the inline blocks. This task cleans up the remaining structure and verifies the final shape.

- [ ] **Read the current `ui()` to verify it matches the expected shape**

The body should now look like:
```rust
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let devmcp = self.devmcp.clone();
        let _guard = FrameGuard::new(devmcp.as_ref(), &ctx);

        let vp_rect = ctx.viewport_rect();
        self.config.window_width = Some(vp_rect.width());
        self.config.window_height = Some(vp_rect.height());

        if !self.plugin_scan_done {
            let _ = self.rescan_plugins();
            self.plugin_scan_done = true;
        }

        let (playback_row, playback_order, playback_pattern, playback_tick, playback_speed) =
            self.draw_preamble(&ctx);

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.config.confirm_on_exit && self.core.module_dirty() && !self.show_exit_confirm && !self.exit_confirmed {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_exit_confirm = true;
            }
        }

        self.handle_menu_bar(ui, &ctx);
        self.handle_transport_bar(ui);
        self.handle_oscilloscope(ui, &ctx);
        self.handle_status_bar(ui);
        self.handle_order_list(ui, playback_order, playback_row, playback_tick, playback_speed);

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.core.module.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("htrk - tracker");
                    ui.add_space(20.0);
                    ui.label("No module loaded. Press Ctrl+O to open a file, or Ctrl+N for a new song.");
                });
                return;
            }

            ui.horizontal(|ui| {
                ui.dev_selectable_value("view.pattern", &mut self.current_view, AppView::Pattern, "Pattern");
                ui.dev_selectable_value("view.sample", &mut self.current_view, AppView::Sample, "Sample");
                ui.dev_selectable_value("view.instrument", &mut self.current_view, AppView::Instrument, "Instrument");
                ui.dev_selectable_value("view.sendfx", &mut self.current_view, AppView::SendFx, "Send FX");
                ui.dev_selectable_value("view.playback", &mut self.current_view, AppView::Playback, "Playback");
                ui.dev_selectable_value("view.automation", &mut self.current_view, AppView::Automation, "Automation");
                ui.dev_selectable_value("view.mixer", &mut self.current_view, AppView::Mixer, "Mixer");
            });
            ui.dev_separator("view.separator");

            match self.current_view {
                AppView::Pattern    => self.handle_pattern_tab(ui, playback_pattern, playback_row, playback_tick, playback_speed),
                AppView::Sample     => self.handle_sample_tab(ui),
                AppView::Instrument => self.handle_instrument_tab(ui),
                AppView::SendFx     => self.handle_sendfx_tab(ui, frame, &ctx),
                AppView::Playback   => self.handle_playback_tab(ui, playback_pattern, playback_row, playback_tick, playback_speed),
                AppView::Automation => self.handle_automation_tab(ui),
                AppView::Mixer      => self.handle_mixer_tab(ui),
            }
        });

        let size = ctx.viewport_rect().size();
        self.config.window_width = Some(size.x);
        self.config.window_height = Some(size.y);

        self.draw_dialogs(&ctx);
    }
```

- [ ] **Run full test suite**

Run: `cargo test --lib --release -- --test-threads=1`
Expected: 351 passed, 0 failed.

- [ ] **Run full build**

Run: `cargo build`
Expected: Clean build.

- [ ] **Commit**

```bash
git add src/app.rs && git commit -m "refactor: trim ui() to orchestration shape"
```
