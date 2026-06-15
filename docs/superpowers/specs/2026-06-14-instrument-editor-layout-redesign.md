# Instrument Editor Layout Redesign

## Goal

Reorganize the instrument editor's right panel (after the instrument list splitter) to match the OpenMPT visual grouping and arrangement described in `instrument_editor.md`. No new data fields — layout and control style only.

## Layout Overview

```
┌──────────────────────────────────────────────────┐
│ Left splitter → │ Instrument 02: [name....] S L │ Header
├──────────────────────────────────────────────────┤
│ ┌──────────┐ ┌──────────┐ ┌──────────┐         │
│ │ General  │ │ Pitch-Pan│ │ Filter   │  Left    │
│ │ Vol=128  │ │ Sep=0    │ │ Cut=65535│  column  │
│ │ Fade=0   │ │ Ctr=60   │ │ Res=0    │          │
│ │          │ │          │ │ LP/HP/BP │          │
│ └──────────┘ └──────────┘ └──────────┘         │
│ ┌──────────┐ ┌──────────┐ ┌──────────┐         │
│ │ NNA      │ │ Random   │ │ Vibrato  │  Right   │
│ │ Cut/Cont │ │ Vol=0    │ │ Type=Sine│  column  │
│ │ DCT=Off  │ │ Pan=0    │ │ Swp/Dp/Rt│          │
│ │ DNA=Cut  │ │ Filt=0   │ │          │          │
│ └──────────┘ └──────────┘ └──────────┘         │
├──────────────────────────────────────────────────┤ ← divider
│ Paint:[Br] [palette] [keyboard] │ [note map]   │ Maps
├──────────────────────────────────────────────────┤ ← divider
│ ● Vol 3  ○ Pan 2  ○ Pitch  ○ Flt               │ Env tabs
├──────────────────────────────────────────────────┤
│              Envelope graph (full width)         │ Graph
│                                                  │
├──────────────────────────────────────────────────┤
│ [Enab][Sust][Carry] [Set][Clr] [Loop][LSt][LEn] │ Toolbar
│ [+Point] [Generate]                              │
└──────────────────────────────────────────────────┘
```

## Detailed Section Layout

### 1. Header Row (fixed height)
- `egui::RichText` heading: `"Instrument 02:"` using `*selected_instrument`
- `TextEdit::singleline` for name (unchanged)
- `[Save...]` `[Load...]` buttons (unchanged)

### 2. Settings Grid (2 columns, scrollable)
Each group is an `egui::Frame::group()` with a tiny heading.

**Left column — General, Pitch-Pan, Filter**

| Group | Controls | Widget | Range |
|---|---|---|---|
| General | Global Volume | `Slider` | 0..=128 |
| | Fadeout | `DragValue` | 0..=4095 |
| Pitch-Pan | Separation | `Slider` | -32..=32 |
| | Center | `DragValue` | 0..=119 |
| Filter | Cutoff | `DragValue` | 0..=0xFFFF |
| | Resonance | `Slider` | 0..=255 |
| | Type | selectable labels | LP/HP/BP/Notch |

**Right column — NNA, Random, Vibrato**

| Group | Controls | Widget | Range |
|---|---|---|---|
| NNA | NNA | selectable labels | Cut/Cont/Off/Fade |
| | DCT | selectable labels | Off/Note/Samp/Inst |
| | DNA | selectable labels | Cut/Off/Fade |
| Random | Volume | `Slider` | 0..=100 |
| | Panning | `Slider` | 0..=100 |
| | Filter Cut | `Slider` | 0..=255 |
| Vibrato | Type | selectable labels | Sine/Ramp/Sq/Rand |
| | Sweep | `DragValue` | 0..=255 |
| | Depth | `DragValue` | 0..=255 |
| | Rate | `DragValue` | 0..=255 |

### 3. Maps Row (horizontal, side-by-side)
- **Sample Map**: paint sample combo + palette + keyboard grid (unchanged logic, more compact layout)
- **Note Map**: transpose grid (unchanged logic)

### 4. Envelope Editor (full width, bottom section)
- **Tabs**: selectable labels with ●/○ and point count (unchanged)
- **Graph**: full available width, taller than current mid-height group
- **Toolbar**: single row of compact framed groups:
  - Flags: Enabled, Sustain, Carry (checkboxes)
  - Sus: Set/Clr buttons
  - Loop: On/Off toggle + LSt/LEn buttons
  - Add: +Point, Generate buttons

## Widget Style
- Labels use `ui.add_sized([label_w, 0.0], ...)` for alignment (keep existing pattern)
- Enum selectors use `selectable_label` in a `ui.horizontal` row
- Framed groups use `egui::Frame::group(ui.style()).inner_margin(Margin::symmetric(4, 2))`
- Constant widget width within each group via fixed `Slider`/`DragValue` width

## Splitters
Two draggable splitters:
1. **`instrument_split`** — existing left/right split between instrument list and main area (unchanged)
2. **`instrument_settings_split`** — new horizontal split within the right panel, dividing the settings/maps section (top) from the envelope editor section (bottom). Default: 0.40.

## Wiring

### Function signature
```rust
pub fn draw_instrument_editor(
    ui: &mut egui::Ui,
    module: &Module,
    selected_instrument: &mut usize,
    selected_sample: &mut usize,
    theme: &TrackerTheme,
    playback_state: &AtomicPlaybackState,
    instrument_split: &mut f32,
    instrument_settings_split: &mut f32,
) -> Option<InstrumentEditEvent>
```

### State (persistent IDs in egui data)
- `instrument_env_type` — `EnvelopeType` (unchanged)
- `env_hovered` — `Option<usize>` (unchanged)
- `env_generator_open` — `bool` (unchanged)
- `sample_map_paint_idx` — `u8` (unchanged)
- `sample_browser_open` — `bool` (unchanged)
- `instrument_settings_split` — `f32` stored in `app.rs` config (new)

### App.rs call site
Two changes in `app.rs`:
1. Pass `&mut self.instrument_settings_split` as the new parameter
2. Initialize `instrument_settings_split: f32` in `HtrkApp` (default 0.40)

## Files Changed

| File | Change |
|---|---|
| `src/app.rs` | Add `instrument_settings_split` field, pass to `draw_instrument_editor` |
| `src/ui/instrument_editor.rs` | Restructure right panel into top settings + bottom envelope, 2-column grid, header row, maps row |
| `src/config.rs` (if exists) | Optionally persist `instrument_settings_split` |

## Testing
- All existing 278 unit tests must pass (no new data fields, pure UI reorganization)
- Manual visual verification: groups render correctly at various window sizes
- Divider drag works and state persists across tab switches

## Future (Out of Scope)
- Plugin/MIDI group (needs data model changes)
- Sample Quality group (needs data model changes)
- Per-instrument default panning (needs data model changes)
