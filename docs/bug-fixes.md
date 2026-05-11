# Bug Fixes

## XM Playback Timing Fixes (squademo.xm patterns 24, 36)
**Date:** 2026-05-11

### Symptom
XM patterns using retrigger (E9x), sample offset (9xx), note delay (EDx), and fine volume slides (EAx/EBx) had incorrect timing — retriggers bled across row boundaries, pattern delay (EEx) was ignored, and EAx/EBx effects were decoded as wrong effect types.

### Root Cause
1. **Retrigger bleed**: `retrig_speed`, `retrig_cnt`, and `last_retrigger_interval` were not reset in `advance_row()`, causing retrigger state from one row to carry into the next.
2. **XM pattern delay ignored**: The `PatternDelay` effect handler in `apply_effect_unified()` set `row_delay_active = true` only for the non-XM path; the XM branch was a no-op stub.
3. **EAx/EBx mis-mapped**: `decode_xm_extended_effect()` mapped `0xA` to `FineVolumeSlideDown` and `0xB` to `NoteCutAfter` instead of `FineVolumeSlideUp` and `FineVolumeSlideDown` respectively.

### Fix
- **sequencer_engine.rs**: Added `retrig_speed`, `retrig_cnt` resets alongside existing `last_retrigger_interval` reset in `advance_row()` (line 3039-3041).
- **sequencer_engine.rs**: Unified the XM and non-XM `PatternDelay` branches — both now set `row_delay_active = true` and `pattern_delay_ticks` (line 1006-1017).
- **xm.rs**: Corrected `decode_xm_extended_effect()` mapping: `0xA` → `FineVolumeSlideUp`, `0xB` → `FineVolumeSlideDown` (line 860-861).

### Verification
- Added `advance_row_resets_retrigger_state` unit test.
- Added `xm_pattern_delay_sets_row_delay_active` unit test.
- Added `advance_row_resets_note_cut_tick` unit test.
- Extended `decode_xm_ext_effect` test with EAx/EBx/ECx/EDx/EEx assertions.
- All 189 tests pass.

## S3M Playback Accuracy Overhaul
**Date:** 2026-05-10

### Symptom
- Pitch slides (Effects E, F, G) in S3M/MOD files sounded out of tune.
- Modules played at 50% of intended volume.
- Complex sample-slicing modules like `arsa.s3m` played with incorrect offsets or failed to loop.
- Missing effects (U, V, W, X) were ignored.

### Root Cause
1.  **Pitch Model:** The sequencer used logarithmic frequency slides for all formats, but S3M/MOD require linear-in-period slides (Amiga model).
2.  **Volume Scaling:** S3M global volume (0-64) was not scaled to the engine's internal range (0-128).
3.  **Sample Offsets:** Lack of support for the `SAx` (High Sample Offset) command prevented modules from addressing large samples. Additionally, sample offsets were incorrectly persistent across notes.
4.  **Timing:** Format-specific effects (Portamento, Volume Slide) were being applied on Tick 0 for non-XM modules, leading to double-speed slides.

### Fix
- **Sequencer Engine:**
  - Implemented period-based pitch slides for non-XM formats in `apply_portamento_up/down` and `apply_tone_portamento`.
  - Scaled S3M pitch slides by 4x to match ST3 internal 16-bit resolution.
  - Restricted pitch and volume slides to start from Tick 1 for non-XM formats.
  - Added support for `SAx` High Sample Offset and unified the `calculate_sample_offset` logic.
  - Fixed Oxx persistence: offsets are now 0 unless an offset effect is present on the current row or `O00` is used.
- **S3M Loader:**
  - Implemented missing effects: `U` (Fine Vibrato), `V` (Global Volume), `W` (Global Volume Slide), `X` (Set Panning).
  - Scaled initial global volume and panning values correctly.
  - Fixed mis-mapped S-commands (SA, SB, SC, SD, SE).
  - Distingished between Fine and Extra-Fine portamento ranges.

### Verification
- Verified compilation with `cargo check`.
- Verified fixes against `arsa.s3m` behaviors (High Sample Offset, Amiga periods).
- All existing sequencer unit tests pass.

## MOD Positional Commands Fix
**Date:** 2026-05-09

### Symptom
.MOD playback can get stuck in infinite repeating loops or skip to incorrect positions when positional commands are used.

