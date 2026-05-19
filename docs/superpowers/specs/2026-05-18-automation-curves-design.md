# Automation Curves Design

## Overview

Add DAW-style automation curves to htrk. Automation allows smooth, continuous control of mixer parameters (volume, panning, filter cutoff, send levels, tempo, etc.) across song positions, complementing the existing per-row effect command system.

### Design Principles

- **Song-level addressing**: Automation points are indexed by order list position + row, independent of pattern reuse
- **Hybrid entry**: Step values at any row + breakpoints with interpolation between them
- **Additive model**: Automation sets a base value, effect commands offset/scale relative to it
- **Two UI surfaces**: Per-channel overlay (inline in pattern grid) and global track editor (separate tab)

## 1. Data Model

### Types

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AutomationTarget {
    ChannelVolume,
    ChannelPanning,
    FilterCutoff,
    FilterResonance,
    SendLevel { bus: u8 },
    GlobalVolume,
    Tempo,
    Speed,
    SendReturnLevel { bus: u8 },
    SendBusParam { bus: u8, param: u8 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum InterpolationMode {
    #[default]
    Hold,
    Linear,
    Smooth,
    Exponential,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub order: u16,
    pub row: u16,
    pub value: f32,
    pub interp_to_next: InterpolationMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationTrack {
    pub id: u32,
    pub target: AutomationTarget,
    pub channel: Option<usize>,
    pub points: Vec<AutomationPoint>,
    pub default_interp: InterpolationMode,
    pub enabled: bool,
}
```

### Module Extension

```rust
// Added to Module struct
pub automation_tracks: Vec<AutomationTrack>,
pub next_automation_id: u32,
```

- All values normalized to 0.0-1.0. The sequencer maps to actual parameter ranges at evaluation time.
- Points sorted by `(order, row)`.
- `id` is auto-incrementing for stable UI references.
- `channel: None` = global track, `channel: Some(n)` = per-channel track.

## 2. Sequencer Integration

### Evaluation Pipeline

Automation evaluates first in the tick pipeline, before effect processing:

```
process_tick()
  +-- evaluate_automation()           // NEW
  +-- if tick == 0: process_tick_zero_unified()
  +-- else: process_effects_tick_unified()
  +-- advance_envelopes()
  +-- current_tick += 1
```

### Interpolation Engine

```rust
fn evaluate_track_at_position(
    points: &[AutomationPoint],
    order: u16, row: u16, tick: u8, speed: u8,
) -> f32
```

Converts position to continuous "song tick" for sub-row interpolation:
`song_tick = (order as u64 * MAX_ROWS + row as u64) * speed as u64 + tick as u64`

Where `MAX_ROWS = 1024`. Finds bracketing points and interpolates based on the left point's `interp_to_next` mode. Before first point = hold first value. After last point = hold last value. No points = return 1.0 for multipliers (identity) or 0.0 for offsets.

Interpolation formulas:
- **Hold**: return left point value
- **Linear**: `left + (right - left) * t` where `t` is normalized position between points
- **Smooth**: cosine interpolation `left + (right - left) * (1 - cos(t * PI)) / 2`
- **Exponential**: `left * (right / left).powf(t)` (with zero-clamping)

### Additive Application Model

| Parameter | Automation sets | Effect interaction |
|-----------|----------------|-------------------|
| ChannelVolume | `auto_volume_factor` (0.0-1.0) | `SetVolume(n)` -> `n * auto_volume_factor` |
| FilterCutoff | `auto_filter_cutoff` (0.0-1.0) | `FilterCutoffSlide` relative to `auto_filter_cutoff * max_cutoff` |
| ChannelPanning | `auto_pan_offset` (-1.0-1.0) | `SetPanning(n)` -> `n + auto_pan_offset * range` |
| SendLevel | `auto_send_factor` per bus (0.0-1.0) | `SetSendLevel` -> `level * auto_send_factor[bus]` |
| GlobalVolume | `auto_global_vol_factor` (0.0-1.0) | `SetGlobalVolume(n)` -> `n * auto_global_vol_factor` |
| Tempo | `auto_tempo_factor` (0.0-1.0) | `SetTempo(n)` -> `n * auto_tempo_factor` |

### ChannelState Additions

```rust
// Per-channel (default = identity)
auto_volume_factor: f32,      // 1.0
auto_pan_offset: f32,         // 0.0
auto_filter_cutoff: f32,      // 1.0
auto_filter_resonance: f32,   // 0.0
auto_send_factor: [f32; 4],   // [1.0, 1.0, 1.0, 1.0]

// Global (SequencerState)
auto_global_vol_factor: f32,  // 1.0
auto_tempo_factor: f32,       // 1.0
```

All default to identity values so no automation = no behavior change.

## 3. UI -- Per-Channel Automation Columns

### Overlay Model

Each channel header gets a dropdown to select an automation target. When selected, the channel's **effect column** transforms into an automation lane. Note, instrument, and volume columns remain unchanged.

### Visual Rendering

| Row State | Visual |
|-----------|--------|
| Breakpoint point | Bright hex value (`3F`), dot on curve |
| Interpolated row | Dim thin curve line at interpolated position, no text |
| No automation data | Dim dash at default position |

The curve is rendered as a thin line overlay in the column background using the Painter API, identical rendering approach to the envelope editor but squeezed into column width.

### Keyboard Entry

| Key | Action |
|-----|--------|
| `0-9 A-F` | Enter hex value at current row (creates point) |
| `Delete` | Remove point at current row |
| `Ctrl+1/2/3/4` | Set interp mode: Hold/Linear/Smooth/Exponential |

### Mouse Interaction

| Action | Behavior |
|--------|----------|
| Click empty row | Create point at click Y position |
| Click + drag point | Move point vertically |
| Shift + drag across rows | Freehand draw (creates points at each row) |
| Right-click point | Context menu: delete, interpolation mode |
| Scroll on point | Fine-adjust value |

## 4. UI -- Global Automation Tracks

### New Tab

`AppView::Automation` tab added to the tab bar.

### Layout

Left sidebar: track list (all automation tracks, grouped by channel/global). Right panel: curve editor for selected track(s).

- Horizontal axis = pattern rows, aligned with current order position
- Y-axis = parameter range (labeled for the target type)
- Current playback position shown as vertical playhead line
- Multiple tracks can be overlaid (shift+select in track list)
- `[+ Add Track]` button opens target picker

### Per-Channel Tracks Appear Here Too

The track list shows all automation tracks -- both global and per-channel. Edits in either the per-channel column overlay or this global view update the same `AutomationTrack` data.

## 5. Persistence

### HTK Format

- New fields on `Module`: `automation_tracks`, `next_automation_id`
- All new types derive `Serialize`/`Deserialize` -- automatic bincode support
- HTK version bumps from 4 to 5
- Loading older HTK files initializes `automation_tracks: vec![]`, `next_automation_id: 0`

### Legacy Formats

IT/XM/S3M/MOD files initialize with empty automation tracks. No import mapping.

### Order List Mutation

When the order list changes (insert/delete/reorder), automation points remap:

```rust
fn remap_automation_orders(tracks: &mut [AutomationTrack], at: u16, shift: i16)
```

Points with `order >= affected_index` get their `order` shifted. Simple linear scan.

## Implementation Order

### Phase 1: Data Model & Interpolation
1. Add `AutomationTarget`, `InterpolationMode`, `AutomationPoint`, `AutomationTrack` types to `src/sequencer/automation.rs`
2. Add `automation_tracks` and `next_automation_id` to `Module`
3. Implement `evaluate_track_at_position` interpolation engine
4. Implement `remap_automation_orders` for order list mutations
5. Unit tests for interpolation (all 4 modes, edge cases, empty tracks)

### Phase 2: Sequencer Integration
1. Add automation factor fields to `ChannelState` and `SequencerState`
2. Implement `evaluate_automation()` on `SequencerEngine`
3. Implement `apply_automation_to_channel()` and `apply_automation_global()`
4. Integrate into `process_tick()` pipeline
5. Modify existing effect application to use automation factors
6. Unit tests: automation + effects coexistence, no automation = no behavior change

### Phase 3: Per-Channel UI
1. Add channel header automation target dropdown
2. Implement automation column rendering (curve overlay + point values)
3. Implement mouse interaction (click, drag, freehand draw)
4. Implement keyboard entry in automation column
5. Implement point context menu (delete, interpolation mode)
6. Add automation column to `SubColumn` navigation

### Phase 4: Global Automation View
1. Add `AppView::Automation` variant
2. Implement track list sidebar
3. Implement lane curve editor (reuse envelope editor patterns)
4. Implement add/remove track with target picker
5. Implement multi-track overlay

### Phase 5: Persistence
1. Add serde derives to all new types
2. Bump HTK version to 5
3. Add backward-compatible loading (version < 5 = empty automation)
4. Hook `remap_automation_orders` into order list mutation paths
5. Integration test: HTK round-trip with automation data
