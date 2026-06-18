# Codebase Analysis & Optimization Report

This report outlines identified bugs, performance bottlenecks, and safety concerns in the `htrk` codebase.

## ✅ Fixed Items

### Fixed: Pattern Row Overflow
- `current_row`, `pattern_break_row`, `pattern_loop_start.1`, `pattern_loop_jump_target` widened from `u8` to `u16`.
- `Effect::PatternBreak { row }` widened to `u16`.
- Format loaders updated with appropriate casts.
- Commit: `9d2b737`

### Fixed: Pattern Index Overflow
- `current_pattern` widened from `u8` to `u16`.
- `get_pattern_for_order()` return type widened to `u16`.
- Commit: `9d2b737`

### Fixed: Audio Thread Mutation Desync
- Removed unreliable `Arc::get_mut()` call from `SetSendPreFader` handler.
- Audio engine now only updates local `send_buses` state; the UI thread is responsible for persisting module data.
- Commit: `4c943f6`

### Fixed: Real-time Thread Allocations
- Pre-allocated `muted_cache`, `solo_cache`, `effective_mute_cache`, `ch_peak_cache` on `AudioEngine`.
- `process_callback` reuses these instead of allocating new `Vec<bool>` / `Vec<f32>` every callback.
- Commit: `6770e96`

### Fixed: Redundant UI Calculations
- `sample_length_bg` precomputes row-duration → shift mapping outside the cell loop into a `sample_len_cache` indexed by sample index.
- Also fixed latent bug: `bg_sample_len` color was referenced in `pattern_grid.rs` but never defined in `TrackerTheme` — added to all 8 theme presets.
- Commit: `df76e9f`

### Fixed: Timing Drift in Headless/Test Mode
- `advance()` now uses subtractive `sample_counter -= samples_per_tick` instead of `= 0.0`.
- Matches the production `process_callback` behavior.
- Commit: `df76e9f`

### Fixed: Unchecked Unwraps in Real-time Thread
- 3 bare `.unwrap()` on `self.module` in `sequencer_engine.rs` replaced with safe `match` guards.
- 12 `.unwrap()` on `engine.module` in `effects/xm.rs` and `effects/legacy.rs` replaced with safe early-return `match` patterns.
- Commit: `4c943f6`

## Remaining Opportunities

### Medium: Buffer Indirection (`Vec<Vec<f32>>`)
- `ch_mix_left`, `ch_mix_right`, `pre_ch_mix_left`, `pre_ch_mix_right` use double indirection.
- Flattening to a single `Vec<f32>` with index math would improve cache locality.
- **Scope:** Touches `engine.rs`, `mixer.rs`, and all `mix_voices_*` callers — significant refactor.

### Low: Unnecessary Arc Cloning
- `evaluate_automation` and `process_cell_unified` clone the `Module` Arc on every tick.
- The clone is necessary for borrow checker reasons (avoids holding reference across `&mut self` calls).
- Overhead is minimal (atomic increment on Arc).
