# Codebase Analysis & Optimization Report

This report outlines identified bugs, performance bottlenecks, and safety concerns in the `htrk` codebase.

## 1. Identified Bugs

### Critical: Pattern Row Overflow
- **File:** `src/sequencer/player.rs`, `src/audio/sequencer_engine.rs`
- **Symptom:** Sequencer loops back to row 0 after row 255.
- **Root Cause:** `SequencerState::current_row` is a `u8`, but patterns support up to 1024 rows.
- **Impact:** Songs with patterns longer than 256 rows will not play correctly beyond the 256th row.
- **Related:** `pattern_break_row` and `pattern_loop_start` also use `u8` and suffer from the same limitation.

### Moderate: Pattern Index Overflow
- **File:** `src/sequencer/player.rs`
- **Symptom:** Wrap-around to pattern 0 when accessing pattern 256.
- **Root Cause:** `SequencerState::current_pattern` is a `u8`, while `MAX_PATTERNS` is defined as 256.

### Moderate: Audio Thread Mutation Desync
- **File:** `src/audio/engine.rs`
- **Symptom:** Persistent module data is not updated when effects like `SetSendPreFader` are triggered via audio commands.
- **Root Cause:** The audio thread attempts to use `Arc::get_mut` on the `Module`, which fails because the UI thread holds a reference. Only the local engine state is updated, not the underlying `Module`.

## 2. Optimization Opportunities

### High: Real-time Thread Allocations
- **File:** `src/audio/engine.rs`
- **Concern:** Frequent heap allocations in the `process_callback` (real-time thread).
- **Specifics:** 
    - `capture_monitoring` allocates `Vec<f32>` every callback.
    - `process_callback` allocates `Vec<bool>` for muted/solo channels multiple times per callback.
    - `LoadModule` processing re-allocates nested `Vec<Vec<f32>>` buffers.
- **Impact:** Potential audio glitches due to non-deterministic allocation timing.

### Medium: Buffer Indirection
- **File:** `src/audio/engine.rs`
- **Concern:** Use of `Vec<Vec<f32>>` for mixing buffers (`ch_mix_left`, etc.).
- **Impact:** Double indirection and poor cache locality. Flattening into a single `Vec<f32>` would improve performance.

### Low: Unnecessary Arc Cloning
- **File:** `src/audio/sequencer_engine.rs`
- **Concern:** `evaluate_automation` clones the `Module` Arc on every tick.
- **Impact:** Minor overhead; could be replaced with a reference.

### Low: Redundant UI Calculations
- **File:** `src/ui/pattern_grid.rs`
- **Concern:** `sample_length_bg` performs floating-point calculations inside nested loops for every visible cell.
- **Impact:** Unnecessary UI thread overhead.

### Low: Timing Drift in Headless/Test Mode
- **File:** `src/audio/sequencer_engine.rs`
- **Concern:** `advance` method resets `sample_counter` to `0.0` instead of using subtractive adjustment.
- **Impact:** Potential drift in long renders compared to real-time playback.

## 3. Safety & Hygiene

### Unchecked Unwraps in Real-time Thread
- **File:** `src/audio/sequencer_engine.rs`, `src/audio/effects/xm.rs`
- **Concern:** Multiple `.unwrap()` calls on `self.module` inside the audio callback.
- **Risk:** If the module is cleared unexpectedly, the audio thread will panic, potentially crashing the application or the audio driver context.
- **Recommendation:** Use `if let Some` guards.
