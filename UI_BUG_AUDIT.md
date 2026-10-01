# UI Bug Audit & Structural Fix Plan (2026-07)

Context: many UI bugs; a text UI was considered but rejected (would require
reinventing every element). Decision: **stay on egui and fix the structural
causes**. This document catalogs what the audit found and the plan to fix it.

The through-line: most UI bugs are NOT in rendering code. They come from four
fragile patterns — (1) in-place mutation of egui's input event queue,
(2) hand-maintained dialog flag lists, (3) untyped egui temp storage, and
(4) frame-diff state in immediate mode. Fixing those patterns removes whole
bug classes instead of patching symptoms.

---

## Bug class 1 — Input pipeline fragility (highest impact)

**Where:** `src/actions/keyboard.rs` (1,647 lines)

The key handler is a 6-stage priority chain:

```
handle_keyboard_input (keyboard.rs:248)
  → handle_alt_menu      (Alt tap / Alt+letter / menu-nav strip)
  → handle_early_text    (Event::Text before focus gate)
  → handle_tab           (Tab channel-steal from egui)
  → focus gate (has_focus early-return)
  → handle_ctrl / handle_ctrl_shift / handle_plain_key
  → event-strip retain() (Tab/Arrow removal, pattern view only)
```

Findings:

- **F1.1 Event-queue surgery in 4 places.** `ctx.input_mut(...).events.retain(...)`
  at `keyboard.rs:84` (Alt+menu keys), `:187` (menu-nav keys), `:291`
  (Tab/Arrow strip), `:1013` (Tab steal). Handlers both *read* and *delete*
  events, so correctness depends on call order. AGENTS.md §11 documents a
  regression (`58265b3`) caused by exactly this. Any new handler that runs in
  the wrong slot re-breaks navigation.
- **F1.2 `any_dialog_open()` is a hand-maintained 12-way OR**
  (`src/app/editing.rs:380-393`): `file_browser.show`, `settings_state.open`,
  `wav_export_state.open`, `sample_export_dialog.is_some()`, `show_about`,
  `show_shortcuts`, `show_exit_confirm`, `show_phrase_generator`,
  `slice_dialog_open`, `sendfx_panel.plugin_browser_open_for.is_some()`,
  `instrument_editor.plugin_browser_open`, `sample_library_state.open`.
  A new dialog that forgets this list silently allows cell edits underneath
  it (the exact bug §11 fixed once already).
- **F1.3 Alt-menu state machine spread over 5 app fields**
  (`menu_bar_active`, `active_menu`, `force_open_menu`, `alt_prev_frame`,
  `alt_intercepted`, plus `alt_l_count`/`alt_l_last`). Press/release edge
  detection via `prev_frame` booleans is the classic immediate-mode
  off-by-one source (§24).
- **F1.4 16 `Event::Text`/`Event::Key` match sites** in one file with
  overlapping key spaces (note preview vs text editing vs Ctrl shortcuts,
  see §13 pitfall).

## Bug class 2 — Untyped egui temp storage (90 call sites, 8 files)

`ui.data()` / `get_temp` / `insert_temp` / `load_temp` used in:
`automation_editor.rs`, `instrument_editor.rs`, `phrase_generator_dialog.rs`,
`plugin_browser.rs`, `sample_map.rs`, `sample_palette.rs`, `sendfx_editor.rs`,
`waveform.rs`.

Problems: values are untyped `Any` keyed by hash IDs, invisible in struct
definitions, lost on restart, and collide when an ID changes shape. §25
already migrated envelope state OUT of temp storage into `AppConfig` —
the same treatment is pending for ~8 files. Related: 94
`Id::new`/`make_persistent_id`/`id_salt` sites; the `plugin_param_scroll`
bug (§25) was a scroll-ID instability caused by auto-generated IDs.

## Bug class 3 — Dialog & layout state sprawl

- **F3.1 12+ dialog flags on `HtrkApp`** in three different shapes: plain
  `bool` (`show_about`, `slice_dialog_open`, `show_phrase_generator`, …),
  `Option<T>` (`sample_export_dialog`), and nested state
  (`settings_state.open`, `file_browser.show`). Opening/closing logic is
  scattered; Escape priority order is hand-maintained (§11).
- **F3.2 Layout-stability bugs** (visible content toggling shifts panels):
  the playback-grid jump (§13) required a permanent fallback rule. Any view
  that renders conditionally can re-introduce this.

## Bug class 4 — Panic paths in UI code (8 unwraps)

| Location | Expression |
|---|---|
| `src/ui/mixer_view.rs:131` | `core.module.as_ref().unwrap()` |
| `src/ui/envelope_editor.rs:200` | `line_pts.last().unwrap()` |
| `src/ui/instrument_editor.rs:103` | `module.instruments.get(i).unwrap()` |
| `src/ui/mixer_view.rs:243,256` | `"ABCD".chars().nth(bus).unwrap()` |
| `src/ui/sample_library.rs:101,108,123` | `library.read()/write().unwrap()` (RwLock poison = panic) |

