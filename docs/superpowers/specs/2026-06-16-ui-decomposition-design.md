# UI Decomposition Design — 2026-06-16

## Goal

Shrink `app.rs::ui()` from ~868 lines to ~80 lines by extracting per-view panel
state into dedicated structs and moving action-dispatch out of the main method.

## Priority Order

1. **Isolate panel state** — move per-view state off `HtrkApp` into panel structs
2. **Shrink `ui()`** — make it a clean view-switcher with one call per panel
3. **Testability** — panels can be tested by constructing their struct + calling `ui()`

## Panel Structs

### New structs in `src/ui/`

Each struct owns the state that currently lives as fields on `HtrkApp`:

| Struct | Fields moved from `HtrkApp` |
|--------|---------------------------|
| `PatternView` | `scroll_row`, `scroll_channel`, `last_visible_rows`, `last_visible_channels`, `channel_names` (cached layer), `channel_rename_state`, `note_on_flash` |
| `PlaybackView` | `playback_scroll_row`, `playback_scroll_channel`, `playback_split`, `playback_zoom`, `playback_last_visible_rows` |
| `SendFxPanel` | `send_bus_effect_types`, `send_bus_params`, `send_pre_fader` |
| `SampleEditor` | `sample_selection`, `sample_clipboard`, `amplify_factor`, `sample_split` |
| `InstrumentEditor` | `instrument_split`, `instrument_settings_split`, `instrument_map_split` |

`AutomationEditorState` already exists as a struct — it stays but is owned by the
new `AutomationEditor` panel struct.

### Struct pattern

```rust
// src/ui/pattern_view.rs  (example)
pub struct PatternView {
    pub scroll_row: usize,
    pub scroll_channel: usize,
    pub last_visible_rows: usize,
    pub last_visible_channels: usize,
    pub channel_names: Vec<String>,
    pub channel_rename_state: ChannelRenameState,
    pub note_on_flash: [bool; 64],
}

impl PatternView {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        core: &mut HtrkCore,
        config: &AppConfig,
        theme: &TrackerTheme,
        playback: &AtomicPlaybackState,
    ) -> Vec<PanelEvent> {
        // 1. Draw channel headers
        // 2. Draw pattern grid
        // 3. Handle interactions → collect PanelEvents
        // 4. Return events for app.rs to dispatch
    }
}
```

## Event System

Only actions that mutate `Arc<Module>` produce events. Selection changes, scroll
position, splitter resizes are handled internally by the panel struct.

### `PanelEvent` enum

```rust
pub enum PanelEvent {
    // Module mutations
    AddChannel,
    RemoveChannel(usize),
    RenameChannel { idx: usize, name: String },
    InsertOrder,
    DuplicateOrder,
    RemoveOrder(usize),
    SelectOrder(usize),

    // Instrument / sample
    LoadInstrument(PathBuf),
    SaveInstrument(PathBuf),
    LoadSample(PathBuf),
    SaveSample(PathBuf),

    // Automation
    AutomationAddTrack,
    AutomationRemoveTrack(usize),
    AutomationEditPoint { track: usize, point: usize, value: f32, position: (u16, u8) },

    // Module-dirty forward
    MarkDirty,
    SyncToAudio,
}
```

### Dispatch in `app.rs`

```rust
fn dispatch_event(&mut self, event: PanelEvent) {
    match event {
        PanelEvent::AddChannel => {
            self.ensure_module_ownership();
            if let Some(ref mut module) = self.module {
                /* mutate */
            }
            self.sync_module_to_audio();
        }
        PanelEvent::SyncToAudio => self.sync_module_to_audio(),
        PanelEvent::LoadInstrument(path) => self.load_instrument_dialog(path),
        // ...
    }
}
```

## Resulting `app.rs::ui()` shape

```rust
fn ui(&mut self, ctx: &egui::Context) {
    self.draw_preamble(ctx);          // zoom, audio init, atomics, clamp, auto-scroll
    self.draw_menu_bar_and_dispatch(ctx);

    egui::TopBottomPanel::top("transport").show(ctx, |ui| {
        draw_transport(ui, &self.core.playback_state, &self.core.command_sender, &self.theme);
    });

    draw_oscilloscope(top_ui, &self.core.playback_state, &self.theme, self.core.num_channels());
    draw_status_bar(bottom_ui, &self.core, &self.config, self.current_octave, &self.theme);

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| { /* view tabs */ });
        let events = match self.current_view {
            AppView::Pattern => self.pattern_view.ui(ui, &mut self.core, &self.config, &self.theme),
            AppView::Sample  => self.sample_editor.ui(ui, &mut self.core, &self.config, &self.theme),
            AppView::Instrument => self.instrument_editor.ui(ui, &mut self.core, &self.config, &self.theme),
            AppView::SendFx  => self.sendfx_panel.ui(ui, &mut self.core, &self.config, &self.theme),
            AppView::Playback => self.playback_view.ui(ui, &mut self.core, &self.config, &self.theme),
            AppView::Automation => self.automation_editor.ui(ui, &mut self.core, &self.config, &self.theme),
        };
        for event in events { self.dispatch_event(event); }
    });

    // Modal overlays (settings, export, about, shortcuts, file browser)
    self.draw_dialogs(ctx);
}
```

Target: ~80 lines.

## Migration Strategy

Extract one panel at a time, in increasing complexity order:

1. **SendFxPanel** — simplest, no module mutation, pure UI state
2. **PlaybackView** — no module mutation, just scroll/split state
3. **SampleEditor** — self-contained, already returns `Option<SampleEditEvent>`
4. **InstrumentEditor** — similar to SampleEditor
5. **PatternView** — most complex, channel add/remove, rename, context menu
6. **AutomationEditor** — already has its own state struct, just needs event extraction

Each step:
- Move fields from `HtrkApp` to panel struct
- Wire panel struct into `HtrkApp` as a field
- Extract `ui()` method body from `app.rs` into panel struct
- Build + test (all 278+3 must pass)
- Commit

## Non-goals

- Not changing the existing `draw_xxx()` free function signatures or return types
- Not reworking the audio engine or core data model
- Not introducing a generic widget framework — just extraction

## Testability

After extraction, a panel can be tested without constructing `HtrkApp`:

```rust
#[test]
fn pattern_view_scrolls_correctly() {
    let mut view = PatternView::default();
    let mut core = HtrkCore::default();
    core.load_module(create_test_module());
    let events = view.ui(&mut Ui::dummy(), &mut core, &Config::default(), &TrackerTheme::dark());
    assert!(view.scroll_row == 0);
}
```

Panels receive `&mut HtrkCore` for read-only access to cursor, selection, and
module data. Module mutations (via `Arc::get_mut`) are deferred to `app.rs`
through `PanelEvent` dispatch — panels never call `ensure_module_ownership` or
`sync_module_to_audio` directly.
