# UI Decomposition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Shrink `app.rs::ui()` from ~868 lines to ~80 by extracting per-view panel state into dedicated structs.

**Architecture:** Each panel gets a struct in `src/ui/` that owns its scroll/split/selection state and provides a `ui()` method. Module-mutation actions bubble up as `PanelEvent` variants that `app.rs` dispatches via the existing `ensure_module_ownership` / `Arc::get_mut` / `sync_module_to_audio` pattern.

**Tech Stack:** Rust, egui, existing `src/ui/` panel pattern (free `draw_xxx()` functions + response types).

---

## File Map

**Create:**
- `src/ui/panel_event.rs` — `PanelEvent` enum
- `src/ui/sendfx_panel.rs` — `SendFxPanel` struct + `ui()` method
- `src/ui/playback_view_panel.rs` — `PlaybackView` struct + `ui()` method
- `src/ui/sample_editor_panel.rs` — `SampleEditor` struct + `ui()` method
- `src/ui/instrument_editor_panel.rs` — `InstrumentEditor` struct + `ui()` method
- `src/ui/pattern_view.rs` — `PatternView` struct + `ui()` method
- `src/ui/automation_editor_panel.rs` — `AutomationEditor` struct + `ui()` method

**Modify:**
- `src/ui/mod.rs` — add `pub mod` for each new file
- `src/app.rs` — replace inline draw+dispatch blocks with panel struct calls

---

### Task 0: Create `PanelEvent` enum

**Files:**
- Create: `src/ui/panel_event.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Create `src/ui/panel_event.rs`**

```rust
/// Events that require module mutation, bubbled up from panel `ui()` methods
/// to `app.rs` for dispatch via ensure_module_ownership / Arc::get_mut.
#[derive(Debug, Clone)]
pub enum PanelEvent {
    // Pattern view
    AddChannel,
    RemoveChannel,
    SetAutomationTarget { channel: usize, target: crate::sequencer::automation::AutomationTarget },
    ContextMenuAction(crate::actions::ContextMenuAction),

    // Order list
    InsertOrder,
    DuplicateOrder,
    RemoveOrder(usize),
    PatternChanged { order_idx: usize, pattern: u8 },
    PatternResized { order_idx: usize, num_rows: u8 },

    // Automation editor
    AutomationTrackAdded {
        target: crate::sequencer::automation::AutomationTarget,
        channel: Option<usize>,
    },
    AutomationTrackRemoved { track_id: u32 },
    AutomationTrackToggled { track_id: u32 },
    AutomationPointChanged {
        track_id: u32,
        point: crate::sequencer::automation::AutomationPoint,
    },
    AutomationPointRemoved { track_id: u32, order: u16, row: u8 },
    AutomationInterpChanged { track_id: u32, mode: crate::sequencer::automation::InterpolationMode },

    // Catch-all for module sync
    SyncToAudio,
}
```

- [ ] **Step 2: Register in `src/ui/mod.rs`**

```rust
pub mod panel_event;
```

- [ ] **Step 3: Build and verify**

```bash
cargo build
```
Expected: clean compile, 0 warnings.

- [ ] **Step 4: Commit**

```bash
git add src/ui/panel_event.rs src/ui/mod.rs
git commit -m "feat: define PanelEvent enum for panel-to-app communication"
```

---

### Task 1: Extract `SendFxPanel`

**Files:**
- Create: `src/ui/sendfx_panel.rs`
- Modify: `src/ui/mod.rs`, `src/app.rs`

`SendFxPanel` is the simplest: no module mutation, pure UI state. It stores the bus effect types, params, and pre-fader state that currently sit on `HtrkApp`.

- [ ] **Step 1: Create `src/ui/sendfx_panel.rs`**

```rust
use eframe::egui;
use crate::audio::commands::CommandSender;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::ui::sendfx_editor::SendEffectType;

pub struct SendFxPanel {
    pub effect_types: [SendEffectType; NUM_SEND_BUSES],
    pub params: [[f32; 5]; NUM_SEND_BUSES],
    pub pre_fader: [bool; NUM_SEND_BUSES],
}

impl Default for SendFxPanel {
    fn default() -> Self {
        SendFxPanel {
            effect_types: [SendEffectType::None; NUM_SEND_BUSES],
            params: [[0.0; 5]; NUM_SEND_BUSES],
            pre_fader: [false; NUM_SEND_BUSES],
        }
    }
}

impl SendFxPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui, command_sender: &mut Option<CommandSender>) {
        crate::ui::sendfx_editor::draw_sendfx_view(
            ui,
            command_sender,
            &mut self.effect_types,
            &mut self.params,
            &mut self.pre_fader,
        );
    }
}
```

- [ ] **Step 2: Register in `src/ui/mod.rs`**

```rust
pub mod sendfx_panel;
```

- [ ] **Step 3: Remove fields from `HtrkApp`** in `src/app.rs`

Delete these lines from the struct:
```rust
    pub(crate) send_bus_effect_types: [SendEffectType; NUM_SEND_BUSES],
    pub(crate) send_bus_params: [[f32; 5]; NUM_SEND_BUSES],
    pub(crate) send_pre_fader: [bool; NUM_SEND_BUSES],
