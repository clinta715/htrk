# UI Design

## Overview

htrk uses egui/eframe for a modern tracker UI. The design prioritizes keyboard-driven
workflow (like original trackers) with modern mouse support for editing.

## Window Layout

```
┌──────────────────────────────────────────────────────────────────────────┐
│ Menu Bar: File | Edit | View | Pattern | Sample | Instrument | Settings │
├──────────────────────────────────────────────────────────────────────────┤
│ Transport Bar                                                            │
│ [◄◄] [►] [■] [►►] [●REC] │ Pat:003 Ord:005 │ BPM:[125] Spd:[6] │ 🔊  │
├───────────┬──────────────────────────────────────────────────────────────┤
│           │  Pattern Editor                                               │
│  Song     │  ┌ Ch1────────┬ Ch2────────┬ Ch3────────┬ Ch4────────┐     │
│  Order    │  │Note Ins Vol│Note Ins Vol│Note Ins Vol│Note Ins Vol│     │
│           │  │ Eff  Param │ Eff  Param │ Eff  Param │ Eff  Param │     │
│  00 ── 00 │  ├────────────┼────────────┼────────────┼────────────┤     │
│  01 ── 01 │ 0│C-5  01  40│...  ..  ..│A#3  02  32│...  ..  ..│     │
│  02 ── 02 │ 1│...  ..  ..│D-3  01  ..│...  ..  ..│F-4  02  40│     │
│  03 ── 01 │ 2│E-5  01  50│...  ..  ..│C-4  02  ..│...  ..  ..│     │
│  04 ── 03 │ 3│...  ..  ..│...  ..  ..│...  ..  ..│...  ..  ..│     │
│  05 ── 00 │ 4│G-4  01  ..│F-3  01  ..│...  ..  ..│D#4  02  ..│     │
│  06 ── 04 │ ... (scrollable)                                            │
│  07 ── 02 │ ...                                                          │
│  08 ── 00 │ 60│...  ..  ..│...  ..  ..│...  ..  ..│...  ..  ..│     │
│  09 ── 05 │ 61│C-5  01  40│...  ..  ..│G-4  02  ..│...  ..  ..│     │
│  10 ── 01 │ 62│...  ..  ..│...  ..  ..│...  ..  ..│...  ..  ..│     │
│  11 ── 03 │ 63│...  ..  ..│...  ..  ..│...  ..  ..│...  ..  ..│     │
│  ...      │  └────────────┴────────────┴────────────┴────────────┘     │
│           │                                                              │
│  [+Ins]   │  Channel Headers (click to mute/solo)                       │
│  [ -Del]  │  [M][S] Ch1: Bass   [M][S] Ch2: Pad   ...                  │
│  [^Up]    │                                                              │
│  [vDn]    │                                                              │
├───────────┴──────────────────────────────────────────────────────────────┤
│ [Pattern] [Samples] [Instruments]                                         │
├──────────────────────────────────────────────────────────────────────────┤
│ Bottom Panel (context-sensitive based on active tab)                      │
│                                                                          │
│ Samples Tab:                                                             │
│ ┌──────────────────────────────────────────────────────────────────────┐│
│ │ Sample List          │ Waveform Display                               ││
│ │ 01: bass             │ ▁▂▃▅▆█▇▅▃▂▁▁▂▃▅▆█▇▅▃▂▁▁▂▃▅▆█▇▅▃▂▁       ││
│ │ 02: pad              │         [████████]  ← loop markers            ││
│ │ 03: kick             │                                                ││
│ │ 04: snare            │ Properties:                                    ││
│ │ 05: hihat            │ Vol: [64] Pan: [32] Loop: [Forward]          ││
│ │ 06: lead             │ RelNote: [0] FineTune: [0]                   ││
│ │                      │ LoopStart: [8000] LoopEnd: [12000]           ││
│ └──────────────────────┴────────────────────────────────────────────────┘│
│                                                                          │
│ Instruments Tab:                                                         ││
│ ┌──────────────────────────────────────────────────────────────────────┐││
│ │ Instrument List      │ Envelope Editor (Vol/Pan/Pitch)                │││
│ │ 01: Bass             │    ┌───┐                                       │││
│ │ 02: Pad              │   ╱     ╲________                             │││
│ │ 03: Drums            │  ╱                ╲───────                    │││
│ │ 04: Lead             │ ─                   sustain─                  │││
│ │                      │    Sample Map Grid (Octaves x Notes):          │││
│ │                      │    C- C# D- D# E- F- F# G- G# A- A# B-         │││
│ │                      │ 0 [01][01][01][01][01][01][01][01][01][01] ... │││
│ │                      │ 1 [01][01][02][02][02] ... (10 octaves)        │││
│ │                      │ NNA: [NoteCut ▼] DCC: [Note ▼] Fade: [256]     │││
│ └──────────────────────┴────────────────────────────────────────────────┘││
├──────────────────────────────────────────────────────────────────────────┤
│ htrk v0.1 │ IT format │ Pat 03 Row 32/64 │ 8ch │ CPU: 1.2% │ MEM: 4MB  │
└──────────────────────────────────────────────────────────────────────────┘
```

