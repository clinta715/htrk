# UI Shrink Phase 2 — `app.rs::ui()` Method Extraction — 2026-06-24

## Goal

Shrink `app.rs::ui()` from ~693 lines to ~50 lines by extracting panel setup code and
tab event handlers into standalone methods on `HtrkApp`. No behavioral changes, no new
types, no file moves.

## Prerequisite

The 2026-06-16 spec ("UI Decomposition") extracted per-view state into panel structs
(`PatternView`, `SampleEditor`, `InstrumentEditor`, `SendFxPanel`, `PlaybackView`,
`AutomationEditor`, `MixerState`) and introduced the `PanelEvent` dispatch system.
That work is complete — each tab's rendering is delegated to `self.xxx.ui(…)`.

The remaining bulk in `ui()` is the **event-response code** after each panel/ui call
and the **panel setup boilerplate** (menu bar, transport, oscilloscope, status bar,
order list).

## Approach

Pure extraction — no behavioral change, no new types, no file moves. Each logical
section of `ui()` becomes a `fn handle_xxx(…)` method on `impl HtrkApp`.

### Panels Extracted (5 methods)

| Method | Lines | What it encapsulates |
|--------|-------|---------------------|
| `handle_menu_bar(ui, ctx)` | 1652–1786 | `draw_menu_bar()` call + all response handling (new/open/save/undo/redo/copy/paste/theme/spacing/quit) + `pending_device_switch`/`pending_reinit` |
| `handle_transport_bar(ui)` | 1788–1803 | `draw_transport()` call + prev/next pattern navigation |
| `handle_oscilloscope(ui, ctx)` | 1805–1817 | `draw_oscilloscope()` call with size computation |
| `handle_status_bar(ui)` | 1819–1846 | `draw_status_bar()` call + sample delta handling |
| `handle_order_list(ui, playback_order, playback_row, playback_tick, playback_speed)` | 1848–1941 | `draw_order_list()` call + order CRUD (insert/delete/duplicate/reorder/resize) with Module Mutation Pattern |

### Tab Event Handlers (7 methods)

| Method | Lines | What it encapsulates |
|--------|-------|---------------------|
| `handle_pattern_tab(ui, playback_pattern, playback_row, playback_tick, playback_speed)` | 1966–2054 | `self.pattern_view.ui()` call + PanelEvent loop (AddChannel, RemoveChannel, SetAutomationTarget, ContextMenuAction, etc.) |
| `handle_sample_tab(ui)` | 2055–2076 | `self.sample_editor.ui()` call + SelectionUpdate handling |
| `handle_instrument_tab(ui)` | 2077–2120 | `self.instrument_editor.ui()` call + Save/Load/Export/Import event handling |
| `handle_sendfx_tab(ui, frame, ctx)` | 2121–2197 | `self.sendfx_panel.ui()` call + plugin browser dialog + load/install logic |
| `handle_playback_tab(ui, playback_pattern, playback_row, playback_tick, playback_speed)` | 2198–2228 | `self.playback_view.ui()` call with resolved fallback pattern |
| `handle_automation_tab(ui)` | 2229–2281 | `self.automation_editor.ui()` call + all auto-response handling (track_added, point_changed, interp_changed, etc.) |
| `handle_mixer_tab(ui)` | 2282–2307 | `self.mixer_state.ui()` call with plugin slot + return level collection |

### Remaining Inline in `ui()` (~50 lines)

1. **Preliminaries** (1624–1650): ctx clone, viewport capture, plugin scan, close-requested intercept, `draw_preamble()`
2. **CentralPanel wrapper** (1943): `egui::CentralPanel::default().show_inside(…)`
3. **No-module guard** (1944–1951): placeholder text when no module is loaded
4. **Tab buttons** (1954–1962): 7 `selectable_value` buttons in a horizontal layout
5. **Tab dispatch** (1965): `match self.current_view { … }` calling the 7 handle methods
6. **Epilogue** (2311–2315): save window dims, `draw_dialogs(&ctx)`

### What `ui()` Will Look Like

```rust
fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
    let ctx = ui.ctx().clone();
    // preliminaries
    let (playback_row, playback_order, playback_pattern, playback_tick, playback_speed) = self.draw_preamble(&ctx);

    self.handle_menu_bar(ui, &ctx);
    self.handle_transport_bar(ui);
    self.handle_oscilloscope(ui, &ctx);
    self.handle_status_bar(ui);
    self.handle_order_list(ui, playback_order, playback_row, playback_tick, playback_speed);

    egui::CentralPanel::default().show_inside(ui, |ui| {
        if self.core.module.is_none() {
            ui.vertical_centered(|ui| { ui.heading("htrk - tracker"); … });
            return;
        }
        ui.horizontal(|ui| { /* tab buttons */ });
        match self.current_view {
            AppView::Pattern    => self.handle_pattern_tab(ui, …),
            AppView::Sample     => self.handle_sample_tab(ui),
            AppView::Instrument => self.handle_instrument_tab(ui),
            AppView::SendFx     => self.handle_sendfx_tab(ui, frame, &ctx),
            AppView::Playback   => self.handle_playback_tab(ui, …),
            AppView::Automation => self.handle_automation_tab(ui),
            AppView::Mixer      => self.handle_mixer_tab(ui),
        }
    });
    // epilogue
    self.draw_dialogs(&ctx);
}
```

## Testing

- No new tests needed — this is pure extraction with zero behavioral change.
- All 351 existing tests must pass after the refactor.
- The binary must compile without warnings.

## Risks

- **Borrow checker**: Some extracted methods access `&mut self` for the app state AND
  `&self` fields at the same time, which could cause borrow conflicts. Pre-extraction
  audit of each method's borrows is required.
- **Parameter proliferation**: `handle_pattern_tab` and `handle_playback_tab` take
  multiple playback-state parameters. Mitigation: keep these as simple positional
  params extracted from `draw_preamble`.
- **Non-panel code inside panels**: Lines 1776–1786 (`pending_device_switch` /
  `pending_reinit`) live between the menu bar panel and transport bar panel. They
  need to be absorbed into `handle_menu_bar` since they logically follow from menu
  bar actions.

## Order of Implementation

1. `handle_sample_tab` — simplest (22 lines)
2. `handle_mixer_tab` — next simplest (26 lines)
3. `handle_playback_tab` — straightforward (31 lines)
4. `handle_instrument_tab` — small event match (44 lines)
5. `handle_automation_tab` — event-match pattern (53 lines)
6. `handle_transport_bar` — tiny panel (16 lines)
7. `handle_oscilloscope` — tiny panel (13 lines)
8. `handle_status_bar` — small panel (28 lines)
9. `handle_pattern_tab` — biggest tab handler (89 lines)
10. `handle_sendfx_tab` — most complex (77 lines + plugin dialog)
11. `handle_order_list` — biggest panel (94 lines, mutation-heavy)
12. `handle_menu_bar` — biggest panel (135 lines, mutation-heavy)
13. Trim `ui()` down to final ~50-line orchestration shape
14. Build + verify 351 tests pass