```

Also remove the corresponding initialisation lines from the `Default` impl:
```rust
            send_bus_effect_types: [SendEffectType::None; NUM_SEND_BUSES],
            send_bus_params: [[0.0; 5]; NUM_SEND_BUSES],
            send_pre_fader: [false; NUM_SEND_BUSES],
```

- [ ] **Step 4: Add panel field to `HtrkApp`**

In the struct fields (after `pending_reinit: bool,`), add:
```rust
    pub(crate) sendfx_panel: crate::ui::sendfx_panel::SendFxPanel,
```

In the `Default` impl (after the `pending_reinit: false,` line), add:
```rust
            sendfx_panel: crate::ui::sendfx_panel::SendFxPanel::default(),
```

- [ ] **Step 5: Replace the inline block in `app.rs::ui()`**

Find the `AppView::SendFx => {` block (currently lines 1401-1409):
```rust
                AppView::SendFx => {
                    crate::ui::sendfx_editor::draw_sendfx_view(
                        ui,
                        &mut self.core.command_sender,
                        &mut self.send_bus_effect_types,
                        &mut self.send_bus_params,
                        &mut self.send_pre_fader,
                    );
                }
```

Replace with:
```rust
                AppView::SendFx => {
                    self.sendfx_panel.ui(ui, &mut self.core.command_sender);
                }
```

- [ ] **Step 6: Build and test**

```bash
cargo build
cargo test
```
Expected: clean compile, all 278+3 pass.

- [ ] **Step 7: Commit**

```bash
git add src/ui/sendfx_panel.rs src/ui/mod.rs src/app.rs
git commit -m "refactor: extract SendFxPanel from app.rs"
```

---

### Task 2: Extract `PlaybackView`

**Files:**
- Create: `src/ui/playback_view_panel.rs`
- Modify: `src/ui/mod.rs`, `src/app.rs`

`PlaybackView` stores scroll, split, zoom state currently on `HtrkApp`.

- [ ] **Step 1: Create `src/ui/playback_view_panel.rs`**

```rust
use eframe::egui;
use std::sync::Arc;
use crate::audio::commands::CommandSender;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::module::Module;
use crate::sequencer::pattern::Pattern;
use crate::ui::pattern_grid::{GridMetrics, ColumnVisibility};
use crate::ui::theme::TrackerTheme;

pub struct PlaybackView {
    pub scroll_row: usize,
    pub scroll_channel: usize,
    pub split: f32,
    pub zoom: u8,
    pub last_visible_rows: usize,
}

impl Default for PlaybackView {
    fn default() -> Self {
        PlaybackView {
            scroll_row: 0,
            scroll_channel: 0,
            split: 0.5,
            zoom: 10,
            last_visible_rows: 0,
        }
    }
}

impl PlaybackView {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        playback_state: &Arc<AtomicPlaybackState>,
        command_sender: &mut Option<CommandSender>,
        theme: &TrackerTheme,
        num_channels: usize,
        pattern: Option<&Pattern>,
        module: Option<&Module>,
        config_highlight_minor: u8,
        config_highlight_major: u8,
        sample_length_bg: bool,
        col_vis: ColumnVisibility,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
    ) {
        let metrics = GridMetrics::new(
            self.zoom as f32,
            crate::ui::pattern_grid::SpacingMode::default(),
            col_vis,
        );

        let visible_rows = crate::ui::playback_view::draw_playback_view(
            ui,
            playback_state,
            command_sender,
            theme,
            num_channels,
            pattern,
            module,
            self.scroll_row,
            self.scroll_channel,
            metrics,
            col_vis,
            config_highlight_minor,
            config_highlight_major,
            sample_length_bg,
            playback_row,
            playback_tick,
            playback_speed,
            &mut self.split,
            &mut self.zoom,
        );

        self.last_visible_rows = visible_rows;

        // Auto-scroll to follow playhead
        if let Some(row) = playback_row {
            if row < self.scroll_row {
                self.scroll_row = row;
            }
            if self.last_visible_rows > 0
                && row >= self.scroll_row + self.last_visible_rows
            {
                self.scroll_row = row - self.last_visible_rows + 1;
            }
        }
    }
}
```

- [ ] **Step 2: Register in `src/ui/mod.rs`**

```rust
pub mod playback_view_panel;
```

- [ ] **Step 3: Remove fields from `HtrkApp`**

Delete these lines from the struct fields:
```rust
    pub(crate) playback_scroll_row: usize,
    pub(crate) playback_scroll_channel: usize,
    pub(crate) playback_last_visible_rows: usize,
    pub(crate) playback_split: f32,
    pub(crate) playback_zoom: u8,
```

Delete these from the `Default` impl:
```rust
            playback_scroll_row: 0,
            playback_scroll_channel: 0,
            playback_last_visible_rows: 0,
            playback_split: 0.5,
            playback_zoom: 10,
```

- [ ] **Step 4: Add panel field to `HtrkApp`**

In the struct fields, add:
```rust
    pub(crate) playback_view: crate::ui::playback_view_panel::PlaybackView,