## Panel Layout (egui)

```rust
impl HtrkApp {
    fn ui_layout(&mut self, ctx: &egui::Context) {
        // Top: Menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            self.draw_menu_bar(ui);
        });

        // Top: Transport bar
        egui::TopBottomPanel::top("transport_bar").show(ctx, |ui| {
            self.draw_transport(ui);
        });

        // Left: Song order list
        egui::SidePanel::left("order_list")
            .min_width(120.0)
            .default_width(150.0)
            .show(ctx, |ui| {
                self.draw_order_list(ui);
            });

        // Center: Pattern editor
        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_pattern_editor(ui);
        });

        // Bottom: Tab panel (samples/instruments)
        egui::TopBottomPanel::bottom("bottom_panel")
            .min_height(200.0)
            .default_height(250.0)
            .show(ctx, |ui| {
                self.draw_bottom_panel(ui);
            });

        // Very bottom: Status bar
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(24.0)
            .show(ctx, |ui| {
                self.draw_status_bar(ui);
            });
    }
}
```

## Pattern Grid Widget

### Cell Display Format

Each channel occupies a fixed-width column in the pattern grid:

```
Channel column layout (fixed-width, monospace):
┌─────────────┐
│C-5 01 40 A0F│  ← Full cell
│^^^ .. .. ....│  ← Note off, no instrument
│... .. .. ....│  ← Empty cell
│=== .. .. ....│  ← Note cut
└─────────────┘

Column positions (character offsets within cell):
  0-2: Note       (C-5, C#5, ---, ^^^, ===)
  3:   space
  4-5: Instrument (01-FF, ..)
  6:   space
  7-8: Volume     (00-40, ..)
  9:   space
  10:  Effect type(A-Z, .)
  11-12: Effect param (00-FF, ..)
Total: 13 characters per channel + 1 separator = 14 chars
```

### Sub-Column Navigation

The cursor can be positioned within a cell at 8 sub-column positions:

```
Position: Note | InsH | InsL | VolH | VolL | FxTyp | FxParH | FxParL
Char:      C-5    0      1     4      0      A       0        F
Offset:    0-2    4      5     7      8      10      11       12
```

### Colors

