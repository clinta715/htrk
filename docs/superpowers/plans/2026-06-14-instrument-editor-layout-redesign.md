# Instrument Editor Layout Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize the instrument editor UI into an OpenMPT-style layout with a 2-column settings grid on top and a full-width envelope editor on the bottom.

**Architecture:** Add a `instrument_settings_split` field to `HtrkApp` for the vertical divider, then reorder the existing groups in `instrument_editor.rs` into header, 2-column grid, maps row, and envelope section.

**Tech Stack:** Rust, egui, existing instrument editor at `src/ui/instrument_editor.rs`

---

### Task 1: Add `instrument_settings_split` field and wire parameter

**Files:**
- Modify: `src/app.rs:90` (field), `src/app.rs:166` (init)
- Modify: `src/app.rs:1394-1401` (call site)
- Modify: `src/ui/instrument_editor.rs:43-51` (function signature)
- Modify: `src/ui/instrument_editor.rs:66-137` (existing splitter code)

- [ ] **Step 1: Add field to HtrkApp**

In `src/app.rs` at line 90, add after `instrument_split`:

```rust
pub(crate) instrument_settings_split: f32,
```

In `src/app.rs` at line 166, add after `instrument_split: 0.15,`:

```rust
instrument_settings_split: 0.40,
```

- [ ] **Step 2: Add parameter to function signature**