```

In the `Default` impl, add:
```rust
            playback_view: crate::ui::playback_view_panel::PlaybackView::default(),
```

- [ ] **Step 5: Replace the `AppView::Playback` block**

Find the `AppView::Playback => {` block (currently lines 1410-1459) and replace it with:

```rust
                AppView::Playback => {
                    let num_channels = self.core.num_channels();
                    let current_pattern = playback_pattern
                        .and_then(|pat| self.core.module.as_ref()?.patterns.get(pat));
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
                    );
                }
```

- [ ] **Step 6: Build and test**

```bash
cargo build
cargo test
```
Expected: clean compile, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/ui/playback_view_panel.rs src/ui/mod.rs src/app.rs
git commit -m "refactor: extract PlaybackView from app.rs"
```

---

### Task 3: Extract `SampleEditor`

**Files:**
- Create: `src/ui/sample_editor_panel.rs`
- Modify: `src/ui/mod.rs`, `src/app.rs`

`SampleEditor` stores selection, clipboard, amplify factor, and split state.

- [ ] **Step 1: Create `src/ui/sample_editor_panel.rs`**

```rust
use std::sync::Arc;
use eframe::egui;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::module::Module;
use crate::ui::theme::TrackerTheme;
use crate::actions;

pub struct SampleEditor {
    pub selection: Option<(usize, usize)>,
    pub clipboard: Option<Arc<Vec<f32>>>,
    pub amplify_factor: f32,
    pub split: f32,
}

impl Default for SampleEditor {
    fn default() -> Self {
        SampleEditor {
            selection: None,
            clipboard: None,
            amplify_factor: 1.0,
            split: 0.5,
        }
    }
}

impl SampleEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &Module,
        selected_sample: &mut usize,
        theme: &TrackerTheme,
        playback_state: &Arc<AtomicPlaybackState>,
    ) {
        if let Some(event) = crate::ui::sample_editor::draw_sample_editor(
            ui,
            module,
            selected_sample,
            theme,
            &mut self.selection,
            &mut self.clipboard,
            &mut self.amplify_factor,
            playback_state,
            &mut self.split,
        ) {
            // Sample editor events only require the event to be handled;
            // they don't need PanelEvent because they go through actions::handle_sample_edit
            // which takes the whole HtrkApp.
            // For now, we still need app.rs to handle this — see caveat below.
        }
    }
}
```

**Caveat:** `actions::handle_sample_edit(self, event)` takes `&mut HtrkApp`, not `&mut HtrkCore`. We need to either:
a. Return the `SampleEditEvent` via `PanelEvent` (preferred — add `SampleEdit(crate::ui::sample_editor::SampleEditEvent)` as a variant)
b. Pass a closure/callback

Use option (a). Add this to `PanelEvent`:
```rust
    SampleEdit(crate::ui::sample_editor::SampleEditEvent),
```

And update `dispatch_event` in app.rs to match and call `crate::actions::handle_sample_edit(self, event)`.

- [ ] **Step 2: Register in `src/ui/mod.rs`**

```rust
pub mod sample_editor_panel;
```

- [ ] **Step 3: Remove fields from `HtrkApp`**

Delete these lines from the struct fields:
```rust
    pub(crate) sample_selection: Option<(usize, usize)>,
    pub(crate) sample_clipboard: Option<Arc<Vec<f32>>>,
    pub(crate) amplify_factor: f32,
    pub(crate) sample_split: f32,
```

Delete these from the `Default` impl:
```rust
            sample_selection: None,
            sample_clipboard: None,
            amplify_factor: 1.0,
            sample_split: 0.5,
```

- [ ] **Step 4: Add panel field to `HtrkApp`**

In the struct fields, add:
```rust
    pub(crate) sample_editor: crate::ui::sample_editor_panel::SampleEditor,
```

In the `Default` impl, add:
```rust
            sample_editor: crate::ui::sample_editor_panel::SampleEditor::default(),
```

- [ ] **Step 5: Add `SampleEdit` variant to `PanelEvent`**

In `src/ui/panel_event.rs`, add:
```rust
    SampleEdit(crate::ui::sample_editor::SampleEditEvent),
```

- [ ] **Step 6: Replace the `AppView::Sample` block**

Find the `AppView::Sample => {` block (lines 1353-1369) and replace with:

```rust
                AppView::Sample => {
                    if let Some(module) = &self.core.module {
                        self.sample_editor.ui(
                            ui,
                            module,
                            &mut self.core.selected_sample,
                            &self.theme,
                            &self.core.playback_state,
                        );
                    }
                }
```

**Important:** Sample edit events that need module mutation (load, save, edit operations) currently go through `actions::handle_sample_edit(self, event)` which takes `&mut HtrkApp`. For this extraction, we need to return the event to app.rs. The cleanest way: have `sample_editor.ui()` return `Option<SampleEditEvent>` that `app.rs` matches on and dispatches.

Update the `ui()` method to return the event:

```rust
impl SampleEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &Module,
        selected_sample: &mut usize,
        theme: &TrackerTheme,
        playback_state: &Arc<AtomicPlaybackState>,
    ) -> Option<crate::ui::sample_editor::SampleEditEvent> {
        crate::ui::sample_editor::draw_sample_editor(
            ui,
            module,
            selected_sample,
            theme,
            &mut self.selection,
            &mut self.clipboard,
            &mut self.amplify_factor,
            playback_state,
            &mut self.split,
        )
    }
}
```

And in the `AppView::Sample` block:
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
                            crate::actions::handle_sample_edit(self, event);
                        }
                    }
                }
```

- [ ] **Step 7: Build and test**

```bash
cargo build
cargo test
```
Expected: clean compile, all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/ui/sample_editor_panel.rs src/ui/mod.rs src/ui/panel_event.rs src/app.rs
git commit -m "refactor: extract SampleEditor from app.rs"
```

---

### Task 4: Extract `InstrumentEditor`

**Files:**
- Create: `src/ui/instrument_editor_panel.rs`
- Modify: `src/ui/mod.rs`, `src/app.rs`

`InstrumentEditor` stores the three splitter positions.

- [ ] **Step 1: Create `src/ui/instrument_editor_panel.rs`**

```rust
use eframe::egui;
use std::sync::Arc;
use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::module::Module;
use crate::ui::theme::TrackerTheme;

pub struct InstrumentEditor {
    pub split: f32,
    pub settings_split: f32,
    pub map_split: f32,
}

impl Default for InstrumentEditor {
    fn default() -> Self {
        InstrumentEditor {
            split: 0.5,
            settings_split: 0.5,
            map_split: 0.5,
        }
    }
}

impl InstrumentEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &Module,
        selected_instrument: &mut usize,
        selected_sample: &mut usize,
        theme: &TrackerTheme,
        playback_state: &Arc<AtomicPlaybackState>,
    ) -> Option<crate::ui::instrument_editor::InstrumentEditEvent> {
        crate::ui::instrument_editor::draw_instrument_editor(
            ui,
            module,
            selected_instrument,
            selected_sample,
            theme,
            playback_state,
            &mut self.split,
            &mut self.settings_split,
            &mut self.map_split,
        )
    }
}
```

- [ ] **Step 2: Register in `src/ui/mod.rs`**

```rust
pub mod instrument_editor_panel;
```

- [ ] **Step 3: Remove fields from `HtrkApp`**

Delete these lines from the struct fields:
```rust
    pub(crate) instrument_split: f32,
    pub(crate) instrument_settings_split: f32,
    pub(crate) instrument_map_split: f32,
```

Delete these from the `Default` impl:
```rust
            instrument_split: 0.5,
            instrument_settings_split: 0.5,
            instrument_map_split: 0.5,
```

- [ ] **Step 4: Add panel field to `HtrkApp`**

In the struct fields, add:
```rust
    pub(crate) instrument_editor: crate::ui::instrument_editor_panel::InstrumentEditor,
```

In the `Default` impl, add:
```rust
            instrument_editor: crate::ui::instrument_editor_panel::InstrumentEditor::default(),
```

- [ ] **Step 5: Replace the `AppView::Instrument` block**

Find the `AppView::Instrument => {` block (lines 1370-1400) and replace with:

```rust
                AppView::Instrument => {
                    if let Some(module) = &self.core.module {
                        if let Some(event) = self.instrument_editor.ui(
                            ui,
                            module,
                            &mut self.core.selected_instrument,
                            &mut self.core.selected_sample,
                            &self.theme,
                            &self.core.playback_state,
                        ) {
                            match event {
                                crate::ui::instrument_editor::InstrumentEditEvent::SaveInstrument => {
                                    crate::actions::save_instrument_dialog(self);
                                }
                                crate::ui::instrument_editor::InstrumentEditEvent::LoadInstrument => {
                                    crate::actions::load_instrument_dialog(self);
                                }
                                crate::ui::instrument_editor::InstrumentEditEvent::ExportInstrument(idx) => {
                                    crate::actions::export_instrument_dialog(self, idx);
                                }
                                crate::ui::instrument_editor::InstrumentEditEvent::ImportInstrument => {
                                    crate::actions::load_instrument_dialog(self);
                                }
                                other => crate::actions::handle_instrument_edit(self, other),
                            }
                        }
                    }
                }
```

- [ ] **Step 6: Build and test**

```bash
cargo build
cargo test
```
Expected: clean compile, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/ui/instrument_editor_panel.rs src/ui/mod.rs src/app.rs
git commit -m "refactor: extract InstrumentEditor from app.rs"
```

---

### Task 5: Extract `PatternView`

**Files:**
- Create: `src/ui/pattern_view.rs`
- Modify: `src/ui/mod.rs`, `src/app.rs`, `src/ui/panel_event.rs`

This is the largest extraction — ~178 lines of inline code including channel add/remove, headers, grid rendering, and action dispatch.

- [ ] **Step 1: Create `src/ui/pattern_view.rs`**

```rust
use std::sync::Arc;

use eframe::egui;

