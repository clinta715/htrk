# Phase 1: Core Foundation — Design Spec

**Date:** 2026-05-19
**Status:** Approved

## Goal
Extract the "brain" of the application from the UI loop. Move all editing logic, module mutation, undo management, and audio command dispatch into a new `HtrkCore` struct under `src/core/`, leaving `HtrkApp` as a thin UI shell.

## Milestone
A headless test that loads a module and performs a series of edits without initializing `eframe`.

---

## Architecture

### HtrkCore struct (`src/core/mod.rs`)

```rust
pub struct HtrkCore {
    // Data model
    module: Option<Arc<Module>>,
    loaded_module_name: String,
    file_path: Option<String>,

    // Editing state
    undo_manager: UndoManager,
    clipboard: Option<Vec<Vec<Cell>>>,
    clipboard_width: usize,
    last_entered_cell: Option<Cell>,

    // Cursor / selection
    cursor: CursorPosition,
    selection: Option<Selection>,
    selection_anchor: Option<CursorPosition>,

    // Channel state
    muted_channels: Vec<bool>,
    solo_channels: Vec<bool>,
    send_levels: Vec<[f32; 4]>,

    // Selection indices
    selected_order: usize,
    selected_sample: usize,
    selected_instrument: usize,

    // Audio bridge
    command_sender: Option<CommandSender>,
    playback_state: Arc<AtomicPlaybackState>,

    // Dirty tracking
    module_dirty: bool,
    last_backup_time: std::time::Instant,
}
```

### What stays in HtrkApp (UI-only)

- `stream: Option<cpal::Stream>` — audio device handle (non-cloneable, side-effectful)
- View state: `current_view`, `edit_mode`, `follow_playback`
- Input preferences: `current_octave`, `cursor_skip`, `edit_mask_*`, `multichannel_*`
- Scroll: `scroll_row`, `scroll_channel`
- Dialog state: `show_shortcuts`, `show_about`, `settings_state`, `wav_export_state`, etc.
- Rendering: `theme`, `theme_preset`, `col_vis`, `last_visible_rows/channels`
- UI widgets: `file_browser`, `channel_rename_state`, `sample_selection`, `sample_clipboard`, `amplify_factor`
- Automation UI: `automation_targets`, `automation_dragging`, `automation_editor_state`
- Send FX UI caching: `send_bus_effect_types`, `send_bus_params`
- Audio device state: `output_device_names`, `selected_device_name`, `current_sample_rate`, `current_sample_format`, `pending_device_switch`, `pending_reinit`, `audio_init_failed`
- Config: `config: AppConfig`

### Submodule layout

```
src/core/
  mod.rs           — HtrkCore struct, constructor, ensure_ownership,
                      with_module_mut(), sync_to_audio(), load_file,
                      new_song, save_file, module accessors
  editing.rs       — Cell-level pattern editing: set_cell_at_cursor,
                      clear_cell_at_cursor, insert_row, delete_row,
                      copy_selection, paste_at_cursor, delete_selection,
                      transpose_selection, select_all, context menu actions,
                      order list mutations (add/delete/duplicate/reorder)
  keyboard.rs      — EditAction enum + handle_edit_action() dispatch.
                      Takes resolved inputs, not raw egui events.
  sample.rs         — handle_sample_edit event handler, import_wav
  instrument.rs     — handle_instrument_edit event handler,
                      load_instrument_from_file
  automation.rs     — Automation point CRUD: create, move, delete,
                      hex entry, interp change, track add/remove
  channels.rs       — Mute/solo toggle, channel add/remove,
                      send_levels, sync_channel_fields
```

### EditAction enum (`src/core/keyboard.rs`)

Decouples key-to-action mapping from action execution. The UI maps egui key events to `EditAction` variants; core processes them.

```rust
pub enum EditAction {
    // Cell editing
    SetCell(Cell),
    ClearCell,
    InsertRow,
    DeleteRow,
    // Selection
    Copy,
    Cut,
    Paste,
    DeleteSelection,
    SelectAll,
    Transpose { semitones: i8 },
    // Playback
    Play,
    PlayFrom { order: usize, row: usize },
    Stop,
    Pause,
    SetPlayMode(PlayMode),
    // Channel
    ToggleMute { channel: usize },
    ToggleSolo { channel: usize },
    // Preview
    PreviewNote { sample_index: usize, note_key: u8, volume: u8, panning: u8 },
    // Undo/redo
    Undo,
    Redo,
    // Navigation (cursor moves)
    MoveCursorUp,
    MoveCursorDown,
    MoveCursorLeft,
    MoveCursorRight,
    // ... more as needed
}
```

---

## Mutation Pattern

### with_module_mut — the standard mutation path

```rust
impl HtrkCore {
    fn ensure_module_ownership(&mut self) { /* same as current app.rs */ }

    fn with_module_mut<R>(&mut self, f: impl FnOnce(&mut Module) -> R) -> Option<R> {
        self.ensure_module_ownership();
        let result = self.module.as_mut().and_then(|arc| {
            Arc::get_mut(arc).map(f)
        });
        if result.is_some() {
            self.sync_to_audio();
            self.module_dirty = true;
        }
        result
    }

    fn sync_to_audio(&self) {
        if let Some(ref sender) = self.command_sender {
            if let Some(ref module) = self.module {
                let _ = sender.send(AudioCommand::LoadModule(module.clone()));
            }
        }
    }
}
```

### Undo-backed mutations

