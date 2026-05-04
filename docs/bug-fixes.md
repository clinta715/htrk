# Bug Fixes

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
