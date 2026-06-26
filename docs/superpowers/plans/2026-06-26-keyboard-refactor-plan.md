# Keyboard Handler Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) for syntax tracking.

**Goal:** Deduplicate `any_dialog_open` and break the 1173-line `handle_keyboard_input` into focused sub-handlers.

**Architecture:** Add `HtrkApp::any_dialog_open()` method, replace two inline computations with calls to it. Extract 5 sub-functions from the keyboard handler; main function becomes a dispatcher.

**Tech Stack:** Rust, egui

---

### Task 1: Add `any_dialog_open()` method to HtrkApp

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add the method**

Find the impl block for `HtrkApp` (search for `impl HtrkApp {` — there is one at ~line 2793). Add the method just before `fn module_dirty`:

```rust
    pub fn any_dialog_open(&self) -> bool {
        self.file_browser.show
            || self.settings_state.open
            || self.wav_export_state.open
            || self.sample_export_dialog.is_some()
            || self.show_about
            || self.show_shortcuts
            || self.show_exit_confirm
            || self.show_phrase_generator
            || self.slice_dialog_open
            || self.sendfx_panel.plugin_browser_open_for.is_some()
            || self.instrument_editor.plugin_browser_open
            || self.sample_library_state.open
    }
```

- [ ] **Step 2: Build check**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "refactor: add HtrkApp::any_dialog_open() method"
```

---

### Task 2: Replace inline computation in `draw_preamble`

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Replace the inline dialog check**

Find the block at lines ~1247–1257:

```rust
        let any_dialog_open = self.file_browser.show
            || self.settings_state.open
            ...;
        if self.current_view == AppView::Pattern && ctx.memory(|m| m.focused().is_none()) && !any_dialog_open {
```

Replace the entire `let any_dialog_open = ...;` assignment and the following `if` with:

```rust
        if self.current_view == AppView::Pattern && ctx.memory(|m| m.focused().is_none()) && !self.any_dialog_open() {
```

- [ ] **Step 2: Build check**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "refactor: use any_dialog_open() in draw_preamble event guard"
```

---

### Task 3: Replace inline computation in `handle_keyboard_input`

**Files:**
- Modify: `src/actions/keyboard.rs`

- [ ] **Step 1: Replace the inline `any_dialog_open` computation**

Find lines 53–64:

```rust
    let any_dialog_open = app.file_browser.show
        || app.settings_state.open
        || app.wav_export_state.open
        || app.sample_export_dialog.is_some()
        || app.show_about
        || app.show_shortcuts
        || app.show_exit_confirm
        || app.show_phrase_generator
        || app.slice_dialog_open
        || app.sendfx_panel.plugin_browser_open_for.is_some()
        || app.instrument_editor.plugin_browser_open
        || app.sample_library_state.open;
```

Replace with:

```rust
    let any_dialog_open = app.any_dialog_open();
```

- [ ] **Step 2: Build check**

Run: `cargo check`

- [ ] **Step 3: Commit**

```bash
git add src/actions/keyboard.rs
git commit -m "refactor: use any_dialog_open() in keyboard handler"
```

---

### Task 4: Extract `handle_early_text` from `handle_keyboard_input`

**Files:**
- Modify: `src/actions/keyboard.rs`

- [ ] **Step 1: Replace inline code with function call**

Find lines 66–79 in `handle_keyboard_input` (the `// Text events` comment block):

```rust
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
```

Replace with:

```rust
    handle_early_text(app, ctx, has_focus, any_dialog_open);
```

- [ ] **Step 2: Add the extracted function**

Before the `note_key_preview_only` function (before line ~652 in current code), add:

```rust
/// Text events: processed unconditionally so note preview works even during dialog input.
/// When a widget has focus or a dialog is open, only play audio; skip cell editing.
fn handle_early_text(app: &mut HtrkApp, ctx: &egui::Context, has_focus: bool, any_dialog_open: bool) {
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
}
```

- [ ] **Step 3: Build and test**

Run: `cargo test --lib` — expect 393 passed

- [ ] **Step 4: Commit**

```bash
git add src/actions/keyboard.rs
git commit -m "refactor: extract handle_early_text from handle_keyboard_input"
```

---

### Task 5: Extract `handle_tab` from `handle_keyboard_input`

**Files:**
- Modify: `src/actions/keyboard.rs`

- [ ] **Step 1: Replace inline Tab interception with function call**

Find lines 82–147 in `handle_keyboard_input` (the `// Tab interception` comment block):

```rust
    // Tab interception: in the pattern editor, Tab always changes columns...
    if is_pattern && !any_dialog_open && app.edit_mode {
        let mut tab_pressed = false;
        let mut shift_pressed = false;
        // ... ~65 lines of event handling ...
        if tab_pressed {
            // ... focus surrender + cursor movement ...
        }
    }
```

Replace with:

```rust
    handle_tab(app, ctx, is_pattern, any_dialog_open);
```

- [ ] **Step 2: Add the extracted function**

After `handle_early_text` (at the module level, before `note_key_preview_only`), add:

```rust
/// Tab interception: in the pattern editor, Tab always changes columns — it must never
/// escape to egui's focus-navigation. Handle it before the focus gate.
fn handle_tab(app: &mut HtrkApp, ctx: &egui::Context, is_pattern: bool, any_dialog_open: bool) {
    if is_pattern && !any_dialog_open && app.edit_mode {
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
}
```

- [ ] **Step 3: Build and test**

Run: `cargo test --lib` — expect 393 passed

- [ ] **Step 4: Commit**

```bash
git add src/actions/keyboard.rs
git commit -m "refactor: extract handle_tab from handle_keyboard_input"
```

---

### Task 6: Extract `handle_ctrl` and `handle_ctrl_shift`

**Files:**
- Modify: `src/actions/keyboard.rs`

- [ ] **Step 1: Add `handle_ctrl` function**

After `handle_tab`, add:

```rust
/// Handle Ctrl+ shortcuts (undo, redo, copy, paste, etc.)
/// Returns true if a shortcut was handled.
fn handle_ctrl(app: &mut HtrkApp, ctx: &egui::Context, any_dialog_open: bool, is_pattern: bool, is_sample: bool) -> bool {
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
                    egui::Key::C if !any_dialog_open => {
                        if is_sample {
                            crate::actions::sample_edit::handle_sample_edit(app, SampleEditEvent::Copy);
                        } else if app.edit_mode && is_pattern {
                            app.core.copy_selection();
                        }
                        handled = true;
                    }
                    egui::Key::X if !any_dialog_open => {
                        if is_sample {
                            crate::actions::sample_edit::handle_sample_edit(app, SampleEditEvent::Cut);
                        } else if app.edit_mode && is_pattern {
                            app.cut_selection();
                        }
                        handled = true;
                    }
                    egui::Key::V if !any_dialog_open => {
                        if is_sample {
                            crate::actions::sample_edit::handle_sample_edit(app, SampleEditEvent::Paste);
                        } else if app.edit_mode && is_pattern {
                            app.core.paste_at_cursor();
                        }
                        handled = true;
                    }
                    egui::Key::A if !any_dialog_open => {
                        if is_sample {
                            crate::actions::sample_edit::handle_sample_edit(app, SampleEditEvent::SelectAll);
                        } else if app.edit_mode && is_pattern {
                            app.core.select_all();
                        }
                        handled = true;
                    }
                    egui::Key::D if app.edit_mode && !any_dialog_open => {
                        if is_pattern {
                            app.duplicate_selection();
                        }
                        handled = true;
                    }
                    egui::Key::E if app.edit_mode && is_pattern && !any_dialog_open => {
                        app.core.fill_pattern();
                        handled = true;
                    }
                    egui::Key::F if app.edit_mode && is_pattern && !any_dialog_open => {
                        app.fill_instrument();
                        handled = true;
                    }
                    egui::Key::G if app.edit_mode && is_pattern && !any_dialog_open => {
                        app.show_phrase_generator = true;
                        handled = true;
                    }
                    egui::Key::I if is_pattern && !any_dialog_open => {
                        app.show_phrase_generator = true;
                        handled = true;
                    }
                    egui::Key::O => {
                        if !any_dialog_open {
                            app.file_browser = crate::ui::file_browser::FileBrowser::open_browser(
                                crate::ui::file_browser::BrowserMode::Modules,
                                app.config.default_mod_path.clone().map(PathBuf::from),
                            );
                            app.file_browser.show = true;
                        }
                        handled = true;
                    }
                    egui::Key::L if is_pattern && !any_dialog_open => {
                        app.core.ensure_pattern_exists();
                        app.core.ensure_module_ownership();
                        if let Some(ref mut module) = app.core.module {
                            if let Some(arc_module) = Arc::get_mut(module) {
                                let pat_idx = app.core.selected_pattern();
                                if pat_idx < arc_module.patterns.len() {
                                    arc_module.patterns[pat_idx].num_rows = arc_module.patterns[pat_idx].num_rows.wrapping_add(1);
                                }
                            }
                        }
                        app.core.sync_module_to_audio();
                        handled = true;
                    }
                    egui::Key::M if is_pattern && !any_dialog_open => {
                        app.core.selection = None;
                        app.core.cursor.row = 0;
                        app.ensure_cursor_visible();
                        handled = true;
                    }
                    egui::Key::W if !any_dialog_open => {
                        app.core.send_command(crate::audio::commands::AudioCommand::Stop);
                        handled = true;
                    }
                    _ => {}
                }
            }
        }
    });
    handled
}
```

- [ ] **Step 2: Replace the Ctrl block in `handle_keyboard_input`**

Find lines ~154–193:

```rust
    if modifiers.ctrl && !modifiers.shift {
        let mut handled = false;
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Key { key, pressed: true, .. } = event {
                    match key {
                        egui::Key::Z if app.edit_mode && !any_dialog_open => { ... }
                        ...
                    }
                }
            }
        });
        if handled { return; }
    }
```

Replace with:

```rust
    if modifiers.ctrl && !modifiers.shift {
        if handle_ctrl(app, ctx, any_dialog_open, is_pattern, is_sample) {
            return;
        }
    }
```

- [ ] **Step 3: Now add `handle_ctrl_shift` function**

After `handle_ctrl`, add:

```rust
/// Handle Ctrl+Shift+ shortcuts (redo via Ctrl+Shift+Z, etc.)
fn handle_ctrl_shift(app: &mut HtrkApp, ctx: &egui::Context, any_dialog_open: bool, is_pattern: bool) -> bool {
    let mut handled = false;
    ctx.input(|i| {
        for event in &i.events {
            if let egui::Event::Key { key, pressed: true, .. } = event {
                match key {
                    egui::Key::Z if app.edit_mode && !any_dialog_open => {
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
                        app.show_shortcuts = !app.show_shortcuts;
                        handled = true;
                    }
                    egui::Key::V if is_pattern && !any_dialog_open => {
                        app.core.ensure_pattern_exists();
                        app.core.ensure_module_ownership();
                        if let Some(ref mut module) = app.core.module {
                            if let Some(arc_module) = Arc::get_mut(module) {
                                let pat_idx = app.core.selected_pattern();
                                if pat_idx < arc_module.patterns.len() {
                                    arc_module.patterns[pat_idx].num_rows = arc_module.patterns[pat_idx].num_rows.wrapping_sub(1).max(1);
                                }
                            }
                        }
                        app.core.sync_module_to_audio();
                        handled = true;
                    }
                    _ => {}
                }
            }
        }
    });
    handled
}
```

- [ ] **Step 4: Replace the Ctrl+Shift block in `handle_keyboard_input`**

Find lines ~194–260:

```rust
    if modifiers.ctrl && modifiers.shift {
        ...
    }
```

Replace with:

```rust
    if modifiers.ctrl && modifiers.shift {
        if handle_ctrl_shift(app, ctx, any_dialog_open, is_pattern) {
            return;
        }
    }
```

- [ ] **Step 5: Add `PathBuf` import**

At the top of the file (after `use std::sync::Arc;` at line 1), add:

```rust
use std::path::PathBuf;
```

Check if it's already imported — `cargo check` will tell us (it might already be brought in via another path). Remove it if not needed.

- [ ] **Step 6: Build and test**

Run: `cargo test --lib` — expect 393 passed

- [ ] **Step 7: Commit**

```bash
git add src/actions/keyboard.rs
git commit -m "refactor: extract handle_ctrl and handle_ctrl_shift"
```

---

### Task 7: Extract `handle_plain_key`

**Files:**
- Modify: `src/actions/keyboard.rs`

- [ ] **Step 1: Read lines ~265–270 to confirm the boundary**

Read the code around lines 260–290 of `src/actions/keyboard.rs` to find the transition between the Ctrl+Shift block and the plain key block. Confirm the plain key code is not wrapped in another `if modifiers.ctrl` guard.

- [ ] **Step 2: If there's an `if !modifiers.ctrl` guard wrapping the plain key block, extract it. Otherwise extract the raw `ctx.input(|i| { ... })` block.**

The extracted function signature should be:

```rust
/// Handle all other key events (arrows, function keys, Escape, Delete, etc.)
fn handle_plain_key(app: &mut HtrkApp, ctx: &egui::Context, any_dialog_open: bool, is_pattern: bool, is_sample: bool, modifiers: egui::Modifiers) {
```

Move the `ctx.input(|i| { for event in &i.events { ... } })` block and its surrounding guard into this function.

- [ ] **Step 3: Build and test**

Run: `cargo test --lib` — expect 393 passed

- [ ] **Step 4: Commit**

```bash
git add src/actions/keyboard.rs
git commit -m "refactor: extract handle_plain_key from handle_keyboard_input"
```

---

### Task 8: Clean up — rewrite `handle_keyboard_input` as dispatcher

**Files:**
- Modify: `src/actions/keyboard.rs`

- [ ] **Step 1: Verify the main function has been reduced to a clean dispatcher**

After all extractions, `handle_keyboard_input` should look like:

```rust
pub(crate) fn handle_keyboard_input(app: &mut HtrkApp, ctx: &egui::Context) {
    let is_pattern = app.current_view == AppView::Pattern;
    let is_sample = app.current_view == AppView::Sample;
    let modifiers = ctx.input(|i| i.modifiers);
    let has_focus = ctx.memory(|m| m.focused().is_some());
    let any_dialog_open = app.any_dialog_open();

    // Text events: note preview always works; cell editing is conditional on dialog/focus.
    handle_early_text(app, ctx, has_focus, any_dialog_open);

    // Tab: always changes columns in pattern editor; never escapes to egui's focus system.
    handle_tab(app, ctx, is_pattern, any_dialog_open);

    // Focus gate: when a widget has focus, skip all key handling (except the Event::Text
    // pre-pass above, which already ran).
    if has_focus {
        return;
    }

    // Ctrl+ shortcuts: undo, redo, copy, paste, etc.
    if modifiers.ctrl && !modifiers.shift {
        if handle_ctrl(app, ctx, any_dialog_open, is_pattern, is_sample) {
            return;
        }
    }

    // Ctrl+Shift+ shortcuts: redo, etc.
    if modifiers.ctrl && modifiers.shift {
        if handle_ctrl_shift(app, ctx, any_dialog_open, is_pattern) {
            return;
        }
    }

    // All remaining keys: arrows, F-keys, Delete, Escape, brackets, Alt+ combos, etc.
    handle_plain_key(app, ctx, any_dialog_open, is_pattern, is_sample, modifiers);
}
```

Compare with the actual state and adjust if any function signature needs tweaking.

- [ ] **Step 2: Run full test suite**

Run: `cargo test --lib` — expect 393 passed
Run: `cargo test --test mcp_integration` — expect 6 passed

- [ ] **Step 3: Commit**

```bash
git add src/actions/keyboard.rs
git commit -m "refactor: rewrite handle_keyboard_input as dispatcher"
```

- [ ] **Step 4: Push**

```bash
git push origin main
```