In `src/ui/instrument_editor.rs:43-51`, add the parameter:

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
) -> Option<InstrumentEditEvent> {
```

- [ ] **Step 3: Pass field at call site**

In `src/app.rs:1401`, add after the `instrument_split` parameter:

```rust
                            &mut self.instrument_settings_split,
```

- [ ] **Step 4: Run cargo check**

Run: `cargo check`
Expected: builds successfully with only pre-existing warnings

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/ui/instrument_editor.rs
git commit -m "feat: wire instrument_settings_split divider parameter"
```

### Task 2: Restructure right panel with vertical divider

**Files:**
- Modify: `src/ui/instrument_editor.rs:64-592`

Replace the current right-panel layout (which is a single flat sequence of envelope group + bottom scroll rows) with a vertical split. Keep the header row above both sections. The envelope editor moves below the divider.

Current structure at line 140:

```rust
// ---- Header ----
ui.horizontal(|ui| { ... });

// ---- Envelope Editor ----
ui.group(|ui| {
    ui.set_min_height(250.0);
    // tabs, graph, toolbar
});

// ---- Bottom properties: two rows ----
ui.add_space(4.0);
// Row 1: NNA/DC + Volumes + Vibrato
// Row 2: Sample Map + Note Map
```

Replace with:

```rust
// ---- Header ----
ui.horizontal(|ui| {
    ui.heading(format!("Instrument {:02}:", *selected_instrument));
    let mut name = inst.name.clone();
    if ui.text_edit_singleline(&mut name).changed() {
        event = Some(InstrumentEditEvent::NameChanged(name));
    }
    ui.separator();
    if ui.button("Save...").clicked() {
        event = Some(InstrumentEditEvent::SaveInstrument);
    }
    if ui.button("Load...").clicked() {
        event = Some(InstrumentEditEvent::LoadInstrument);
    }
});

// ---- Top section: settings grid + maps ----
let top_id = ui.make_persistent_id("inst_settings_area");
egui::TopBottomPanel::top(top_id)
    .resizable(true)
    .default_height(200.0)
    .height_range(100.0..=f32::INFINITY)
    .show_inside(ui, |ui| {
        // 2-column settings grid (Task 3)
        // Maps row (Task 4)
    });

// ---- Bottom section: envelope editor ----
// envelope tabs + graph + toolbar (Task 5)
```

**However**, `TopBottomPanel::show_inside` may not work well inside an existing `ui.horizontal`. A simpler approach: use `ui.vertical()` with manual `allocate_rect` and a horizontal divider between top and bottom.

Actually, the simplest approach: wrap the entire right side in a `ui.vertical()`, allocate the top portion at a fraction of height, draw a splitter below it, then allocate the rest for the envelope editor. Use the existing `draw_vertical_splitter` pattern but for horizontal splits.

```rust
use crate::ui::draw_horizontal_splitter; // need to add this or inline

let total_h = ui.available_height();
let settings_h = total_h * *instrument_settings_split;

// --- Top: settings grid + maps ---
let (top_rect, _) = ui.allocate_space(egui::vec2(ui.available_width(), settings_h));
let mut top_ui = ui.child_ui(top_rect, *ui.layout());

// Draw settings grid in top_ui (Task 3)
draw_settings_grid(&mut top_ui, inst, theme, &mut event, *selected_instrument);

// --- Horizontal divider ---
crate::ui::draw_horizontal_splitter(ui, total_h, instrument_settings_split, 0.15, 0.70, theme);

// --- Bottom: envelope editor ---
let bottom_rect = egui::Rect::from_min_size(
    egui::pos2(top_rect.left(), top_rect.bottom() + 6.0),
    egui::vec2(ui.available_width(), ui.available_height() - settings_h - 6.0),
);
let mut bottom_ui = ui.child_ui(bottom_rect, *ui.layout());
draw_envelope_section(&mut bottom_ui, inst, env_type, ...);
```

But actually, the simpler approach: draw the top section with `ui.vertical(|ui| { ... })` limited in height, then a separator/draggable line, then the rest below.

Let me use the simplest approach that works:

```rust
// Available height in the right panel
let total_h = ui.available_height();
let split_y = (total_h * *instrument_settings_split).max(100.0).min(total_h - 150.0);

// --- Top section ---
let (top_rect, _) = ui.allocate_space(egui::vec2(ui.available_width(), split_y));
let mut top_ui = ui.child_ui(top_rect, *ui.layout());
// ... draw in top_ui

// --- Splitter ---
let splitter_rect = egui::Rect::from_min_size(
    egui::pos2(top_rect.left(), top_rect.bottom()),
    egui::vec2(top_rect.width(), 4.0),
);
let splitter_resp = ui.allocate_rect(splitter_rect, egui::Sense::drag());
if splitter_resp.dragged_by(egui::PointerButton::Primary) {
    let delta = splitter_resp.drag_delta().y;
    *instrument_settings_split = ((top_rect.height() + delta) / total_h)
        .clamp(0.15, 0.75);
}
// Visual: draw a 1px line
ui.painter_at(splitter_rect).line_segment(
    [egui::pos2(splitter_rect.left(), splitter_rect.center().y),
     egui::pos2(splitter_rect.right(), splitter_rect.center().y)],
    egui::Stroke::new(1.0, theme.grid_line_major),
);

// --- Bottom section ---
let bottom_y = splitter_rect.bottom();
let bottom_h = total_h - bottom_y;
let (bottom_rect, _) = ui.allocate_space(egui::vec2(ui.available_width(), bottom_h));
let mut bottom_ui = ui.child_ui(bottom_rect, *ui.layout());
// ... draw envelope in bottom_ui
```

- [ ] **Step 1: Restructure the right panel with vertical split**

Replace the right-panel code (from line 140-end of the `if let Some(inst)` block at ~583) to use the splitter pattern above.

The `draw_settings_grid` and `draw_envelope_section` functions will be written in subsequent tasks. For now, leave them as inline placeholders that just render the existing content in the wrong spots — then we move content in later tasks.

Actually, since this is a pure reorganization, let me do it as a single large edit that moves everything into place in one task.

Alternative approach: write two helper functions and restructure the main body.

Let me revise: the file is 946 lines. The cleanest approach is:
1. Extract the settings grid into a helper function `draw_settings_grid`
2. Extract the envelope section into a helper function `draw_envelope_section`
3. Replace the main body with the vertical split + calls to the helpers

- [ ] **Step 1: Extract envelope section into helper function**

Move the envelope-related code (tabs + graph + toolbar, lines ~158-328) into a standalone function at the bottom of the file:

```rust
fn draw_envelope_section(
    ui: &mut egui::Ui,
    inst: &Instrument,
    env_type: &mut EnvelopeType,
    theme: &TrackerTheme,
    playback_state: &AtomicPlaybackState,
    selected_instrument: usize,
    generator_open: &mut bool,
) -> Option<InstrumentEditEvent> {
    // existing envelope tabs + graph + toolbar code
}
```

The function takes `env_type: &mut EnvelopeType` so state persistence remains in the data store. It returns `Option<InstrumentEditEvent>` for envelope events.

- [ ] **Step 2: Compile-check after extract**

Run: `cargo check`
Expected: builds successfully

- [ ] **Step 3: Extract settings grid into helper function**

Move the NNA + Volumes + Vibrato + Sample Map + Note Map code into:

```rust
fn draw_settings_grid(
    ui: &mut egui::Ui,
    inst: &Instrument,
    theme: &TrackerTheme,
    selected_instrument: usize,
) -> Option<InstrumentEditEvent> {
    // existing settings code arranged in 2-column grid
}
```

- [ ] **Step 4: Compile-check after extract**

Run: `cargo check`
Expected: builds successfully

- [ ] **Step 5: Restructure main body**

Replace the existing right-panel code (after the header) with:

```rust
// ---- Vertical split between settings and envelope ----
let total_h = ui.available_height();
let mut split_y = (total_h * *instrument_settings_split).max(100.0).min(total_h - 120.0);

// --- Top section: settings ---
let (top_rect, _) = ui.allocate_space(egui::vec2(ui.available_width(), split_y));
{
    let mut top_ui = ui.child_ui(top_rect, *ui.layout());
    if let Some(e) = draw_settings_grid(&mut top_ui, inst, theme, *selected_instrument) {
        event = Some(e);
    }
}

// --- Horizontal splitter ---
let splitter_rect = egui::Rect::from_min_size(
    egui::pos2(top_rect.left(), top_rect.bottom()),
    egui::vec2(top_rect.width(), 4.0),
);
let splitter_resp = ui.allocate_rect(splitter_rect, egui::Sense::drag());
if splitter_resp.dragged_by(egui::PointerButton::Primary) {
    let new_h = (top_rect.height() + splitter_resp.drag_delta().y).max(100.0).min(total_h - 120.0);
    *instrument_settings_split = new_h / total_h;
}
// Draw splitter line and grab handle
let painter = ui.painter_at(splitter_rect.expand2(egui::vec2(0.0, 2.0)));
painter.line_segment(
    [egui::pos2(splitter_rect.left(), splitter_rect.center().y),
     egui::pos2(splitter_rect.right(), splitter_rect.center().y)],
    egui::Stroke::new(1.0, theme.grid_line_major),
);

// --- Bottom section: envelope editor ---
let bottom_rect = egui::Rect::from_min_size(
    egui::pos2(top_rect.left(), splitter_rect.bottom()),
    egui::vec2(ui.available_width(), ui.available_height() - splitter_rect.bottom()),
);
{
    let mut bottom_ui = ui.child_ui(bottom_rect, *ui.layout());
    if let Some(e) = draw_envelope_section(
        &mut bottom_ui, inst, &mut env_type, theme, playback_state,
        *selected_instrument, &mut generator_open,
    ) {
        event = Some(e);
    }
}
```

- [ ] **Step 6: Compile-check**

Run: `cargo check`
Expected: builds successfully

- [ ] **Step 7: Commit**

```bash
git add src/ui/instrument_editor.rs
git commit -m "feat: split instrument editor into settings top + envelope bottom sections"
```

### Task 3: Build the 2-column settings grid

**Files:**
- Modify: `src/ui/instrument_editor.rs` (the `draw_settings_grid` function body)

Replace the existing inline settings code (currently NNA + Volumes + Vibrato + Sample Map + Note Map in scrollable rows) with a 2-column grid of framed groups.

Layout:

```
┌──────────────────────────────────────────────────┐
│ ┌───────────┐ ┌──────────┐ ┌───────────┐       │
│ │ General   │ │ Pitch-Pan│ │ Filter    │        │
│ │ Vol: OOO  │ │ Sep: OOO │ │ Cut: [   ]│        │
│ │ Fade: [  ]│ │ Ctr: [  ]│ │ Res: OOO  │        │
│ │           │ │          │ │ LP HP BP  │        │
│ └───────────┘ └──────────┘ └───────────┘       │
│ ┌───────────┐ ┌──────────┐ ┌───────────┐       │
│ │ NNA       │ │ Random   │ │ Vibrato   │        │
│ │ Cut/Cont..│ │ Vol: OOO │ │ Sine Ramp.│        │
│ │ DCT: ...  │ │ Pan: OOO │ │ Swp:[  ]  │        │
│ │ DNA: ...  │ │ Flt: OOO │ │ Dpt:[  ]  │        │
│ │           │ │          │ │ Rate:[  ] │        │
│ └───────────┘ └──────────┘ └───────────┘       │
├──────────────────────────────────────────────────┤
│ Paint:[Br] [palette] [keyboard] │ [note map]    │
└──────────────────────────────────────────────────┘
```

- [ ] **Step 1: Build the left column groups**

In `draw_settings_grid`, draw the first row of groups (General / Pitch-Pan / Filter) using `ui.columns(2)` or `ui.horizontal` with groups:

```rust
fn draw_settings_grid(
    ui: &mut egui::Ui,
    inst: &Instrument,
    theme: &TrackerTheme,
    selected_instrument: usize,
) -> Option<InstrumentEditEvent> {
    let mut event = None;

    // --- Left column groups ---
    ui.columns(2, |columns| {
        // Left column
        columns[0].vertical(|ui| {
            draw_group(ui, "General", theme, |ui| {
                // Global Volume
                let mut gvol = inst.global_volume;
                if ui.add(egui::Slider::new(&mut gvol, 0..=128).text("Vol")).changed() {
                    event = Some(InstrumentEditEvent::GlobalVolumeChanged(gvol));
                }
                // Fadeout
                let mut fade = inst.fade_out;
                if ui.add(egui::DragValue::new(&mut fade).range(0..=4095).speed(1).prefix("Fade: ")).changed() {
                    event = Some(InstrumentEditEvent::FadeoutChanged(fade));
                }
            });
            draw_group(ui, "Pitch-Pan", theme, |ui| {
                let mut sep = inst.pitch_pan_separation;
                if ui.add(egui::Slider::new(&mut sep, -32..=32).text("Sep")).changed() {
                    event = Some(InstrumentEditEvent::PitchPanSeparationChanged(sep));
                }
                let mut center = inst.pitch_pan_center;
                if ui.add(egui::DragValue::new(&mut center).range(0..=119).prefix("Ctr: ")).changed() {
                    event = Some(InstrumentEditEvent::PitchPanCenterChanged(center));
                }
            });
            draw_group(ui, "Filter", theme, |ui| {
                let mut cutoff = inst.filter_cutoff;
                if ui.add(egui::DragValue::new(&mut cutoff).range(0..=0xFFFF).speed(10).prefix("Cut: ")).changed() {
                    event = Some(InstrumentEditEvent::FilterCutoffChanged(cutoff));
                }
                let mut res = inst.filter_resonance;
                if ui.add(egui::Slider::new(&mut res, 0..=255).text("Res")).changed() {
                    event = Some(InstrumentEditEvent::FilterResonanceChanged(res));
                }
                // Filter type
                let ft = inst.filter_type;
                let mut ft_u8 = ft.to_u8();
                ui.horizontal(|ui| {
                    if ui.selectable_label(ft_u8 == 0, "LP").clicked() { ft_u8 = 0; }
                    if ui.selectable_label(ft_u8 == 1, "HP").clicked() { ft_u8 = 1; }
                    if ui.selectable_label(ft_u8 == 2, "BP").clicked() { ft_u8 = 2; }
                    if ui.selectable_label(ft_u8 == 3, "Notch").clicked() { ft_u8 = 3; }
                });
                let new_ft = crate::sequencer::effect::FilterType::from_u8(ft_u8);
                if new_ft != ft {
                    event = Some(InstrumentEditEvent::FilterTypeChanged(new_ft));
                }
            });
        });
        // Right column
        columns[1].vertical(|ui| {
            // ... right column groups
        });
    });

    // --- Maps row ---
    // ... sample map + note map

    event
}
```

Note: `draw_group` is a tiny helper:

```rust
fn draw_group(ui: &mut egui::Ui, label: &str, theme: &TrackerTheme, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).color(theme.fg_dim));
            content(ui);
        });
}
```

- [ ] **Step 2: Build the right column groups**

Right column: NNA / Random / Vibrato:

```rust
draw_group(ui, "NNA", theme, |ui| {
    use crate::sequencer::instrument::NewNoteAction;
    ui.horizontal(|ui| {
        if ui.selectable_label(inst.nna == NewNoteAction::NoteCut, "Cut").clicked() {
            event = Some(InstrumentEditEvent::NnaChanged(NewNoteAction::NoteCut));
        }
        if ui.selectable_label(inst.nna == NewNoteAction::Continue, "Cont").clicked() {
            event = Some(InstrumentEditEvent::NnaChanged(NewNoteAction::Continue));
        }
        if ui.selectable_label(inst.nna == NewNoteAction::NoteOff, "Off").clicked() {
            event = Some(InstrumentEditEvent::NnaChanged(NewNoteAction::NoteOff));
        }
        if ui.selectable_label(inst.nna == NewNoteAction::NoteFade, "Fade").clicked() {
            event = Some(InstrumentEditEvent::NnaChanged(NewNoteAction::NoteFade));
        }
    });
    // DCT
    ui.horizontal(|ui| {
        ui.label("DCT:");
        use crate::sequencer::instrument::DuplicateCheckType;
        if ui.selectable_label(inst.duplicate_check_type == DuplicateCheckType::Disabled, "Off").clicked() { ... }
        if ui.selectable_label(inst.duplicate_check_type == DuplicateCheckType::Note, "Note").clicked() { ... }
        if ui.selectable_label(inst.duplicate_check_type == DuplicateCheckType::Sample, "Samp").clicked() { ... }
        if ui.selectable_label(inst.duplicate_check_type == DuplicateCheckType::Instrument, "Inst").clicked() { ... }
    });
    // DNA
    ui.horizontal(|ui| {
        ui.label("DNA:");
        use crate::sequencer::instrument::DuplicateCheckAction;
        if ui.selectable_label(inst.duplicate_check_action == DuplicateCheckAction::NoteCut, "Cut").clicked() { ... }
        if ui.selectable_label(inst.duplicate_check_action == DuplicateCheckAction::NoteOff, "Off").clicked() { ... }
        if ui.selectable_label(inst.duplicate_check_action == DuplicateCheckAction::NoteFade, "Fade").clicked() { ... }
    });
});
draw_group(ui, "Random", theme, |ui| {
    let mut rvol = inst.random_volume;
    if ui.add(egui::Slider::new(&mut rvol, 0..=100).text("Vol")).changed() {
        event = Some(InstrumentEditEvent::RandomVolumeChanged(rvol));
    }
    let mut rpan = inst.random_panning;
    if ui.add(egui::Slider::new(&mut rpan, 0..=100).text("Pan")).changed() {
        event = Some(InstrumentEditEvent::RandomPanningChanged(rpan));
    }
    let mut frc = inst.filter_random_cutoff;
    if ui.add(egui::Slider::new(&mut frc, 0..=255).text("Flt")).changed() {
        event = Some(InstrumentEditEvent::FilterRandomCutoffChanged(frc));
    }
});
draw_group(ui, "Vibrato", theme, |ui| {
    ui.horizontal(|ui| {
        if ui.selectable_label(inst.vib_type == 0, "Sine").clicked() { ... }
        if ui.selectable_label(inst.vib_type == 1, "Ramp").clicked() { ... }
        if ui.selectable_label(inst.vib_type == 2, "Sq").clicked() { ... }
        if ui.selectable_label(inst.vib_type == 3, "Rand").clicked() { ... }
    });
    let mut sweep = inst.vib_sweep;
    if ui.add(egui::DragValue::new(&mut sweep).range(0..=255).speed(1).prefix("Swp: ")).changed() { ... }
    let mut depth = inst.vib_depth;
    if ui.add(egui::DragValue::new(&mut depth).range(0..=255).speed(1).prefix("Dpt: ")).changed() { ... }
    let mut rate = inst.vib_rate;
    if ui.add(egui::DragValue::new(&mut rate).range(0..=255).speed(1).prefix("Rate: ")).changed() { ... }
});
```

- [ ] **Step 3: Build the maps row**

Below the 2-column grid, add the sample map and note map:

```rust
ui.separator();
ui.horizontal(|ui| {
    // Sample Map
    ui.group(|ui| {
        ui.label("Paint Sample:");
        if ui.button("Browse...").clicked() { ... }
        // sample palette + keyboard
    });
    // Note Map
    ui.group(|ui| {
        // note map grid
    });
});
```

This is the same code as the existing Row 2 (lines 530-582), just positioned directly below the settings grid.

- [ ] **Step 4: Compile-check**

Run: `cargo check`
Expected: builds successfully

- [ ] **Step 5: Commit**

```bash
git add src/ui/instrument_editor.rs
git commit -m "feat: 2-column settings grid with framed groups in instrument editor"
```

### Task 4: Run full test suite

**Files:**
- None

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: all 278 tests + 3 integration tests pass

- [ ] **Step 2: Final commit if needed**

```bash
git add -A
git commit -m "chore: instrument editor layout redesign complete"
```
