# Module Mut Helper & Event Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move the event-stripping cleanup from `draw_preamble` into the keyboard handler, and add a `with_module_mut` helper to eliminate repeated ensure/get_mut/sync boilerplate.

**Architecture:** Pure refactoring — move code, add one method, replace call sites. No behavioral changes.

**Tech Stack:** Rust

---

### Task 1: Move event cleanup from `draw_preamble` to `handle_keyboard_input`

**Files:**
- Modify: `src/app.rs` (remove block)
- Modify: `src/actions/keyboard.rs` (add block)

- [ ] **Step 1: Remove cleanup from `draw_preamble`**

In `src/app.rs`, find:

```rust
        if self.current_view == AppView::Pattern && ctx.memory(|m| m.focused().is_none()) && !self.any_dialog_open() {
            ctx.input_mut(|i| {
                i.events.retain(|e| !matches!(e,
                    egui::Event::Key { key: egui::Key::Tab, pressed: true, .. }
                    | egui::Event::Key { key: egui::Key::ArrowUp, pressed: true, .. }
                    | egui::Event::Key { key: egui::Key::ArrowDown, pressed: true, .. }
                    | egui::Event::Key { key: egui::Key::ArrowLeft, pressed: true, .. }
                    | egui::Event::Key { key: egui::Key::ArrowRight, pressed: true, .. }
                ));
            });
        }
```

Delete the entire block.

- [ ] **Step 2: Add cleanup as last step in `handle_keyboard_input`**

In `src/actions/keyboard.rs`, after the `handle_plain_key(app, ctx, ...)` call and before the closing `}`, add:

```rust
    // Strip Tab/Arrow keys from the event queue so egui's widget system doesn't
    // interpret them as focus-navigation or scroll commands. The handlers above
    // processed these events by value but did not consume them from the queue.
    if is_pattern && !has_focus && !any_dialog_open {
        ctx.input_mut(|i| {
            i.events.retain(|e| !matches!(e,
                egui::Event::Key { key: egui::Key::Tab, pressed: true, .. }
                | egui::Event::Key { key: egui::Key::ArrowUp, pressed: true, .. }
                | egui::Event::Key { key: egui::Key::ArrowDown, pressed: true, .. }
                | egui::Event::Key { key: egui::Key::ArrowLeft, pressed: true, .. }
                | egui::Event::Key { key: egui::Key::ArrowRight, pressed: true, .. }
            ));
        });
    }
```

- [ ] **Step 3: Build and test**

Run: `cargo test --lib` — 393 passed

- [ ] **Step 4: Commit**

```bash
git add src/app.rs src/actions/keyboard.rs
git commit -m "refactor: move event cleanup from draw_preamble into keyboard handler"
```

---

### Task 2: Add `with_module_mut` method to `HtrkCore`

**Files:**
- Modify: `src/core/mod.rs`

- [ ] **Step 1: Add the method**

Find the `impl HtrkCore` block in `src/core/mod.rs`. Add after `sync_module_to_audio`:

```rust
    /// Execute a mutation on the loaded module, handling ownership and sync.
    /// Returns `Some(result)` if the module was present, `None` otherwise.
    pub fn with_module_mut<F, R>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Module) -> R,
    {
        self.ensure_module_ownership();
        let result = self.module.as_mut().and_then(|a| Arc::get_mut(a)).map(f);
        self.sync_module_to_audio();
        result
    }
```

- [ ] **Step 2: Build check**

Run: `cargo check` — should succeed with 2 pre-existing clack warnings

- [ ] **Step 3: Commit**

```bash
git add src/core/mod.rs
git commit -m "feat: add HtrkCore::with_module_mut helper"
```

---

### Task 3: Replace call sites in `src/actions/`

- [ ] **Step 1: Find all eligible call sites in `src/actions/`**

Run: `rg -n 'ensure_module_ownership' src/actions/` to find all pattern occurrences.

For each file, look for the 4-step pattern and replace with `with_module_mut`.

**Pattern to find:**
```rust
core.ensure_module_ownership();
if let Some(ref mut module) = core.module {
    if let Some(arc_module) = Arc::get_mut(module) {
        // mutation
    }
}
core.sync_module_to_audio();
```

**Replace with:**
```rust
core.with_module_mut(|arc_module| {
    // mutation (same body)
});
```

**Files to check:** `src/actions/keyboard.rs`, `src/actions/file_io.rs`, `src/actions/sample_edit.rs`

For each call site:
1. Verify it's a simple 4-step pattern (no early returns, no branching that skips sync)
2. Replace with `with_module_mut`
3. Remove the `ensure_module_ownership` and `sync_module_to_audio` lines
4. If `Arc` was only imported for this usage, remove unused import

- [ ] **Step 2: Build after each file**

`cargo check` after each file to catch any issues.

- [ ] **Step 3: Test**

`cargo test --lib` — 393 passed

- [ ] **Step 4: Commit**

```bash
git add src/actions/
git commit -m "refactor: use with_module_mut in actions/"
```

---

### Task 4: Replace call sites in `src/app.rs`

- [ ] **Step 1: Find all eligible call sites**

Run: `rg -n 'ensure_module_ownership' src/app.rs`

For each call site, check if it's the simple 4-step pattern:
```rust
self.core.ensure_module_ownership();
if let Some(ref mut module) = self.core.module {
    if let Some(arc_module) = Arc::get_mut(module) {
        ...
    }
}
self.core.sync_module_to_audio();
```

Replace with:
```rust
self.core.with_module_mut(|arc_module| {
    ...
});
```

Skip call sites where:
- The mutation is inside a conditional that also modifies non-module state (e.g., `if cond { mutate; other_stuff; }`)
- The code inspects the result of `Arc::get_mut` before deciding what to do
- The code has early returns or complex control flow

- [ ] **Step 2: Build and test after replacing a batch**

Run: `cargo check` and `cargo test --lib` — 393 passed

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "refactor: use with_module_mut in app.rs"
```

---

### Task 5: Replace call sites in remaining `src/` files

- [ ] **Step 1: Find all remaining call sites**

Run: `rg -n 'ensure_module_ownership' src/ --include='*.rs'` and exclude already-processed files.

Check files like:
- `src/ui/` pattern_editor, sendfx_editor, instrument_editor, etc.
- `src/edit/` commands
- `src/mcp/mutations.rs`

Same replacement pattern as Task 3.

- [ ] **Step 2: Build and test after each file**

`cargo check` then `cargo test --lib`

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor: use with_module_mut in remaining call sites"
```

---

### Task 6: Final verification

- [ ] **Step 1: Full test suite**

```bash
cargo test --lib 2>&1 | tail -3
cargo test --test mcp_integration 2>&1 | tail -5
```

- [ ] **Step 2: Verify no warnings**

```bash
cargo build 2>&1 | grep -E "warning|error"
```

Only the 2 pre-existing clack `FileTypeOwned` warnings should appear.

- [ ] **Step 3: Push**

```bash
git push origin main
```
