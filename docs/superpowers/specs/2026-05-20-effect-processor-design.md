# Phase 2: Enum-Dispatched EffectProcessor — Design Spec

**Date:** 2026-05-20
**Status:** Approved

## Goal

Break the 5,209-line `sequencer_engine.rs` into format-specific effect handlers using an enum-dispatched `EffectProcessor`, eliminating the pervasive `is_xm` / `use_xm_model` branching from the engine's core tick loop.

## Milestone

Playback works identically for all formats (XM, MOD, S3M, IT), but with effect logic cleanly separated into three files under `src/audio/effects/`. All 38 existing tests pass unchanged.

---

## Architecture

### File Layout

```
src/audio/
  effects/
    mod.rs              — EffectProcessor enum, EffectContext struct, shared helpers
    xm.rs               — XmProcessor: period-domain XM/FT2 effect handling
    legacy.rs           — LegacyProcessor: frequency-domain MOD/S3M/IT handling
  sequencer_engine.rs   — Slimmed to ~2500 lines (transport, tick loop, row advance,
                           voices, envelopes, tests)
```

### EffectProcessor Enum

```rust
// src/audio/effects/mod.rs
pub enum EffectProcessor {
    Xm(XmProcessor),
    Legacy(LegacyProcessor),
}
```

Selected once in `SequencerEngine::load_module()` based on `module.flags.xm_period_model`. Replaces the `use_xm_model: bool` field on `SequencerEngine`.

### EffectContext

A borrow bundle passed to every processor method. Avoids giving processors `&mut SequencerEngine`:

```rust
pub struct EffectContext<'a> {
    pub channels: &'a mut [ChannelState],
    pub voices: &'a mut [Voice],
    pub module: &'a Module,
    pub sample_rate: f64,
    pub global_volume: &'a mut f32,
}
```

Constructed per-tick from `SequencerEngine` fields:

```rust
let module = self.module.as_ref().unwrap().clone(); // Arc clone — cheap
let mut ctx = EffectContext {
    channels: &mut self.state.channels,
    voices: &mut self.voices,
    module: &module,
    sample_rate: self.output_sample_rate,
    global_volume: &mut self.global_volume,
};
self.processor.process_tick(&mut ctx, tick);
```

### Dispatched Methods

Five methods on `EffectProcessor`, each delegating to the active variant:

| Method | Purpose | Source Location (Current) |
|--------|---------|---------------------------|
| `apply_effect(ctx, ch, effect, is_row_start)` | Row-start effect setup | `apply_effect_unified` (L823–1579, 756 lines) |
| `process_tick(ctx, tick)` | Per-tick continuous effects | `process_effects_tick_unified` (L1581–1862, 282 lines) |
| `trigger_note(ctx, ch, key, remapped, sample, sample_idx, cell, module, inst_idx)` | Note-on with instrument/sample | `trigger_channel_note_period` (XM, L546–729) / `trigger_channel_note` (legacy, L2312–2525) |
| `trigger_delayed_note(ctx, ch, linear)` | Delayed note on target tick | `trigger_delayed_note_period` (XM, L2156–2288) / `trigger_delayed_note` (legacy, L3055–3111) |
| `process_volume_column(ctx, ch, vol)` | Volume column decoding | inline XM `vol_kol` (L1588–1619) / `apply_volume_column` (legacy, L2648–2698) |

### Handler-Owned Helper Methods

Each processor variant owns the format-specific helper methods currently branched inside `SequencerEngine`:

**XmProcessor** (from period-domain methods, L1864–2288):
- `apply_tone_portamento_period`
- `apply_vibrato_period`
- `apply_tremolo_period`
- `apply_arpeggio_period`
- `apply_volume_slide_period`
- `apply_tremor_period`
- `do_multi_retrig_period`
- `retrig_channel_note_period`
- `handle_note_off_period`
- `update_voices_from_period` (shared, stays in mod.rs)

**LegacyProcessor** (from frequency-domain methods, L2700–3111):
- `apply_portamento_up`
- `apply_portamento_down`
- `apply_tone_portamento`
- `apply_volume_slide`
- `apply_panning_slide`
- `apply_vibrato`
- `apply_tremolo`
- `apply_arpeggio`
- `apply_panbrello`
- `apply_tremor`
- `retrigger_channel_note`
- `handle_note_off`

### What Stays in SequencerEngine