use crate::audio::playback_state::AtomicPlaybackState;
use crate::sequencer::module::Module;
use crate::sequencer::pattern::Pattern;
use crate::sequencer::effect::NUM_SEND_BUSES;
use crate::ui::panel_event::PanelEvent;
use crate::ui::pattern_grid::{GridMetrics, ColumnVisibility, AutomationOverlayInfo};
use crate::ui::channel_headers::{ChannelRenameState, ChannelHeadersResponse};
use crate::ui::theme::TrackerTheme;
use crate::ui::pattern_grid::SpacingMode;
use crate::core::HtrkCore;

pub struct PatternView {
    pub scroll_row: usize,
    pub scroll_channel: usize,
    pub last_visible_rows: usize,
    pub last_visible_channels: usize,
    pub channel_names: Vec<String>,
    pub channel_rename_state: ChannelRenameState,
    pub note_on_flash: [bool; 64],
}

impl Default for PatternView {
    fn default() -> Self {
        PatternView {
            scroll_row: 0,
            scroll_channel: 0,
            last_visible_rows: 0,
            last_visible_channels: 0,
            channel_names: Vec::new(),
            channel_rename_state: ChannelRenameState::default(),
            note_on_flash: [false; 64],
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
        playback_order: Option<usize>,
        playback_pattern: Option<usize>,
        playback_row: Option<usize>,
        playback_tick: Option<u8>,
        playback_speed: u8,
        note_on_flash: &[bool; 64],
    ) -> Vec<PanelEvent> {
        let mut events = Vec::new();
        let num_channels = core.num_channels();

        let metrics = GridMetrics::new(config_editor_font_size as f32, config_spacing_mode, config_col_vis);
        let visible_channels = GridMetrics::calculate_visible_channels(ui, metrics);
        let visible_channels = visible_channels.min(num_channels - self.scroll_channel).max(1);

        // Channel add/remove buttons
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
        });

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
            note_on_flash,
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
                if let Some(t) = target {
                    events.push(PanelEvent::SetAutomationTarget { channel: ch, target: t });
                }
            }
        }

        if let Some(module) = &core.module {
            if !module.order_list.is_empty() {
                let order_idx = core.selected_order.min(module.order_list.len().saturating_sub(1));
                let pat_idx = module.order_list[order_idx] as usize;
                let grid_playback_row = if playback_pattern == Some(pat_idx) { playback_row } else { None };
                if let Some(pattern) = module.patterns.get(pat_idx) {
                    let auto_overlays: Vec<Option<AutomationOverlayInfo>> = (0..num_channels).map(|ch| {
                        core.automation_targets.get(ch).and_then(|t| t.as_ref()).map(|target| {
                            let track = module.automation_tracks.iter()
                                .find(|tr| tr.channel == Some(ch) && tr.target == *target)
                                .map(|tr| std::sync::Arc::new(tr.clone()));
                            AutomationOverlayInfo {
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
                        // ensure_cursor_visible is a UI concern — caller handles it
                    }
                    if let Some(pos) = grid_resp.drag_position {
                        if core.selection_anchor.is_none() {
                            core.selection_anchor = Some(core.cursor);
                        }
                        core.cursor = pos;
                        if let Some(anchor) = core.selection_anchor {
                            core.selection = Some(crate::core::Selection {
                                start: anchor,
                                end: core.cursor,
                            });
                        }
                        // ensure_cursor_visible is a UI concern
                    }
                    if let Some(action) = grid_resp.context_menu_action {
                        events.push(PanelEvent::ContextMenuAction(action));
                    }
                    if let Some(interaction) = grid_resp.automation_interaction {
                        // automation interactions mutate the module
                        events.push(PanelEvent::AutomationInteraction(interaction));
                    }
                    if grid_resp.toggle_sample_length_bg {
                        // This is a config toggle — we need a way to communicate it back
                        // We could add a PanelEvent variant or just return a separate bool
                    }
                    if let Some(tooltip) = grid_resp.effect_tooltip {
                        ui.label(egui::RichText::new(&tooltip).size(10.0).color(egui::Color32::GRAY));
                    }
                }
            }
        }

        events
    }
}
```

**Note:** `ensure_cursor_visible()` is called after cursor changes. This is a UI concern (it adjusts `self.scroll_row`) so it should be called by `app.rs` after `pattern_view.ui()` returns. Add a method or flag.

**Also note:** `toggle_sample_length_bg` toggles a config flag — this needs a PanelEvent variant too:
```rust
    ToggleSampleLengthBg,
```

And `AutomationInteraction` needs a variant:
```rust
    AutomationInteraction(crate::ui::pattern_grid::AutomationInteraction),
```

- [ ] **Step 2: Register in `src/ui/mod.rs`**

```rust
pub mod pattern_view;
```

- [ ] **Step 3: Add new `PanelEvent` variants**

In `src/ui/panel_event.rs`, add:
```rust
    ContextMenuAction(crate::actions::ContextMenuAction),
    SetAutomationTarget { channel: usize, target: crate::sequencer::automation::AutomationTarget },
    AutomationInteraction(crate::ui::pattern_grid::AutomationInteraction),
    ToggleSampleLengthBg,
```

- [ ] **Step 4: Remove fields from `HtrkApp`**

Delete these lines:
```rust
    pub(crate) scroll_row: usize,
    pub(crate) scroll_channel: usize,
    pub(crate) channel_names: Vec<String>,
    pub(crate) channel_rename_state: crate::ui::channel_headers::ChannelRenameState,
    pub(crate) last_visible_rows: usize,
    pub(crate) last_visible_channels: usize,
    pub(crate) prev_channel_notes: [u16; 64],
```

Delete these from the `Default` impl:
```rust
            scroll_row: 0,
            scroll_channel: 0,
            channel_names: Vec::new(),
            channel_rename_state: ChannelRenameState::default(),
            last_visible_rows: 0,
            last_visible_channels: 0,
            prev_channel_notes: [0; 64],
```

**Important:** `prev_channel_notes` is used for note-on flash detection in `process_cell_unified`. The `note_on_flash` array is computed from it at the top of `ui()`. Move the flash detection into `PatternView`.

- [ ] **Step 5: Add `pattern_view` field to `HtrkApp`**

In the struct fields:
```rust
    pub(crate) pattern_view: crate::ui::pattern_view::PatternView,
```

In the `Default` impl:
```rust
            pattern_view: crate::ui::pattern_view::PatternView::default(),
```

- [ ] **Step 6: Move the note-on flash logic**

Find the block in `app.rs::ui()` that computes `note_on_flash` (around lines 833-845):

```rust
        let mut note_on_flash: [bool; 64] = [false; 64];
        let playback_state = &*self.core.playback_state;
        for ch in 0..64 {
            let current = playback_state.channel_notes[ch].load(std::sync::atomic::Ordering::Relaxed);
            if current != self.prev_channel_notes[ch] {
                note_on_flash[ch] = true;
            }
            self.prev_channel_notes[ch] = current;
        }
```

This moves into `PatternView::ui()` as an internal detail. Remove `prev_channel_notes` from `HtrkApp` and put it in `PatternView`.

- [ ] **Step 7: Handle `ensure_cursor_visible`**

After `pattern_view.ui()` returns, `app.rs` calls `self.ensure_cursor_visible()`. The pattern view should expose a `needs_cursor_visible: bool` flag:

```rust
pub fn ui(&mut self, ...) -> Vec<PanelEvent> {
    let mut events = Vec::new();
    // ... existing code ...
    if let Some(pos) = grid_resp.clicked_position {
        core.cursor = pos;
        core.selection = None;
        core.selection_anchor = None;
    }
    // Note: we DON'T call ensure_cursor_visible here.
    // The caller (app.rs) will call it after ui() returns.
    // ...
    events
}
```

`app.rs` stays responsible for calling `ensure_cursor_visible()` after cursor changes.

- [ ] **Step 8: Replace the `AppView::Pattern` block**

Find the entire `AppView::Pattern => {` block (lines 1174-1352) and replace with:

```rust
                AppView::Pattern => {
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
                        playback_order,
                        playback_pattern,
                        playback_row,
                        playback_tick,
                        playback_speed,
                        &self.note_on_flash,
                    );
                    for event in events {
                        match event {
                            PanelEvent::AddChannel => {
                                self.core.ensure_module_ownership();
                                if let Some(ref mut module) = self.core.module {
                                    if let Some(arc_module) = Arc::get_mut(module) {
                                        if arc_module.channel_panning.len() < MAX_CHANNELS {
                                            arc_module.channel_panning.push(PANNING_CENTER);
                                            arc_module.channel_volume.push(VOLUME_MAX);
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
                            PanelEvent::ContextMenuAction(action) => {
                                self.handle_context_menu_action(action);
                            }
                            PanelEvent::AutomationInteraction(interaction) => {
                                self.handle_automation_interaction(interaction);
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
                            PanelEvent::ToggleSampleLengthBg => {
                                self.config.toggle_sample_length_bg();
                            }
                            _ => {}
                        }
                    }
                    if let Some(pos) = self.pattern_view.cursor_changed_to {
                        self.ensure_cursor_visible();
                    }
                }
```

**Note:** This step requires `MAX_CHANNELS`, `PANNING_CENTER`, `VOLUME_MAX` to be imported in `app.rs`. They already are (check the imports).

- [ ] **Step 9: Build and test**

```bash
cargo build
cargo test
```
Expected: clean compile, all tests pass. Fix any remaining reference errors.

- [ ] **Step 10: Commit**

```bash
git add src/ui/pattern_view.rs src/ui/mod.rs src/ui/panel_event.rs src/app.rs
git commit -m "refactor: extract PatternView from app.rs"
```

---

### Task 6: Extract `AutomationEditor`

**Files:**
- Create: `src/ui/automation_editor_panel.rs`
- Modify: `src/ui/mod.rs`, `src/app.rs`, `src/ui/panel_event.rs`

`AutomationEditor` already has its own `AutomationEditorState` struct. This task wraps it in a panel struct and moves the action-dispatch out of `app.rs`.

- [ ] **Step 1: Create `src/ui/automation_editor_panel.rs`**

```rust
use eframe::egui;
use crate::sequencer::module::Module;
use crate::sequencer::automation::{AutomationTarget, AutomationPoint, InterpolationMode};
use crate::ui::panel_event::PanelEvent;
use crate::ui::automation_editor::{AutomationEditorState, AutomationEditorResponse};
use crate::ui::theme::TrackerTheme;

pub struct AutomationEditor {
    pub state: AutomationEditorState,
}

impl Default for AutomationEditor {
    fn default() -> Self {
        AutomationEditor {
            state: AutomationEditorState::default(),
        }
    }
}

impl AutomationEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &mut Module,
        theme: &TrackerTheme,
    ) -> Vec<PanelEvent> {
        let mut events = Vec::new();

        let auto_resp = crate::ui::automation_editor::draw_automation_editor(
            ui,
            module,
            &mut self.state,
            theme,
        );

        if let Some((target, channel)) = auto_resp.track_added {
            events.push(PanelEvent::AutomationTrackAdded { target, channel });
        }
        if let Some(tid) = auto_resp.track_removed {
            events.push(PanelEvent::AutomationTrackRemoved { track_id: tid });
        }
        if let Some(tid) = auto_resp.track_toggled {
            events.push(PanelEvent::AutomationTrackToggled { track_id: tid });
        }
        if let Some((track_id, point)) = auto_resp.point_changed {
            events.push(PanelEvent::AutomationPointChanged { track_id, point });
        }
        if let Some((track_id, order, row)) = auto_resp.point_removed {
            events.push(PanelEvent::AutomationPointRemoved { track_id, order, row });
        }
        if let Some((track_id, mode)) = auto_resp.interp_changed {
            events.push(PanelEvent::AutomationInterpChanged { track_id, mode });
        }

        events
    }
}
```

**Note:** `draw_automation_editor` takes `&mut Module` (not `&Arc<Module>`) — it mutates the module directly. This is an exception because the editor was written before the `Arc::get_mut` pattern was standardized. For the extraction, we keep this signature but the panel returns events instead of mutating directly.

**Wait — `draw_automation_editor` already takes `&mut Module` and mutates it directly in the draw function.** This means the `Arc::get_mut` dance happens BEFORE calling `draw_automation_editor` (it does in the current app.rs code). After extraction, the panel struct's `ui()` method would need the raw `&mut Module` reference.

But this breaks the `PanelEvent` pattern. The current code does:

```rust
self.core.ensure_module_ownership();
if let Some(ref mut module) = self.core.module {
    if let Some(arc_module) = Arc::get_mut(module) {
        draw_automation_editor(ui, arc_module, &mut self.state, &self.theme);
        // ... dispatch responses ...
        self.core.sync_module_to_audio();
    }
}
```

For the extraction, we have two options:
a. Keep the `&mut Module` in the panel `ui()` — but this requires the caller to do the `Arc::get_mut` dance, which means the event pattern doesn't work cleanly for this panel.
b. Have the panel take `&mut Module` and mutate directly, returning events only for actions that need `sync_module_to_audio`.

Option (b) is more practical — the panel takes `&mut Module`, the caller does the `Arc::get_mut` dance around the call.

**Revised approach:**

```rust
impl AutomationEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &mut Module,
        theme: &TrackerTheme,
    ) {
        crate::ui::automation_editor::draw_automation_editor(
            ui,
            module,
            &mut self.state,
            theme,
        );
    }
}
```

And the caller in `app.rs` still wraps with `Arc::get_mut`. This is less ideal but avoids duplicating the editor's internal logic.

Actually, looking at `draw_automation_editor`, its `AutomationEditorResponse` provides events for track CRUD and point editing that `app.rs` dispatches. The editor internally mutates `module` for most operations (point edits, interp changes), while `app.rs` handles track creation/removal.

For the extraction, simply keep the existing pattern: the panel returns responses that the caller (app.rs) dispatches. The panel just wraps the draw call and state.

```rust
impl AutomationEditor {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        module: &mut Module,
        theme: &TrackerTheme,
    ) -> AutomationEditorResponse {
        crate::ui::automation_editor::draw_automation_editor(
            ui,
            module,
            &mut self.state,
            theme,
        )
    }
}
```

- [ ] **Step 2: Register in `src/ui/mod.rs`**

```rust
pub mod automation_editor_panel;
```

- [ ] **Step 3: Add field to `HtrkApp`, remove `automation_editor_state`**

In struct fields, replace:
```rust
    pub(crate) automation_editor_state: crate::ui::automation_editor::AutomationEditorState,
```
with:
```rust
    pub(crate) automation_editor: crate::ui::automation_editor_panel::AutomationEditor,
```

In the `Default` impl, replace:
```rust
            automation_editor_state: crate::ui::automation_editor::AutomationEditorState::default(),
```
with:
```rust
            automation_editor: crate::ui::automation_editor_panel::AutomationEditor::default(),
```

- [ ] **Step 4: Replace the `AppView::Automation` block**

Find the `AppView::Automation => {` block (lines 1461-1509) and replace with:

```rust
                AppView::Automation => {
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
                            self.core.sync_module_to_audio();
                        }
                    }
                }
```

- [ ] **Step 5: Build and test**

```bash
cargo build
cargo test
```
Expected: clean compile, all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/ui/automation_editor_panel.rs src/ui/mod.rs src/app.rs
git commit -m "refactor: extract AutomationEditor panel from app.rs"
```

---

### Task 7: Final `ui()` cleanup

**Files:**
- Modify: `src/app.rs`

No remaining state to move. The `ui()` method now calls panel `ui()` methods and dispatches events. This task extracts the preamble logic and dialog rendering into helper methods.

- [ ] **Step 1: Extract `draw_preamble` method**

Move lines 773-876 (zoom, audio init, keyboard, clamp, export progress, view switch, atomics read, note flash, follow-playback scroll) into:

```rust
impl HtrkApp {
    fn draw_preamble(&mut self, ctx: &egui::Context) -> (
        Option<usize>,  // playback_order
        Option<usize>,  // playback_pattern
        Option<usize>,  // playback_row
        Option<u8>,     // playback_tick
        u8,             // playback_speed
    ) {
        // zoom factor
        ctx.set_zoom_factor(self.config.zoom_factor);

        // lazy audio init
        if self.stream.is_none() && !self.audio_init_failed {
            self.init_audio();
        }

        // keyboard input
        crate::actions::keyboard::handle_keyboard_input(self, ctx);

        // focus-based key filtering
        // ...

        // clamp cursor/scroll
        // ...

        // export progress
        // ...

        // pending view switch
        // ...

        // read playback state
        // ...

        // note flash — now handled inside PatternView
        // ...

        // follow-playback auto-scroll
        // ...

        (playback_order, playback_pattern, playback_row, playback_tick, playback_speed)
    }
}
```

- [ ] **Step 2: Extract `draw_dialogs` method**

Move lines 1513-1637 (shortcuts, settings, export, about, sample export, file browser, auto-backup) into:

```rust
impl HtrkApp {
    fn draw_dialogs(&mut self, ctx: &egui::Context) {
        // shortcuts window
        // settings window
        // wav export window
        // about window
        // sample export dialog
        // file browser
        // auto-backup check
        // repaint request
    }
}
```

- [ ] **Step 3: Replace `ui()` body**

The main method becomes:

```rust
pub fn ui(&mut self, ctx: &egui::Context) {
    let (playback_order, playback_pattern, playback_row, playback_tick, playback_speed) =
        self.draw_preamble(ctx);

    self.draw_menu_bar_and_dispatch(ctx);

    egui::TopBottomPanel::top("transport").show(ctx, |ui| {
        crate::ui::transport::draw_transport(
            ui,
            &self.core.playback_state,
            &mut self.core.command_sender,
            &self.theme,
        );
    });

    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        crate::ui::status_bar::draw_status_bar(
            ui,
            self.core.module.as_deref(),
            self.core.selected_order,
            self.core.cursor.row,
            64, // total_rows placeholder
            self.core.num_channels(),
            0,  // cpu_pct placeholder
            self.current_octave,
            self.cursor_skip,
            self.core.selected_instrument,
            self.core.selected_sample,
            &self.core.playback_state,
            self.edit_mode,
            "",
            &self.theme,
        );
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        // Left panel: order list
        egui::Panel::left("order_list")
            .min_size(120.0)
            .default_size(150.0)
            .show_inside(ui, |ui| {
                if let Some(ref module) = self.core.module {
                    let order_resp = crate::ui::order_list::draw_order_list(
                        ui, module, self.core.selected_order,
                        playback_order, playback_row, playback_tick, playback_speed,
                        &self.theme,
                    );
                    // dispatch order list events (same as current code)
                }
            });

        // Central panel: view tabs + active view
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.core.module.is_none() {
                // no module placeholder
                return;
            }

            // View tabs
            ui.horizontal(|ui| {
                ui.dev_selectable_value("view.pattern", &mut self.current_view, AppView::Pattern, "Pattern");
                ui.dev_selectable_value("view.sample", &mut self.current_view, AppView::Sample, "Sample");
                ui.dev_selectable_value("view.instrument", &mut self.current_view, AppView::Instrument, "Instrument");
                ui.dev_selectable_value("view.sendfx", &mut self.current_view, AppView::SendFx, "Send FX");
                ui.dev_selectable_value("view.playback", &mut self.current_view, AppView::Playback, "Playback");
                ui.dev_selectable_value("view.automation", &mut self.current_view, AppView::Automation, "Automation");
            });
            ui.dev_separator("view.separator");

            match self.current_view {
                AppView::Pattern => { /* pattern_view.ui() + dispatch */ }
                AppView::Sample => { /* sample_editor.ui() + dispatch */ }
                AppView::Instrument => { /* instrument_editor.ui() + dispatch */ }
                AppView::SendFx => { /* sendfx_panel.ui() */ }
                AppView::Playback => { /* playback_view.ui() */ }
                AppView::Automation => { /* automation_editor.ui() + dispatch */ }
            }
        });
    });

    self.draw_dialogs(ctx);
}
```

Target: ~80-100 lines.

- [ ] **Step 4: Build and test**

```bash
cargo build
cargo test
```
Expected: clean compile, all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "refactor: final ui() cleanup — preamble + dialogs helpers"
```
