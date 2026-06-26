# Module Mut Helper & Event Cleanup Refactoring

**Date:** 2026-06-26

## Summary

Two refactors: (1) move the Tab/Arrow event-stripping hack from `draw_preamble` into `handle_keyboard_input`, (2) introduce `HtrkCore::with_module_mut` to eliminate the 4-step `ensure → get_mut → mutate → sync` boilerplate.

## Changes

### #3: Event cleanup location

**Problem:** `draw_preamble` (`app.rs:1268`) strips Tab/Arrow events from the queue after `handle_keyboard_input` returns. This is keyboard-logic leaking into the app layer.

**Fix:** Move the `ctx.input_mut(|i| { i.events.retain(...) })` block from `draw_preamble` into `handle_keyboard_input` as the final dispatch step. Condition: `is_pattern && !has_focus && !any_dialog_open`. Uses variables already computed at function entry.

**Files:** `src/app.rs` (remove ~12 lines), `src/actions/keyboard.rs` (add ~10 lines)

### #4: `with_module_mut` helper

**Problem:** The `ensure_module_ownership → if let Some(ref mut module) → Arc::get_mut → mutate → sync_module_to_audio` pattern is copy-pasted ~40 times, making mutations error-prone.

**Fix:** Add method to `HtrkCore`:

```rust
pub fn with_module_mut<F, R>(&mut self, f: F) -> Option<R>
where F: FnOnce(&mut Module) -> R
```

Then replace all eligible call sites. A call site is eligible when:
- It does the full 4-step pattern (ensure + get_mut + mutate + sync)
- The closure is a single straight-line mutation (no early returns, no complex control flow)
- It doesn't need to inspect the arc_module before/after outside the closure

Call sites that need special handling (e.g., mutate inside a conditional that also affects other state, or mutations that need the result of `Arc::get_mut` before deciding to sync) are left as-is.

**Files:** `src/core/mod.rs` (add method), grep and replace in all eligible call sites across `src/`.

## Testing

`cargo test --lib` — all 393 existing tests must pass unchanged.
`cargo build` — zero new warnings.