| Section | Lines | Notes |
|---------|-------|-------|
| Struct definition + fields | 43–53 | Remove `use_xm_model`, add `processor: EffectProcessor` |
| Lifecycle/transport (play, stop, pause, load_module) | 70–209 | `load_module` sets processor variant |
| `advance` + `process_tick` (tick loop) | 210–261 | Calls processor methods instead of inline branches |
| Automation | 263–330 | Unchanged |
| `process_tick_zero_unified` | 334–369 | Unchanged (collects cells) |
| `process_cell_unified` | 371–544 | Replaces `is_xm` branches with processor calls |
| Voice allocation + NNA | 2527–2665 | Unchanged |
| `advance_row` | 3334–3450 | Unchanged |
| `advance_envelopes` | 3113–3332 | Still has XM/non-XM branches internally (future Phase 2b) |
| Utility (voice alloc, compute_volume, etc.) | 3452–3599 | Unchanged |
| Free helper functions | 3601–3719 | Shared — move to `effects/mod.rs` |
| Tests | 3721–5209 | Relocate format-specific tests to handler files |

---

## Branch Resolution Strategy

When splitting a method with `is_xm` branches into two handler methods:

1. **XM-only code** → `XmProcessor` method. Remove all `if is_xm` guards; the code is the body.
2. **Non-XM code** → `LegacyProcessor` method. Remove all `if !is_xm` / `else` guards; the code is the body.
3. **Shared code** (runs for both formats) → stays in `EffectContext` as a helper, or inlined in both.
4. **Format-conditional constants** (e.g., S3M speed ×4) → handled inside `LegacyProcessor` using `module.format`.

Example — `apply_effect_unified` TonePortamento branch (L924–951):

```rust
// Before (in SequencerEngine):
Effect::TonePortamento { speed } => {
    if is_xm {
        // XM period-domain setup (~20 lines)
    } else {
        // legacy speed memory (~3 lines)
    }
    ch.active_effects.tone_portamento = true;
}

// After:
// In XmProcessor::apply_effect:
Effect::TonePortamento { speed } => {
    // XM period-domain setup (same 20 lines, no if guard)
    ch.active_effects.tone_portamento = true;
}

// In LegacyProcessor::apply_effect:
Effect::TonePortamento { speed } => {
    // legacy speed memory (same 3 lines, no if guard)
    ch.active_effects.tone_portamento = true;
}
```

---

## Migration Strategy

Every step compiles and passes all tests. Commit after each step.

### Step 1: Create effects module with stubs
- Create `src/audio/effects/mod.rs` with `EffectProcessor` enum, `EffectContext` struct, `XmProcessor` and `LegacyProcessor` stubs.
- Create empty `src/audio/effects/xm.rs` and `src/audio/effects/legacy.rs`.
- Add `processor: EffectProcessor` field to `SequencerEngine`, set in `load_module()`.
- Keep `use_xm_model` field temporarily during migration (removed in Step 7).
- **Verification:** `cargo check` + `cargo test`.

### Step 2: Move shared free functions to effects/mod.rs
- Move `compute_samples_per_tick`, `get_vibrato_value`, `advance_single_envelope`, `evaluate_envelope`, `compute_playback_frequency`, `quantize_to_semitone`, `fastrand`, `VIBRATO_SINE_TABLE`, `VIBRATO_RAMP_TABLE`, `FUNK_TRACK` from `sequencer_engine.rs` to `effects/mod.rs` (pub(crate)).
- Update imports.
- **Verification:** `cargo test`.

### Step 3: Move helper methods to handlers
- XM period-domain helpers (L1864–2310: `apply_tone_portamento_period`, `apply_vibrato_period`, `apply_tremolo_period`, `apply_arpeggio_period`, `apply_volume_slide_period`, `apply_tremor_period`, `do_multi_retrig_period`, `retrig_channel_note_period`, `update_voices_from_period`) → `XmProcessor`.
- Legacy frequency-domain helpers (L2700–3111: `apply_portamento_up`, `apply_portamento_down`, `apply_tone_portamento`, `apply_volume_slide`, `apply_panning_slide`, `apply_vibrato`, `apply_tremolo`, `apply_arpeggio`, `apply_panbrello`, `apply_tremor`, `retrigger_channel_note`) → `LegacyProcessor`.
- `apply_volume_column` (L2648–2698) → `LegacyProcessor`.
- `handle_note_off_period` (L760–819) → `XmProcessor`.
- `handle_note_off` (L2609–2627) → `LegacyProcessor`.
- `calculate_sample_offset` (L731–758) → `effects/mod.rs` (shared).
- Temporarily: `SequencerEngine` delegates to `self.processor.helper_name(ctx, ...)` for each call. Where the call sites in `apply_effect_unified` or `process_effects_tick_unified` call these helpers, they go through the processor.
- **Note:** Some helpers are called from both `apply_effect_unified` and `process_effects_tick_unified`. Moving them first means the big dispatch methods can call them through the processor.
- **Verification:** `cargo test`.

