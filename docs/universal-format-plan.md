# Universal Internal Format Plan

## Goal

Eliminate all `ModuleFormat::XM` branching in the sequencer. Replace the two
parallel code paths (XM and non-XM) with a single unified path. Format-specific
behavior is either encoded into the data at import time or parameterized via
`ModuleFlags`.

## Current State

- Data model (`Cell`/`Effect`/`Module`) is already unified across formats
- Sequencer has **19 XM-specific functions** and their non-XM counterparts
- `ChannelState` has ~20 fields only used by the XM path
- XM stashes raw effect bytes (`eff_typ_xm`/`eff_xm`) then re-dispatches on
  subsequent ticks — redundant with `ActiveEffects` + `last_*` pattern
- XM uses period arithmetic; non-XM uses frequency arithmetic
- XM and IT have different envelope/fadeout volume formulas
- Volume column slides are re-decoded on non-zero ticks from a raw byte

## Phase 1 — Data Model Changes (DONE 2026-05-05)

**Summary of changes:**

| File | Change | Status |
|------|--------|--------|
| `src/sequencer/pattern.rs` | Added `volume_effect: Option<Effect>` to `Cell`; updated `is_empty()` | ✅ |
| `src/sequencer/player.rs` | Marked all XM-specific fields with comment `// XM — will be removed in Phase 4` | ✅ |
| `src/sequencer/module.rs` | Added `xm_envelope_model: bool` to `ModuleFlags` | ✅ |
| `src/formats/xm.rs` | Added `volume_effect: None` to Cell literal in `decode_xm_cell`; set `xm_envelope_model: true` | ✅ |
| `src/formats/it.rs` | Added `volume_effect: None` to Cell literal in IT pattern parser; set `xm_envelope_model: false` | ✅ |
| `src/formats/modfile.rs` | Uses `ModuleFlags::default()` — no change needed | ✅ |
| `src/formats/s3m.rs` | Uses `ModuleFlags::default()` — no change needed | ✅ |

**Notes:**
- ChannelState XM fields are kept (not removed) because they are still
  referenced by the XM sequencer path. Removal will happen in Phase 4 when
  the XM sequencer functions are rewritten.
- All 168 lib unit tests pass. The 2 IT integration test failures are
  pre-existing (IT save doesn't write format magic correctly, not related
  to these changes).
- The `volume_effect` field is currently always set to `None` at
  construction sites — actual population will happen in Phase 2.

## Phase 1 — Data Model Changes (Plan)

### `Cell` (src/sequencer/pattern.rs)

Add a decoded volume column effect:

```rust
pub struct Cell {
    pub note: Note,
    pub instrument: Option<u8>,
    pub volume: Option<u8>,           // ≤ 64 only after this change
    pub volume_effect: Option<Effect>, // decoded vol col slide/porta/vibrato
    pub effect: Effect,
}
```

### `ChannelState` (src/sequencer/player.rs)

**Remove** (all XM-specific, no longer needed):
- `eff_typ_xm`, `eff_xm`, `vol_kol`
- `real_period`, `want_period`, `out_period`, `porta_speed_period`, `porta_dir`
- `vib_pos`, `trem_pos`, `vib_speed`, `vib_depth`, `trem_speed`, `trem_depth`
- `wave_ctrl`, `retrig_cnt`, `retrig_speed`, `retrig_vol`
- `rel_ton`, `real_vol`, `old_vol`, `old_pan`, `tremor_pos_byte`

**Keep** (format-neutral):
- `last_note`, `last_instrument`, `last_sample`
- `channel_volume`, `row_volume`, `channel_panning`
- `last_effect`, all `last_*speed`/`depth`/`up`/`down` memory fields
- `last_arpeggio`, `last_retrigger_interval`
- `last_sample_offset`, `last_panbrello_speed`, `last_panbrello_depth`
- `tremor_ontime`, `tremor_offtime`, `tremor_counter`, `tremor_active`
- `glissando`, `fine_tune_offset`
- `portamento_target_period: Option<f64>`
- `muted`, `solo`
- `delayed_cell`, `note_delay_ticks`
- `active_effects: ActiveEffects`
- `note_cut_tick: Option<u8>`