```rust
pub fn set_cell_at_cursor(&mut self, cell: Cell) {
    let cursor = self.cursor;
    self.ensure_module_ownership();
    if let Some(ref mut module) = self.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let cmd = SetCellCommand::new(cursor, cell);
            let _ = self.undo_manager.execute(Box::new(cmd), arc_module);
            self.sync_to_audio();
            self.module_dirty = true;
        }
    }
}
```

### Module replacement (load/new)

```rust
pub fn load_file(&mut self, path: &Path) -> Result<(), ...> {
    let module = /* load from path */;
    self.module = Some(Arc::new(module));
    self.undo_manager.clear();
    self.cursor = CursorPosition::default();
    self.selection = None;
    self.sync_channel_fields();
    self.sync_to_audio();
    self.module_dirty = false;
}
```

---

## HtrkApp → HtrkCore Interaction

```rust
pub struct HtrkApp {
    core: HtrkCore,
    // ... UI-only fields ...
}
```

### Keyboard handling — thin delegation

```rust
impl HtrkApp {
    fn handle_keyboard_input(&mut self, key_event: ...) {
        let action = self.resolve_key_to_action(key_event); // uses octave, edit_mode, etc.
        if let Some(action) = action {
            self.core.handle_edit_action(action);
        }
    }
}
```

### Widget responses — thin delegation

```rust
match response {
    GridResponse::ContextMenuAction(action) => self.core.handle_context_menu_action(action),
    // ...
}

// sendfx_editor and playback_view take &mut HtrkCore instead of &mut CommandSender
draw_sendfx_view(&mut self.core, ...);
draw_playback_view(&mut self.core, ...);
```

### Core access from UI (read-only)

- `self.core.module()` → `Option<&Module>` — for rendering
- `self.core.playback_state()` → `&Arc<AtomicPlaybackState>` — passed to transport/oscilloscope
- `self.core.cursor()` → `CursorPosition` — for scroll adjustment
- `self.core.selected_sample()` → `usize` — editor indexing

### Audio init stays in App

`init_audio` creates `cpal::Stream` + `CommandSender`, then calls `self.core.set_command_sender(sender)` and stores `playback_state`.

---

## Headless Milestone

```rust
fn test_headless_edit() {
    let mut core = HtrkCore::new(AtomicPlaybackState::new());
    core.load_file(Path::new("test.it")).unwrap();
    core.set_cell_at_cursor(Cell::default());
    assert!(core.module().unwrap().patterns[0].cell(0, 0).is_some());
    core.undo();
}
```

---

## Migration Strategy

Execute in this order, committing after each step so the app compiles at every point:

1. **Create `src/core/mod.rs`** — Define `HtrkCore` struct with all fields, `new()` constructor, `module()` / `playback_state()` accessors, and `with_module_mut` / `sync_to_audio` helpers. Wire `src/lib.rs` to include `core` module.

2. **Replace `HtrkApp` fields with `core: HtrkCore`** — Move all core-owned fields into `HtrkCore`, give `HtrkApp` a `core` field. Temporarily add `Deref`/`DerefMut` impls to `HtrkApp` targeting `HtrkCore` so all existing `self.module`, `self.cursor` etc. calls still compile.

3. **Migrate editing methods to `editing.rs`** — Move `set_cell_at_cursor`, `clear_cell_at_cursor`, `insert_row`, `delete_row`, `copy_selection`, `paste_at_cursor`, `delete_selection`, `transpose_selection`, `select_all`, and all order-list mutation methods. Remove `Deref`/`DerefMut` for these methods one at a time, fixing call sites in `app.rs` to use `self.core.`.

4. **Migrate sample/instrument/automation methods** — Move `handle_sample_edit` → `sample.rs`, `handle_instrument_edit` → `instrument.rs`, `load_instrument_from_file` → `instrument.rs`, automation methods → `automation.rs`. Fix call sites.

5. **Migrate channel and keyboard methods** — Move mute/solo/send logic → `channels.rs`. Define `EditAction` enum and `handle_edit_action` in `keyboard.rs`. Refactor `handle_keyboard_input` in `app.rs` to map keys to `EditAction` variants and call `self.core.handle_edit_action(action)`.

6. **Migrate load/save/new** — Move `load_file`, `new_song`, `save_file`, `save_current_file`, `import_wav` → `mod.rs`. Fix call sites.

7. **Migrate the 3 CommandSender widgets** — Change `transport.rs`, `sendfx_editor.rs`, `playback_view.rs` to take `&mut HtrkCore` instead of `&mut CommandSender`. Move relevant `send_command` calls inside `HtrkCore` methods.

8. **Remove `Deref`/`DerefMut`** — Once all methods are genuinely on `HtrkCore`, remove the temporary deref impls. All `self.module` in `app.rs` must now be `self.core.module()`, etc.

9. **Add headless test** — Create `tests/headless_edit.rs` that constructs `HtrkCore` without `eframe`, loads a module, performs edits, and verifies state.

10. **Clean up** — Remove dead code from `app.rs`. Verify `app.rs` is now primarily UI rendering + thin delegation.

## Risk Mitigation

- **Incremental compilation**: Every step must compile and run. The `Deref`/`DerefMut` trick ensures step 2 is a no-op in behavior.
- **Cursor in core**: Editing methods need cursor position. Moving it to core means the UI reads it via accessor and computes scroll offsets separately.
- **Widgets that take `&mut CommandSender`**: These are the trickiest migration points. The `&mut HtrkCore` approach lets them call `core.playback_command(AudioCommand::Play)` etc., with headless tests getting no-ops.
- **`init_audio` remains in App**: Audio device initialization requires OS/hardware interaction. The app creates the `CommandSender` and injects it into core.