```rust
struct TrackerTheme {
    // Background
    bg_default: Color32,          // Default row background
    bg_highlight: Color32,        // Every 4th or 8th row (configurable)
    bg_selected: Color32,         // Selected cells
    bg_playback: Color32,         // Current playback row
    bg_channel_alt: Color32,      // Alternating channel background

    // Text colors
    fg_note: Color32,             // Note text
    fg_note_empty: Color32,       // "---" empty note
    fg_note_off: Color32,         // "^^^" note off
    fg_note_cut: Color32,         // "===" note cut
    fg_instrument: Color32,       // Instrument number
    fg_volume: Color32,           // Volume value
    fg_effect: Color32,           // Effect type letter
    fg_effect_param: Color32,     // Effect parameter

    // Cursor
    cursor_outline: Color32,      // Cursor rectangle outline
    cursor_fill: Color32,         // Cursor fill (semi-transparent)

    // Channel header
    channel_header_bg: Color32,
    channel_header_fg: Color32,
    channel_muted: Color32,       // Muted channel indicator

    // Order list
    order_bg: Color32,
    order_fg: Color32,
    order_selected: Color32,
    order_playing: Color32,
}

// Default modern dark theme
impl Default for TrackerTheme {
    fn default() -> Self {
        TrackerTheme {
            bg_default:  Color32::from_rgb(24, 24, 32),
            bg_highlight: Color32::from_rgb(32, 32, 44),
            bg_selected: Color32::from_rgb(60, 60, 120),
            bg_playback: Color32::from_rgb(40, 60, 40),
            bg_channel_alt: Color32::from_rgb(28, 28, 36),

            fg_note: Color32::from_rgb(255, 255, 255),
            fg_note_empty: Color32::from_rgb(80, 80, 100),
            fg_note_off: Color32::from_rgb(200, 200, 80),
            fg_note_cut: Color32::from_rgb(255, 80, 80),
            fg_instrument: Color32::from_rgb(100, 200, 255),
            fg_volume: Color32::from_rgb(100, 255, 100),
            fg_effect: Color32::from_rgb(255, 180, 80),
            fg_effect_param: Color32::from_rgb(255, 200, 120),

            cursor_outline: Color32::from_rgb(255, 255, 0),
            cursor_fill: Color32::from_rgba_premultiplied(255, 255, 0, 40),

            channel_header_bg: Color32::from_rgb(40, 40, 60),
            channel_header_fg: Color32::from_rgb(200, 200, 255),
            channel_muted: Color32::from_rgb(255, 80, 80),

            order_bg: Color32::from_rgb(20, 20, 30),
            order_fg: Color32::from_rgb(180, 180, 220),
            order_selected: Color32::from_rgb(255, 255, 0),
            order_playing: Color32::from_rgb(80, 255, 80),
        }
    }
}
```

### Rendering Approach

The pattern grid is rendered using egui's `Painter` API for maximum performance:

```rust
fn draw_pattern_grid(&mut self, ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            self.visible_channels as f32 * CHAR_WIDTH * 14.0,
            VISIBLE_ROWS as f32 * ROW_HEIGHT,
        ),
        egui::Sense::click_and_drag(),
    );

    let painter = ui.painter_at(rect);

    // Calculate visible row range based on scroll
    let first_row = self.scroll_offset_row;
    let last_row = (first_row + VISIBLE_ROWS).min(current_pattern.num_rows);

    for row in first_row..last_row {
        let y = rect.top() + (row - first_row) as f32 * ROW_HEIGHT;
        let is_highlight = row % 4 == 0;
        let is_playback = row == self.playback_row && self.is_playing;

        // Draw row background
        let bg = if is_playback {
            theme.bg_playback
        } else if is_highlight {
            theme.bg_highlight
        } else {
            theme.bg_default
        };
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), y),
                egui::pos2(rect.right(), y + ROW_HEIGHT),
            ),
            0.0,
            bg,
        );

        // Draw row number
        painter.text(
            egui::pos2(rect.left() + 2.0, y + ROW_HEIGHT / 2.0),
            egui::Align2::LEFT_CENTER,
            format!("{:03}", row),
            egui::FontId::monospace(FONT_SIZE),
            theme.fg_note_empty,
        );

        // Draw cells for each visible channel
        for ch in 0..self.visible_channels {
            let x = rect.left() + ROW_NUM_WIDTH + ch as f32 * CHANNEL_WIDTH;
            let cell = pattern.cell(row, ch);
            draw_cell(&painter, x, y, cell, ch, row, &theme);
        }
    }

    // Draw cursor
    if let Some(cursor) = self.cursor_visible() {
        draw_cursor(&painter, cursor, &theme);
    }

    // Draw selection rectangle
    if let Some(sel) = &self.selection {
        draw_selection(&painter, sel, &theme);
    }
}
```

## Waveform Display

The sample editor waveform is rendered using `egui::Painter` line segments:

```rust
fn draw_waveform(
    painter: &egui::Painter,
    rect: egui::Rect,
    sample: &Sample,
    view_start: usize,
    view_end: usize,
    loop_start: usize,
    loop_end: usize,
    theme: &TrackerTheme,
) {
    let data = &sample.data;
    let view_len = view_end - view_start;
    let pixels_available = rect.width() as usize;

    // Decimation: determine samples per pixel
    let samples_per_pixel = (view_len / pixels_available).max(1);

    // Draw center line
    let center_y = rect.center().y;
    painter.line_segment(
        [egui::pos2(rect.left(), center_y), egui::pos2(rect.right(), center_y)],
        egui::Stroke::new(1.0, Color32::from_rgb(60, 60, 80)),
    );

    // Draw waveform as min/max pairs per pixel for accuracy
    let mut points: Vec<egui::Pos2> = Vec::with_capacity(pixels_available * 2);

    for px in 0..pixels_available {
        let start = view_start + px * samples_per_pixel;
        let end = (start + samples_per_pixel).min(view_end);

        let mut min_val = 1.0f32;
        let mut max_val = -1.0f32;

        for i in start..end {
            let val = data.get(i).copied().unwrap_or(0.0);
            min_val = min_val.min(val);
            max_val = max_val.max(val);
        }

        let x = rect.left() + px as f32;
        let y_min = center_y - max_val * (rect.height() / 2.0);
        let y_max = center_y - min_val * (rect.height() / 2.0);

        painter.line_segment(
            [egui::pos2(x, y_min), egui::pos2(x, y_max)],
            egui::Stroke::new(1.0, Color32::from_rgb(0, 200, 255)),
        );
    }

    // Draw loop markers
    if loop_start >= view_start && loop_start <= view_end {
        let x = rect.left() + ((loop_start - view_start) as f32 / view_len as f32) * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(2.0, Color32::from_rgb(255, 255, 0)),
        );
    }
    if loop_end >= view_start && loop_end <= view_end {
        let x = rect.left() + ((loop_end - view_start) as f32 / view_len as f32) * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(2.0, Color32::from_rgb(255, 255, 0)),
        );
    }
}
```

## Envelope Editor

Graphical point-based envelope editor with drag support:

```
        Volume Envelope
    64 ┤  ╭──╮
       │ ╱    ╲
    48 ┤╱      ╲
       │        ╲
    32 ┤         ╲___________
       │                     ╲
    16 ┤                      ╲───
       │
     0 ┼──┬──┬──┬──┬──┬──┬──┬──┬──┬──► ticks
          L  S     E  L
          ^loop     ^loop
             ^sustain

Legend:
  ● = draggable point
  L = loop start/end markers
  S = sustain point
```

```rust
fn draw_envelope_editor(
    ui: &mut egui::Ui,
    envelope: &mut Envelope,
    theme: &TrackerTheme,
) -> EnvelopeEditResponse {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 120.0),
        egui::Sense::click_and_drag(),
    );

    let painter = ui.painter_at(rect);
    let max_tick = envelope.points.last().map(|p| p.tick).unwrap_or(64).max(64);
    let tick_scale = rect.width() / max_tick as f32;
    let value_scale = rect.height() / 64.0;

    // Draw grid
    for tick in (0..=max_tick).step_by(8) {
        let x = rect.left() + tick as f32 * tick_scale;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(0.5, Color32::from_rgb(50, 50, 70)),
        );
    }

    // Draw envelope line
    if envelope.points.len() >= 2 {
        let points: Vec<egui::Pos2> = envelope.points.iter().map(|p| {
            egui::pos2(
                rect.left() + p.tick as f32 * tick_scale,
                rect.bottom() - p.value as f32 * value_scale,
            )
        }).collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(2.0, theme.fg_note)));
    }

    // Draw draggable points
    let mut dragged_point = None;
    for (i, point) in envelope.points.iter().enumerate() {
        let pos = egui::pos2(
            rect.left() + point.tick as f32 * tick_scale,
            rect.bottom() - point.value as f32 * value_scale,
        );

        let point_rect = egui::Rect::from_center_size(pos, egui::vec2(12.0, 12.0));
        let point_response = ui.interact(point_rect, ui.id().with("env_point").with(i), egui::Sense::drag());

        if point_response.dragged() {
            dragged_point = Some(i);
        }

        let color = if envelope.sustain_point == Some(i) {
            Color32::from_rgb(255, 255, 0)   // Yellow = sustain
        } else if envelope.loop_start == Some(i) || envelope.loop_end == Some(i) {
            Color32::from_rgb(0, 255, 0)     // Green = loop
        } else {
            Color32::from_rgb(255, 255, 255)  // White = normal
        };

        painter.circle_filled(pos, 5.0, color);
        painter.circle_stroke(pos, 5.0, egui::Stroke::new(1.0, Color32::BLACK));
    }

    EnvelopeEditResponse { dragged_point }
}
```