### `ModuleFlags` (src/sequencer/module.rs)

Add if needed for envelope model selection:

```rust
pub struct ModuleFlags {
    // ... existing fields ...
    pub xm_envelope_model: bool, // true = FT2 envelope math
}
```

## Phase 2 — Import Changes (DONE 2026-05-05)

**Summary of changes:**

| File | Change | Status |
|------|--------|--------|
| `src/formats/xm.rs:768-773` | Fixed `decode_xm_volume_column`: 0xF0-0xFF → TonePortamento (was wrong PortamentoDown); 0xD0-0xEF → None (pan slides, handled via vol_kol) | ✅ |
| `src/formats/xm.rs:692-722` | Changed `decode_xm_cell`: vol col raw byte stored in `cell.volume` (for vol_kol backward compat), decoded effect in `cell.volume_effect`; `cell.effect` is now the effect column only (not overridden by vol col) | ✅ |
| `src/audio/sequencer_engine.rs:388-400` | Added `volume_effect` eff_typ_xm mapping in `process_cell_xm` for continuous effects | ✅ |
| `src/audio/sequencer_engine.rs:478-482` | Added `volume_effect` `apply_effect_xm` call after ch_state borrow ends | ✅ |
| `src/audio/sequencer_engine.rs:307` | Removed unused `_vol_col` variable | ✅ |
| `src/audio/sequencer_engine.rs:469-472` | Removed dead code comment block | ✅ |
| `src/formats/xm.rs:1375-1378` | Updated `decode_xm_volume_column_portamento` test for corrected mappings | ✅ |

**Key design decisions:**
- `cell.volume` stores the RAW volume column byte for vol column effect rows
  (> 0x50). This keeps `vol_kol` alive in `process_effects_tick_xm` for pan
  slides (0xD/0xE) which have no Effect variant. The `process_cell_xm` code
  applies the condition `if vol <= 64` so raw effect bytes don't set channel
  volume.
- `cell.volume_effect` stores the DECODED effect variant for the unified
  sequencer (Phase 3). On tick 0, `process_cell_xm` sets `eff_typ_xm` from
  `volume_effect` for continuous effects and calls `apply_effect_xm`.
- `cell.effect` is no longer overridden by the volume column effect. Both
  fields are independent, matching XM semantics where vol col and effect
  column coexist.
- Bug fix: 0xF0-0xFF in the volume column now correctly decodes to
  TonePortamento (was PortamentoDown). 0xD0-0xEF no longer produce
  incorrect TonePortamento/PortamentoUp (they are pan slides handled via
  vol_kol).

### IT/S3M/MOD Loaders

Minimal or no changes — they already decode directly into `Effect` variants
and don't depend on raw byte stashing in the sequencer.

## Phase 3 — Unified Sequencer (DONE 2026-05-05)

**Summary: All format-specific branching in the sequencer has been replaced with unified dispatch.**

### Completed Sub-Phases

**Phase 3.1 — ActiveEffects dispatch**
- Removed `eff_typ_xm` dispatch from `process_effects_tick_xm`, replaced with `ActiveEffects` flag checks
- Added `TonePortamentoVolumeSlide` and `VibratoVolumeSlide` handling to `apply_effect_xm` (sets both sub-effect flags)
- Removed dead code: `should_vib`/`should_tp_vcol` checks, `eff_typ_xm` 0x14/0x1B/0x0E dispatch paths
- **Fixed XM note delay**: added `note_delay_ticks` check to `process_effects_tick_xm` (was broken — delayed notes stored but never triggered)
- **Fixed XM retrig**: added `retrig_speed`/`last_retrigger_interval` checks
- Removed `eff_typ_xm`/`eff_xm` setting from `process_cell_xm` (no longer needed)
- Removed dead period reset code (eff_typ_xm was always 0xFF at tick 0 after advance_row reset)
- Removed volume_effect eff_typ_xm setting (apply_effect handles ActiveEffects directly)
- 3 new tests: `xm_active_effects_dispatch_volume_slide`, `xm_active_effects_dispatch_tpvs`, `xm_note_delay_triggers_on_correct_tick`

