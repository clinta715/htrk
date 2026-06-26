# Keyboard Handler Refactoring

**Date:** 2026-06-26
**Status:** Approved

## Summary

Two targeted refactors: (1) deduplicate `any_dialog_open` into a method, (2) break the 1173-line `handle_keyboard_input` into focused sub-handlers.

## Changes

### 1. `HtrkApp::any_dialog_open()` method

Add a method to `HtrkApp` that checks all dialog-open flags:

```rust
impl HtrkApp {
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
}
```

Replace:
- `keyboard.rs:53-64` — compute `any_dialog_open` local from `app.any_dialog_open()`
- `app.rs:1241-1252` — use `app.any_dialog_open()` instead of inline computation

### 2. Decompose `handle_keyboard_input`

Split the 1173-line function into a dispatcher plus extracted helpers. The main function becomes:

```
fn handle_keyboard_input(app, ctx):
    compute is_pattern, is_sample, modifiers, has_focus, any_dialog_open
    handle_early_text(app, ctx, any_dialog_open)
    handle_tab(app, ctx, is_pattern, any_dialog_open)
    if has_focus: return
    handle_ctrl(app, ctx, any_dialog_open, is_pattern, is_sample)
    handle_ctrl_shift(app, ctx, any_dialog_open, is_pattern)
    handle_plain_key(app, ctx, any_dialog_open, is_pattern, is_sample)
```

Each extracted helper:

| Function | Source lines | Parameters | Notes |
|---|---|---|---|
| `handle_early_text` | 66–79 | `app, ctx, any_dialog_open` | Event::Text pre-pass for note preview |
| `handle_tab` | 82–147 | `app, ctx, is_pattern, any_dialog_open` | Tab/Shift+Tab column navigation |
| `handle_ctrl` | 154–193 | `app, ctx, any_dialog_open, is_pattern, is_sample` | Ctrl+Z/Y/C/X/V/A/D/E/F/G/I/O/L/M/W |
| `handle_ctrl_shift` | 194–260 | `app, ctx, any_dialog_open, is_pattern` | Ctrl+Shift+Z/C/V |
| `handle_plain_key` | 270–647 | `app, ctx, any_dialog_open, is_pattern, is_sample` | All other keys |

No behavioral changes. No new dependencies. Each helper is `fn()` at module scope, not methods on `HtrkApp`.

## Files Changed

- `src/app.rs` — add `any_dialog_open()` method, update `draw_preamble`
- `src/actions/keyboard.rs` — add 5 extracted functions, rewrite `handle_keyboard_input` as dispatcher

## Testing

`cargo test --lib` — all 393 existing tests must pass unchanged.
