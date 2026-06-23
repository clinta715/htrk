# HTRK Style Guide

Design token reference for HTRK's UI layer. Use these constants instead of
magic numbers to keep the UI visually consistent and theme-aware.

## 1. Typography Scale

Use these `f32` constants instead of inline `.size(N)` calls.

| Constant | Value | Where to use |
|----------|-------|-------------|
| `FONT_TITLE` | 16.0 | View titles — "INSTRUMENT 0A", "SAMPLE 01", tab/panel top-level names |
| `FONT_SECTION` | 13.0 | Section headers within panels (use `style::section_header()` helper) |
| `FONT_BODY` | 11.0 | Labels, status-bar text, dialog body, list items, file info |
| `FONT_DATA` | 12.0 | Pattern cell text (also the config-driven default cell font size) |
| `FONT_CAPTION` | 10.0 | Tooltips, hints, small control labels, info-bar metrics |
| `FONT_DETAIL` | 9.0 | File meta, axis labels, sub-detail annotations |
| `FONT_MICRO` | 7.0 | Oscilloscope / envelope axis marks, very dense displays |

**Example:**
```rust
// Before:
ui.label(egui::RichText::new("Hello").size(11.0).color(theme.fg_dim));

// After:
use crate::ui::style::FONT_BODY;
ui.label(egui::RichText::new("Hello").size(FONT_BODY).color(theme.fg_dim));
```

## 2. Spacing Scale

Use these `f32` constants instead of inline `.add_space(N)` or sizing calls.

| Constant | Value | Where to use |
|----------|-------|-------------|
| `SP_XS` | 2.0 | Minimal separation between adjacent elements |
| `SP_SM` | 4.0 | Default/compact padding inside panels |
| `SP_MD` | 8.0 | Standard gap between major sections |
| `SP_LG` | 12.0 | Large gap between groups / before a new section |
| `STATUS_BAR_H` | 22.0 | Fixed height of the status bar |
| `LIST_ROW_H` | 16.0 | Typical list/detail row height |

**Example:**
```rust
// Before:
ui.add_space(8.0);

// After:
use crate::ui::style::SP_MD;
ui.add_space(SP_MD);
```

## 3. UI Helpers

### `section_header(ui, text, theme)`

Draws a section header label: strong, `FONT_SECTION` size, colored with
`theme.fg_instrument`. Use this everywhere for consistency.

```rust
// Before:
ui.label(egui::RichText::new("Audio Defaults").size(13.0).strong()
    .color(egui::Color32::from_rgb(100, 200, 255)));

// After:
use crate::ui::style;
style::section_header(ui, "Audio Defaults", theme);
```

### `dialog(title, id)`

Builds a centered, non-collapsible egui `Window` with a given id. Mark
`.resizable(true)` only for dynamic-content dialogs (file browser, phrase
generator, etc.).

```rust
// Before:
egui::Window::new("Settings")
    .id(egui::Id::new("settings"))
    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
    .collapsible(false)
    .show(ctx, |ui| { ... });

// After:
use crate::ui::style;
style::dialog("Settings", "settings")
    .resizable(true)
    .show(ctx, |ui| { ... });
```

## 4. Color Tokens (`TrackerTheme`)

All color tokens live in `TrackerTheme` (`src/ui/theme.rs`). Access through
`theme.field_name` in any function that receives `&TrackerTheme`.

### Backgrounds

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `bg_default` | `#181820` | Default cell / panel background |
| `bg_highlight` | `#20202C` | Alternate row / hover highlight |
| `bg_measure` | `#282838` | Measure-boundary rows |
| `bg_selected` | `#3C3C78` | Selected cells |
| `bg_playback` | `#283C28` | Playback-track highlight |
| `bg_channel_alt` | `#1C1C24` | Channel-alternate rows |

### Foregrounds

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `fg_note` | `#FFFFFF` | Note name (e.g. `C-5`) |
| `fg_note_empty` | `#505064` | Empty note cell |
| `fg_note_off` | `#C8C850` | Note-off (`===`) |
| `fg_note_cut` | `#FF5050` | Note-cut (`^^^`) |
| `fg_instrument` | `#64C8FF` | Instrument column, section headers |
| `fg_volume` | `#64FF64` | Volume column |
| `fg_effect` | `#FFB450` | Effect column |
| `fg_effect_param` | `#FFC878` | Effect parameters |
| `fg_text` | `#C8C8DC` | General text |
| `fg_dim` | `#787887` | Dim/muted labels |
| `fg_dimmer` | `#5A5A69` | Lowest-priority text |