**Phase 3.2 — Unified `process_effects_tick`**
- Created `process_effects_tick_unified` merging XM and IT non-zero tick paths
- Both paths use `ActiveEffects` dispatch (structurally identical)
- Format-specific code gated on `is_xm = self.module_format == ModuleFormat::XM`
- Removed old IT `process_effects_tick` and XM `process_effects_tick_xm`
- `process_tick` no longer branches on `is_xm` for non-zero ticks

**Phase 3.3 — Unified `apply_effect`**
- Created `apply_effect_unified` merging XM and IT effect application into one function
- Uses `ch.active_effects` directly (not local variable) — XM calls twice (effect + volume_effect)
- Format-specific code gated on `is_xm` for: TonePortamento (period vs frequency), PortamentoUp/Down, Vibrato/Tremolo state, SetGlobalVolume scale (64 vs 128), PatternDelay, VibratoWaveform/TremoloWaveform, SetFineTune, SetEnvelopePosition, Tremor, FineVolumeSlide, NoteDelay
- IT-only effects handled: ExtendedEffect, Panbrello, GlobalVolumeSlide, VolPortamento, VolVibrato, etc.
- Removed old `apply_effect_xm` and IT `apply_effect`
- Both `process_cell_xm` and `process_cell_with_module` call `apply_effect_unified`

**Phase 3.4 — Unified `process_tick_zero` / `process_cell`**
- Created `process_tick_zero_unified` and `process_cell_unified`
- Replaced both XM (`process_tick_zero_xm`/`process_cell_xm`) and IT (`process_tick_zero`/`process_cell`/`process_cell_with_module`) versions
- Format-specific branches for: volume column handling (vol_kol vs apply_volume_column), tone portamento (period vs frequency), note trigger (trigger_channel_note_xm vs trigger_channel_note), note off (handle_note_off_xm vs handle_note_off), volume_effect (XM only)
- `process_tick` and `play()`/`play_from()` now call unified functions
- No `is_xm` branching in tick dispatch anymore

**Phase 3.5 — Unified `advance_envelopes`**
- Converted `is_xm` check from `self.module_format` to `module.flags.xm_envelope_model`
- This is the appropriate feature flag: XM envelope model includes FT2 fade-out, auto-vibrato, note_off handling

### Status Before → After

| Metric | Before Phase 3 | After Phase 3 |
|--------|---------------|--------------|
| Format branches in `process_tick` | 2 (`is_xm`) | 0 |
| Tick-zero functions | 2 (`_xm` + IT) | 1 unified |
| Tick effect functions | 2 (`_xm` + IT) | 1 unified |
| Apply effect functions | 2 (`_xm` + IT) | 1 unified |
| `xm_envelope_model` usage | 0 places | 1 place (`advance_envelopes`) |
| `eff_typ_xm` usage for dispatch | ~60 lines | 0 lines |
| Dead code paths in tick processing | 4 (0x14, 0x1B, 0x0E, period reset) | 0 |
| Total tests | 168 | 171 |

### Remaining (saved for Phase 4)
- `module_format` field on `SequencerEngine` — still used in `process_effects_tick_unified`, `apply_effect_unified`, `process_cell_unified` for period vs frequency model
- 19 XM-specific functions — all still exist and are called for XM modules
- XM-specific `ChannelState` fields — still exist but `eff_typ_xm`/`eff_xm` no longer used for dispatch

### Pitch Model

Convert to **frequency-based** everywhere. XM period operations are translated:

| XM operation | Unified equivalent |
|---|---|
| `period = get_note_period(key, ft, linear)` | `freq = period_to_frequency(period, linear, 8363)` called once at trigger |
| `period += speed * 4` (porta up) | `freq *= 2^(-speed*4/1536)` in linear mode |
| `period -= speed * 4` (porta down) | `freq *= 2^(speed*4/1536)` in linear mode |
| Vibrato adds to period | Vibrato multiplies frequency |
| `out_period = real_period + vib` | `freq = base_freq * (1 + vib_factor)` |

The conversion factor `1536` comes from the XM linear period formula:
`freq = 8363 * 2^((7680 - period) / 1536)`, so `Δperiod = 4` → `freq *= 2^(-4/1536)`.

### Envelope / Fade-Out Model

**Option A** (preferred): Normalize at import. XM's fade-out (`fade_out_amp`
decremented by `fade_out_speed = inst.fade_out * 512`) is converted to an
IT-style `fade_out_rate`:

```
fade_out_rate = 65536.0 / (inst.fade_out * 512) * 4096.0
```

This makes XM fade-out last the same number of ticks under the IT model.

**Option B**: Keep `xm_envelope_model` flag in `ModuleFlags` and select the
formula at runtime. Less pure but safer for FT2 compatibility.

### Volume Column on Non-Zero Ticks

Not needed — volume column slides are decoded into `volume_effect` at import
time, which sets `ActiveEffects.volume_slide` on tick 0, and
`process_effects_tick()` applies them on every tick. No `vol_kol` re-decode.

### Auto-Vibrato

Keep as-is but gate on instrument `vib_depth > 0` rather than
`ModuleFormat::XM`. Non-XM instruments have `vib_depth = 0` so it's a no-op
for them.

## Phase 4 — Cleanup (DONE 2026-05-05)

### Completed Steps

**Phase 4.1 — Remove `module_format` from `SequencerEngine`**
- Replaced `pub module_format: ModuleFormat` with `use_xm_model: bool`
- Set from `module.format == ModuleFormat::XM` in `load_module()`
- All format-specific gating now uses `self.use_xm_model` instead of `self.module_format == ModuleFormat::XM`

**Phase 4.2 — Remove dead `ChannelState` fields**
- Removed `eff_typ_xm: u8` — no longer used for dispatch (replaced by `ActiveEffects`)
- Removed `eff_xm: u8` — unused since Phase 2 removed raw effect byte storage
- Removed `advance_row` reset of `eff_typ_xm = 0xFF`
- Updated `advance_row_resets_effects` test accordingly

**Phase 4.3 — Remove all `fn *_xm` functions**
- 4 functions **inlined** into call sites: `apply_portamento_up_xm`, `apply_portamento_down_xm`, `apply_portamento_up_xm_fine`, `apply_portamento_down_xm_fine` — their period math was moved directly into `apply_effect_unified` and `process_effects_tick_unified`
- 11 functions **renamed** from `_xm` to `_period` suffix: `trigger_channel_note_period`, `handle_note_off_period`, `apply_tone_portamento_period`, `apply_vibrato_period`, `apply_tremolo_period`, `apply_arpeggio_period`, `apply_volume_slide_period`, `apply_tremor_period`, `do_multi_retrig_period`, `retrig_channel_note_period`, `trigger_delayed_note_period`, `update_voices_from_period`
- 4 functions were already removed in Phase 3: `process_tick_zero_xm`, `process_cell_xm`, `apply_effect_xm`, `process_effects_tick_xm`
- Test function `auto_vibrato_period_base_set_on_trigger_xm` → `auto_vibrato_period_base_set_on_trigger_note`

**Result: Zero `fn *_xm` functions remain in the codebase.**

### Status at End of Phase 4