`mixer_view.rs:131` is reachable without a module loaded if the view is
switched before the guard; the RwLock unwraps turn any poisoned lock into a
UI crash.

## Bug class 5 — Immediate-mode frame diffs (20 sites)

`prev_*` / `last_frame` trackers (e.g. `sample_editor.last_sample_index`,
`prev_selected_instrument` in `instrument_editor.rs:54`) diff frames to detect
changes. Wrong initial values cause one-frame reset flashes; §25's palette
scroll-reset is this class.

## What is NOT the problem

- Rendering code (waveform, oscilloscope, pattern grid paint) is largely
  healthy; §18's allocation discipline shows it has been tuned.
- The core logic layer is already UI-free: `HtrkCore`
  (`src/core/mod.rs`), 30+ undoable `EditCommand` types
  (`src/edit/commands.rs`), and `execute_mutation()` with 49 handlers
  (`src/mcp/mutations/`) form a complete command API. The bugs live in the
  glue between egui events and this core — not in the core.
- The audio engine and format loaders are unaffected.

---

## Fix plan (structural, incremental, testable)

### P1 — Single-pass input routing (kills F1.1, F1.3, F1.4)
Restructure `handle_keyboard_input` into:
1. **Capture pass**: read all events ONCE into a `Vec<InputIntent>` (typed
   enum: `EditChar(char)`, `Nav(Arrow)`, `Chord(Modifiers, Key)`, …) with a
   per-intent `owner` decided in one place (widget focus / dialog / app).
2. **Dispatch pass**: handlers consume intents; they never touch
   `ctx.input_mut`.
3. Delete all 4 `events.retain` sites. Event suppression becomes "the intent
   was owned", not "delete from the shared queue".
4. Fold the Alt-menu state machine into one `MenuNavState` struct with its
   own `handle_event(&mut self, &InputIntent)`.

Acceptance: arrow/Tab navigation works in pattern view, in dialogs, and in
widgets (the §11 + §24 + `58265b3` scenarios) — encode each as a smoketest
luau case under `smoketest/`.

### P2 — Dialog registry (kills F1.2, F3.1)
Replace the 12 flags with one `enum Dialog` + `Vec<Dialog>` stack on
`HtrkApp`:
- `any_dialog_open()` becomes `!stack.is_empty()` — impossible to forget.
- Escape pops the top; open/close sites become `push`/`pop`.
- Per-dialog state (settings, export, browser) moves into the variant payload.

### P3 — Typed UI state structs (kills class 2)
Migrate each `get_temp`/`insert_temp` pair to a named field on the owning
panel struct (`SampleEditor`, `InstrumentEditor`, `AutomationEditor`, …),
with `Default` + `#[serde(default)]` on `AppConfig` where persistence is
wanted (pattern already established in §25). Keep `id_salt` on any
`ScrollArea` whose content height changes (§25 rule).

### P4 — De-panic the UI (kills class 4)
`current_pattern_or_default()`-style fallbacks for the 8 unwraps;
`RwLock::read().unwrap()` → `if let Ok(...)` with a status message.

### P5 — Layout-stability lint rule (prevents F3.2 recurrence)
Add to AGENTS.md §18-style rules: every `CollapsingHeader`/conditional view
must reserve its strip or use a stable fallback (the playback grid rule
generalized). Consider a debug assertion in dev builds that logs panel
position changes frame-to-frame.

### P6 — Regression harness coverage
The `smoketest/` luau suite already drives the real UI. Add focused cases:
dialog-open blocks cell editing; Tab/arrow in each dialog; Alt-menu open/close
cycle; widget-focus + note preview coexistence (§11).

Also: `tests/golden_render.rs` (untracked WIP) hangs on its `xm_linear` /
`xm_amiga` cases (single `#[test]` runs all 5 sequentially, so case-level
bisection needs an edit). Full loop audit of the render/tick path
(`renderer.rs`, `sequencer_engine/mod.rs`, `effects/{xm,legacy,shared}.rs`,
`voice_pool.rs`, `clock.rs`, `period.rs`, `mixer.rs`, `resampler.rs`) found
every `while`/`loop` bounded — the stall is a non-progress/NaN-style stall or
pathological slowdown in the XM period-model tick path, not a visible infinite
loop. `legacy_htk` reproduces its pinned hash exactly and completes fine, so
the XM case is a pre-existing engine/test issue (predates the UI work; touches
no UI code). Not a UI bug — track separately when picking up renderer work.

---

## Sequencing

P2 and P4 are small and independent — do first (immediate bug reduction).
P1 is the high-value structural change; P3 parallelizes with it. P5/P6 are
ongoing discipline. After P1+P2 the "text UI" question can be revisited
cheaply: the input-intent layer and command API are exactly what a future
alternative frontend would bind to — nothing here is wasted either way.