### Pattern Editor UI

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `cursor_outline` | `#FFFF00` | Cursor outline (yellow) |
| `cursor_fill` | `rgba(255,255,0,40)` | Cursor fill |
| `playback_cursor` | `#50C850` | Playback position cursor |
| `grid_line` | `#2D2D37` | Row grid lines |
| `grid_line_minor` | `#1E1E26` | Sub-row grid lines |

### Panels / Layout

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `panel_bg` | `#101012` | Panel background |
| `panel_border` | `#1E1E23` | Panel border |
| `splitter_bg` | `#28282D` | Splitter bar background |
| `splitter_border` | `#37373C` | Splitter bar border |
| `splitter_active` | `#46648C` | Splitter bar hover/active |

### Channel Headers

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `channel_header_bg` | `#28283C` | Header background |
| `channel_header_fg` | `#C8C8FF` | Header text |
| `channel_muted` | `#FF5050` | Muted indicator |
| `channel_solo` | `#50FF50` | Solo indicator |

### Transport / Status

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `transport_bg` | `#1E1E2A` | Transport bar background |
| `transport_fg` | `#C8C8F0` | Transport text |
| `transport_active` | `#50C850` | Active playback indicator |
| `status_bg` | `#1E1E2A` | Status bar background |
| `status_fg` | `#B4B4DC` | Status bar text |

### Order List

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `order_bg` | `#14141E` | Order list background |
| `order_fg` | `#B4B4DC` | Order list text |
| `order_selected` | `#FFFF00` | Selected order entry |
| `order_playing` | `#50FF50` | Playing order entry |

### VU Meter

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `vu_green` | `#28C846` | Low level |
| `vu_yellow` | `#D2BE28` | Medium level |
| `vu_red` | `#D74646` | Peak/clip level |
| `meter_bg` | `#141414` | Meter background |

### Pan / Send

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `pan_left` | `#6482FF` | Pan-left indicator |
| `pan_right` | `#FF6E6E` | Pan-right indicator |
| `pan_center` | `#BEBED2` | Pan-center indicator |
| `send_bus_colors[4]` | various | Bus color per slot |

### Automation

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `automation_overlay_bg` | `rgba(15,25,40,180)` | Overlay panel background |
| `automation_point` | `#78C8FF` | Automation point fill |
| `automation_value_text` | `#A0DCFF` | Value labels |
| `automation_curve` | `#64C8FF` | Curve line |
| `automation_curve_dim` | `rgba(60,120,160,80)` | Dim/faded curve |
| `automation_guide_line` | `rgba(80,120,160,100)` | Guide/drag lines |

### Misc

| Token | Dark Modern | Purpose |
|-------|-------------|---------|
| `scope_bg` | `#080808` | Oscilloscope background |
| `scope_cell_bg` | `#0C0C0E` | Scope cell background |
| `loop_marker` | `#00C8FF` | Loop start/end markers |
| `playback_position_dot` | `rgba(255,255,100,220)` | Position dot in timeline |
| `playback_position_line` | `rgba(255,100,100,180)` | Position line in editor |
| `envelope_colors[4]` | various | Envelope (fill, stroke) per point |
| `bg_sample_len` | `rgba(40,40,80,80)` | Sample-length overlay |
| `sample_len_shift` | `0.15` | Sample-length shading offset |

## 5. When NOT to Use Design Tokens

- **egui skin colors** (buttons, frames, scrollbars, window decorations) are
  handled automatically by `TrackerTheme::to_visuals()` → `ctx.set_visuals()`.
  Do not override widget-level colors manually.
- **Computed / dynamic colors** (e.g. HSV oscilloscope waveforms, peak meters
  with interpolated colors) should remain as inline math.
- **Sample palette** and **waveform editor** use their own distinct palette
  (green-toned in the default theme) that is intentionally separate from the
  tracker-grid color tokens. These are not plan to be migrated.