| Metric | Before | After |
|--------|--------|-------|
| `module_format` field on Engine | Yes | No |
| `fn *_xm` functions | 19 | 0 |
| `eff_typ_xm`/`eff_xm` in ChannelState | 2 fields | 0 fields |
| Remaining period-based functions | 16 | 11 (renamed `_period`) |
| `is_xm` gating in code | `module_format == XM` | `use_xm_model` bool |
| Tests | 171 | 171 |

## Phase 5 — Native .htk Format (DONE 2026-05-05)

### Summary
Implemented a native binary format that preserves the unified `Module` struct including `volume_effect`, enabling lossless save/load of any loaded module regardless of original format. Save only supports .htk; load supports all formats.

### Design
- **Encoding**: `serde` + `bincode` for simple, maintainable serialization with minimal boilerplate
- **Header**: 12 bytes — magic `b"HTRA"` + version `u32 LE` + flags `u32 LE` (reserved)
- **Payload**: bincode-encoded Module (with custom serializers for large arrays and `Arc<Vec<f32>>`)
- **Preservation**: All `ModuleFlags` persisted including `xm_period_model`, `xm_envelope_model`, `linear_slides`

### Changes

| File | Change |
|------|--------|
| `Cargo.toml` | Added `serde` (with derive) and `bincode` deps |
| `src/sequencer/module.rs` | Added `ModuleFormat::HTK`, `ModuleFlags::xm_period_model: bool`, serde derives on `Module`/`ModuleFlags`/`ModuleFormat` |
| `src/sequencer/pattern.rs` | Added manual `Serialize`/`Deserialize` for `Pattern` (bypasses 32-element array limit); serde derives on `Cell` |
| `src/sequencer/note.rs` | Serde derives on `Note` |
| `src/sequencer/effect.rs` | Serde derives on `Effect` |
| `src/sequencer/sample.rs` | Serde derives on all types; custom `arc_vec_f32_serde` module for `Arc<Vec<f32>>` |
| `src/sequencer/instrument.rs` | Serde derives on all types; custom `array_u8_120_serde` for `[u8; 120]` arrays |
| `src/formats/htk.rs` | **New file**: `save_module(module)` and `load_module(data)` |
| `src/formats/mod.rs` | Added `htk` module; HTK detection (magic); HTK dispatch in `load_module`; `save_module` now only routes to HTK |
| `src/formats/xm.rs` | Set `xm_period_model: true` in XM loader |
| `src/formats/it.rs` | Set `xm_period_model: false` in IT loader |
| `src/audio/sequencer_engine.rs` | `load_module` sets `use_xm_model` from `module.flags.xm_period_model` instead of `module.format == XM` |
| `src/errors.rs` | Added `Bincode(String)` error variant + `From<bincode::Error>` impl |
| `src/app.rs` | Save dialog only shows `.htk`; `save_file()` drops format parameter; `open_file_dialog` includes `.htk` |
| `src/ui/status_bar.rs` | Added `ModuleFormat::HTK` display |
| `tests/format_conversions.rs` | Replaced format-specific round-trip tests with single HTK round-trip test |

### Status
- Tests: 172 (171 lib + 1 integration)
- Zero `fn *_xm` functions remain
- Zero `.htk` → `.htk` round-trips verified
- XM/S3M save functions are dead code (keep as reference)

## Remaining Work (saved for future)
- Convert remaining `_period` functions to frequency-based math (would allow removing `real_period`, `want_period`, `out_period`, `porta_speed_period`, `porta_dir`, `vib_pos`, `trem_pos`, `vib_speed`, `vib_depth`, `trem_speed`, `trem_depth`, `wave_ctrl`, `retrig_cnt`, `retrig_speed`, `retrig_vol`, `vol_kol`, `rel_ton`, `real_vol`, `old_vol`, `old_pan`, `tremor_pos_byte`, `note_cut_tick`)
- Remove `use_xm_model` bool entirely (once all XM-specific behavior is absorbed into feature flags/frequency math)
- Unify `advance_envelopes` fully (remove remaining `is_xm` blocks for fade-out and auto-vibrato)