### Step 4: Move `apply_effect_unified` into handlers
- Copy `apply_effect_unified` into `XmProcessor::apply_effect` (keep only `is_xm` branches) and `LegacyProcessor::apply_effect` (keep only else branches). Shared branches (both formats do the same thing) go into both.
- `SequencerEngine.apply_effect_unified` becomes `self.processor.apply_effect(ctx, ch, effect, is_row_start)`.
- Remove old method.
- **Verification:** `cargo test` — all 38 tests pass.

### Step 5: Move `process_effects_tick_unified` into handlers
- Same split: XM version into `XmProcessor::process_tick`, legacy into `LegacyProcessor::process_tick`.
- `SequencerEngine` calls `self.processor.process_tick(ctx, tick)`.
- **Verification:** `cargo test`.

### Step 6: Move note trigger methods
- `trigger_channel_note_period` (L546–729) → `XmProcessor::trigger_note`
- `trigger_channel_note` (L2312–2525) → `LegacyProcessor::trigger_note`
- `trigger_delayed_note_period` (L2156–2288) → `XmProcessor::trigger_delayed_note`
- `trigger_delayed_note` (L3055–3111) → `LegacyProcessor::trigger_delayed_note`
- `process_cell_unified` updated to call `self.processor.trigger_note(...)` and `self.processor.trigger_delayed_note(...)`.
- **Verification:** `cargo test`.

### Step 7: Remove `use_xm_model` and temporary `is_xm()` helper
- All branching is now inside the processor. Remove the `use_xm_model` field from `SequencerEngine`.
- `process_cell_unified` calls processor for note trigger, volume column, and effect application — no format checks remain in `SequencerEngine`.
- **Verification:** `cargo test`.

### Step 8: Relocate tests
- Move XM-specific tests (xm_effects_dispatch, xm_portamento, xm_vibrato, xm_tremolo, xm_volume_slide, xm_arpeggio, xm_tremor, xm_retrigger, xm_note_delay, xm_auto_vibrato, xm_envelope, xm_key_off) to `xm.rs` test module.
- Move legacy-specific tests (mod_playback, s3m_portamento, it_tone_portamento, note_delay, pattern_loop, global_volume_slide, extra_fine_portamento, funk_it, karplus_strong, memory_preservation) to `legacy.rs` test module.
- Keep shared transport/voice tests in `sequencer_engine.rs` (samples_per_tick, envelope_advance, vibrato_tables, playback_frequency, voice_allocation, row_reset).
- **Verification:** `cargo test`.

---

## Expected File Sizes After Phase 2

| File | Before | After |
|------|--------|-------|
| `sequencer_engine.rs` | 5209 | ~2500 (transport, tick loop, row advance, envelopes, shared state) |
| `effects/mod.rs` | — | ~300 (enum dispatch, context, shared helpers) |
| `effects/xm.rs` | — | ~1200 (XM effects, period-domain note trigger, helpers, tests) |
| `effects/legacy.rs` | — | ~1000 (legacy effects, frequency-domain note trigger, helpers, tests) |

---

## Risk Mitigation

- **Every step compiles and passes tests** — No big-bang rewrite. Each method is moved one at a time.
- **Zero behavioral change** — Pure code movement. The split is mechanical: each `if is_xm` branch goes to its handler.
- **Enum dispatch is zero-cost** — No vtable, same as current `if is_xm` branch (compiler sees exhaustive match).
- **Shared state via EffectContext** — Borrow bundle avoids `&mut self` on `SequencerEngine`, keeping the borrow checker happy.
- **`advance_envelopes` deferred** — The envelope code has its own XM/non-XM split (~200 lines). It's less hot-path critical and can be a follow-up (Phase 2b) to keep this phase focused.
- **S3M-specific quirks** (speed ×4, raw effects) stay in `LegacyProcessor` where `module.format == ModuleFormat::S3M` is checked.

## AGENTS.md Compliance

- **Row State Isolation** (Rule 1): `advance_row` stays in `SequencerEngine` — no change.
- **Note Delay** (Rule 2): Delayed note handling moves to processors but behavior is identical.
- **Volume Resets** (Rule 3): Volume reset logic moves with the note trigger — no change.
- **Testing Requirements** (Rule 4): All 38 existing tests must pass at every step. New handler-specific tests added in Step 8.
- **Format-Conditional Processing** (Rule 6): XM and legacy paths are now physically separated into different files — impossible for one to affect the other.