### Root Cause
1.  **Effect Dxx (Pattern Break):** The row was decoded as BCD (binary-coded decimal), but MOD files use plain binary. For example, D1E (row 30) decoded as (1×10 + 14) = 24 instead of 30.
2.  **Effect E6x (Pattern Loop):** The format check was inverted. The condition `if !is_xm` incorrectly skipped the effect for MOD/S3M formats and only processed it for non-MOD (XM). Since MOD and S3M support pattern loops but XM doesn't, this caused loops to be ignored in MOD files.

### Fix
- **modfile.rs:** Changed row calculation in `convert_effect` from BCD `(param >> 4) * 10 + (param & 0x0F)` to plain binary `param`.
- **sequencer_engine.rs:** Changed condition in `PatternLoop` handling from `if !is_xm` to `if is_xm` to skip only for XM, allowing MOD/S3M to use pattern loops.

### Verification
- Added `mod_pattern_loop_sets_loop_start` unit test.
- Added `mod_pattern_loop_executes_loop` unit test.
- Updated `convert_effect_pattern_break` test to use correct binary value.
- All 176 library tests pass.

## Playback Stall and Timing Accuracy Fix
**Date:** 2026-05-09

### Symptom
When attempting to play a song, the sequencer would advance exactly one row, the VU meters would flicker, and then playback would stop. The application remained responsive.

### Root Cause
1.  **UI Feedback Loop:** The Pattern Order panel in `src/app.rs` was calling `sync_module_to_audio()` on every frame. `sync_module_to_audio()` sends `AudioCommand::LoadModule`, which causes the audio engine to stop playback for safety.
2.  **Audio Engine Latency:** The `process_callback` loop processed ticks *after* mixing samples for that tick, introducing a 1-tick delay for all notes and effects.
3.  **Timing Inaccuracy:** The sample counter was reset to 0 on each tick instead of using a subtractive approach, leading to cumulative timing errors from fractional samples.

### Fix
- **UI:** Wrapped the `sync_module_to_audio()` call in the order list panel with a `changed` flag. It now only triggers when the user actually modifies the order list or pattern indices.
- **Audio Engine:**
  - Refactored the `process_callback` loop to trigger `sequencer.process_tick()` at the **beginning** of the tick.
  - Used `sample_counter -= samples_per_tick` to maintain fractional precision.
- **Sequencer Engine:**
  - Refactored `play()` and `play_from()` to initialize the state for immediate tick processing in the next callback.
  - Removed redundant manual `process_tick_zero_unified()` calls during initialization.

### Verification
- Verified that playback now continues correctly across multiple rows and patterns.
- Notes trigger with sample-accurate precision.
- All 174 library tests pass.
**Date:** 2026-05-06

### Symptom
Operations like importing a WAV sample, undo/redo, inserting rows, and order list edits would silently fail whenever the audio engine thread held a reference to the `Arc<Module>`. `Arc::get_mut()` returns `None` when `Arc::strong_count > 1`, and all call sites treated this as a no-op.

### Root Cause
`Arc::get_mut()` only succeeds when there is a single owner. The audio engine always holds a clone of `Arc<Module>` (sent via `AudioCommand::LoadModule`), so `strong_count` is always >= 2 during playback — and often even when stopped if the audio thread hasn't released its copy yet.

A second bug compounded this: the initial `ensure_module_ownership()` fix sent `LoadModule` immediately after cloning, but the command channel then held a reference to the new `Arc`, keeping `strong_count` at 2. The subsequent `Arc::get_mut()` would still return `None`, so mutations silently failed.

### Fix
- Added `ensure_module_ownership()` method that checks `Arc::strong_count()`. If count > 1, it clones the module data into a fresh `Arc` (with count 1) and replaces `self.module`. It does **not** send `LoadModule` at this point.
- Added `sync_module_to_audio()` method called **after** each mutation to send `LoadModule` with the (now mutated) Arc. This ensures the audio engine receives the updated module.
- All 15 mutation sites now follow the pattern: `ensure_module_ownership()` → `Arc::get_mut()` → mutate → `sync_module_to_audio()`.
- This guarantees mutations always succeed (strong_count is 1 after ensure_module_ownership) and the audio engine always receives updates.

### Verification
- All 174 existing unit tests pass.
- WAV import now works correctly even during playback.

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