## Keyboard Shortcuts

### Pattern Editor — Navigation

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move cursor up/down one row |
| `←` / `→` | Move cursor left/right one sub-column |
| `Tab` | Move cursor to next channel |
| `Shift+Tab` | Move cursor to previous channel |
| `PgUp` / `PgDn` | Scroll pattern up/down by 16 rows |
| `Home` | Jump to row 0 |
| `End` | Jump to last row |
| `Ctrl+Home` | Jump to first channel |
| `Ctrl+End` | Jump to last channel |
| `Ctrl+↑` / `Ctrl+↓` | Scroll visible channels left/right |

### Pattern Editor — Note Entry

Lower keyboard row (octave N):
```
  Z  S  X  D  C  V  G  B  H  N  J  M  ,
  C  C# D  D# E  F  F# G  G# A  A# B  C+1
```

Upper keyboard row (octave N+1):
```
  Q  2  W  3  E  R  5  T  6  Y  7  U
  C  C# D  D# E  F  F# G  G# A  A# B
```

| Key | Action |
|-----|--------|
| `Z` - `,` (lower row) | Play note in current octave |
| `Q` - `U` (upper row) | Play note in current octave + 1 |
| Note keys | Enter note AND advance cursor to next row |
| `1` - `9`, `0` | Enter hex digit in volume/effect columns |
| `A` - `F` | Enter hex digit in volume/effect columns |
| `Delete` / `Backspace` | Clear current cell |
| `.` (period) | Enter note-off (^^^) |
| `Ctrl+.` | Enter note-cut (===) |

### Pattern Editor — Octave

| Key | Action |
|-----|--------|
| `Ctrl+Z` | Octave down |
| `Ctrl+X` | Octave up |
| `Numpad *` | Octave down |
| `Numpad /` | Octave up |
| `Ctrl+1`-`Ctrl+9` | Set octave directly |

### Pattern Editor — Editing

| Key | Action |
|-----|--------|
| `Ctrl+C` | Copy selection |
| `Ctrl+X` | Cut selection |
| `Ctrl+V` | Paste at cursor |
| `Ctrl+Z` | Undo |
| `Ctrl+Y` | Redo |
| `Insert` | Insert empty row |
| `Ctrl+Insert` | Insert row across all channels |
| `Ctrl+Delete` | Delete row across all channels |
| `Shift+↑/↓` | Extend selection up/down |
| `Shift+←/→` | Extend selection left/right |
| `Ctrl+A` | Select all |
| `Escape` | Clear selection |

### Transport

| Key | Action |
|-----|--------|
| `F5` | Play from beginning |
| `F6` | Play from current position |
| `F7` | Play from current pattern start |
| `F8` | Stop |
| `Space` | Play/Stop toggle |
| `Ctrl+Space` | Play from current row |
| `F9` | Pause/Resume |

### Song Order List

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate orders |
| `Enter` | Edit pattern number |
| `+` / `-` | Increment/decrement pattern number |
| `Insert` | Insert new order |
| `Delete` | Delete order |
| `Ctrl+↑` / `Ctrl+↓` | Move order up/down |

### General

| Key | Action |
|-----|--------|
| `Ctrl+N` | New song |
| `Ctrl+O` | Open file |
| `Ctrl+S` | Save file |
| `Ctrl+Shift+S` | Save as... |
| `Ctrl+Q` | Quit |
| `F11` | Toggle follow-playback mode |
| `F12` | Toggle channel mute (on selected channel) |

## Mouse Interactions

### Pattern Grid

| Action | Behavior |
|--------|----------|
| Click | Move cursor to clicked cell/sub-column |
| Drag | Extend selection from click point |
| Double-click | Select entire row or column |
| Right-click | Context menu (copy/paste/insert/delete) |
| Scroll wheel | Scroll rows up/down |
| Ctrl+Scroll | Scroll channels left/right |
| Middle-click + drag | Pan the pattern view |

### Waveform Display

