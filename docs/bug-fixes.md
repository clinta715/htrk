# Bug Fixes

## Arc::get_mut Silent Failure on Module Mutation
**Date:** 2026-05-06

### Symptom
Operations like importing a WAV sample, undo/redo, inserting rows, and order list edits would silently fail whenever the audio engine thread held a reference to the `Arc<Module>`. `Arc::get_mut()` returns `None` when `Arc::strong_count > 1`, and all call sites treated this as a no-op.

### Root Cause
`Arc::get_mut()` only succeeds when there is a single owner. The audio engine always holds a clone of `Arc<Module>` (sent via `AudioCommand::LoadModule`), so `strong_count` is always >= 2 during playback — and often even when stopped if the audio thread hasn't released its copy yet.

### Fix
- Added `ensure_module_ownership()` method to `HtrkApp` that checks `Arc::strong_count()`.
- If count > 1, it clones the module data, creates a new `Arc`, and sends `AudioCommand::LoadModule` to the audio engine with the new copy.
- All former `Arc::get_mut()` call sites (15 locations) replaced with `ensure_module_ownership()` followed by `Arc::get_mut()`.
- This guarantees mutations always succeed.

### Verification
- All 174 existing unit tests pass.
- Manual verification: WAV import works while playing.

## cry4bass.mod - Pattern 8, Row 30: Looping Sample Fix
**Date:** 2026-05-04

### Symptom
In pattern 8, row 30 of `cry4bass.mod`, a sample would start looping/retriggering indefinitely instead of playing silence or ending correctly.

### Root Cause Analysis
1.  **State Carryover**: The `last_retrigger_interval` (from effect `E9x` / `Rxy`) and `active_effects` flags were not being reset in `advance_row`. Row 29 contained a retrigger effect that "leaked" into row 30.
2.  **NoteDelay (EDx) Implementation**: The engine only handled `NoteDelay` by setting a `delay_tick` on an *existing* active voice. If a channel was empty on tick 0, the delayed note would never trigger.
3.  **Volume Reset missing**: When a delayed note triggered with a new instrument, it failed to reset to the sample's default volume, inheriting stale volume slides from previous rows.

### Fix
- Modified `SequencerEngine::advance_row` to explicitly reset per-row channel state:
  - `active_effects = ActiveEffects::default()`
  - `last_retrigger_interval = 0`
  - `note_cut_tick = None`
  - `eff_typ_xm = 0xFF`
  - `vol_kol = 0`
- Refactored `NoteDelay` to store the `Cell` in `channel.delayed_cell` and check `channel.note_delay_ticks` independently of active voices.
- Added volume reset logic to `trigger_delayed_note` when an instrument is present.

### Verification
- Added `advance_row_resets_effects` unit test.
- Added `note_delay_stores_cell` unit test.
- Verified pattern 8 row 30 data structure in `cry4bass.mod` via `analyze_mod`.