| Action | Behavior |
|--------|----------|
| Click | Set playback cursor position |
| Drag | Select region |
| Double-click | Zoom to selection / zoom to fit |
| Scroll wheel | Zoom in/out |
| Shift+Scroll | Scroll horizontally |
| Drag loop marker | Move loop start/end point |

### Envelope Editor

| Action | Behavior |
|--------|----------|
| Click point | Select point |
| Drag point | Move point (constrained to grid) |
| Double-click | Add new point |
| Right-click point | Delete point (if not first/last) |
| Right-click between points | Context menu: set sustain, set loop point |

### Sample Map Grid

| Action | Behavior |
|--------|----------|
| Click note | Assign current paint sample to note |
| Drag over notes | Paint current sample over range of notes |
| Scroll | Change paint sample index |

## Menu Structure

```
File
  ├─ New Song                  Ctrl+N
  ├─ Open...                   Ctrl+O
  ├─ Recent Files ─────────────►
  ├─ ───────────────────────
  ├─ Save                      Ctrl+S
  ├─ Save As...                Ctrl+Shift+S
  ├─ Export as WAV...
  ├─ ───────────────────────
  ├─ Preferences...            Ctrl+P
  └─ Quit                      Ctrl+Q

Edit
  ├─ Undo                      Ctrl+Z
  ├─ Redo                      Ctrl+Y
  ├─ ───────────────────────
  ├─ Cut                       Ctrl+X
  ├─ Copy                      Ctrl+C
  ├─ Paste                     Ctrl+V
  ├─ Delete                    Delete
  ├─ Select All                Ctrl+A
  ├─ ───────────────────────
  ├─ Insert Row                Insert
  ├─ Delete Row                Ctrl+Delete
  ├─ ───────────────────────
  ├─ Transpose +1              Ctrl+T
  ├─ Transpose -1              Ctrl+Shift+T
  ├─ Transpose +12             Ctrl+Shift+Up
  └─ Transpose -12             Ctrl+Shift+Down

View
  ├─ Follow Playback           F11
  ├─ ───────────────────────
  ├─ Pattern Editor
  ├─ Sample Editor
  ├─ Instrument Editor
  ├─ ───────────────────────
  ├─ Increase Visible Channels
  ├─ Decrease Visible Channels
  ├─ ───────────────────────
  └─ Theme ───────────────────►
      ├─ Dark Modern (default)
      ├─ Dark Retro
      └─ Light

Pattern
  ├─ New Pattern
  ├─ Duplicate Pattern
  ├─ Delete Pattern
  ├─ ───────────────────────
  ├─ Resize Pattern...
  ├─ ───────────────────────
  ├─ Interpolate Selection
  └─ Randomize Selection

Sample
  ├─ Import Sample...
  ├─ Export Sample...
  ├─ ───────────────────────
  ├─ New Sample
  ├─ Delete Sample
  ├─ ───────────────────────
  ├─ Normalize
  ├─ Amplify...
  ├─ Reverse
  ├─ Crop to Selection
  └─ Convert to 16-bit / 8-bit

Instrument
  ├─ New Instrument
  ├─ Duplicate Instrument
  ├─ Delete Instrument
  └─ ───────────────────────
      ├─ Import from Sample
      └─ Split by Key Zones...

Settings
  ├─ Audio Device...
  ├─ MIDI Device...
  ├─ Keybindings...
  └─ About htrk
```

## Font Configuration

```rust
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Load monospace font for pattern grid
    fonts.font_data.insert(
        "tracker_font".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/PxPlus_IBM_VGA8.ttf"
        ))),
    );

    // Set as highest-priority monospace font
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "tracker_font".to_owned());

    ctx.set_fonts(fonts);
}
```

## Responsive Layout

The UI adapts to different window sizes:

| Window Width | Channels Visible | Bottom Panel |
|-------------|-----------------|-------------|
| < 800px | 4 | Collapsed (toggle) |
| 800-1200px | 8 | Shown (shared tab) |
| 1200-1600px | 12 | Shown (shared tab) |
| > 1600px | 16+ | Split view possible |

The pattern grid uses horizontal scrolling for channels beyond visible count.
Vertical scrolling is smooth, with optional "follow playback" mode that auto-scrolls
to keep the current playback row visible.
