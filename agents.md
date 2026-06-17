# Agent Guidelines for htrk

To prevent regressions in the sequencer, audio engine, and effect processing pipeline, follow these rules when modifying playback or format logic:

## 1. Row State Isolation
- **Mandate**: Every row starts with a clean slate for non-persistent effects.
- **Rule**: All continuous effect flags (in `ActiveEffects`), per-row counters (retrigger, tremor, note cut), and delayed cell pointers MUST be reset in `SequencerEngine::advance_row`.
- **Exception**: "Memory" values (e.g., `last_portamento_speed`) should persist, but the *active* flag for the slide must not carry over unless the pattern data contains a new command.

## 2. Note Delay (EDx / SDx) Handling
- **Mandate**: Delayed notes must trigger correctly even on "silent" channels.
- **Rule**: Do not rely on an active `Voice` to handle `NoteDelay`. Always store the delayed `Cell` in the `ChannelState` and trigger it from the sequencer loop on the target tick.
- **Verification**: If a note is delayed, ensure tick 0 still processes global effects and volume column updates from that cell, but defers the actual voice trigger.

## 3. Volume and Panning Resets
- **Mandate**: Instrument changes must reset channel properties to defaults.
- **Rule**: When a new instrument/sample is triggered (including delayed notes), the channel volume MUST be reset to the sample's `default_volume` unless a volume command on the same row overrides it.
- **Regression Check**: Ensure volume slides from the previous row do not affect the starting volume of a new note.

## 4. Testing Requirements
- **Mandate**: No fix is complete without an empirical reproduction test.
- **Rule**: When fixing a sequencer bug, add a unit test to `src/audio/sequencer_engine.rs` that simulates the specific pattern/row transition that failed. Use `advance_row()` and `advance()` in tests to verify state transitions.

## 5. Module Mutation Pattern
- **Mandate**: Never use `Arc::get_mut()` directly on `self.module` without first ensuring unique ownership, and always sync to the audio engine after mutation.
- **Rule**: Always call `self.ensure_module_ownership()` before any `Arc::get_mut()` call on the module. Always call `self.sync_module_to_audio()` after mutation. `ensure_module_ownership()` only clones the module if needed (does NOT send to audio engine). `sync_module_to_audio()` sends `LoadModule` to the audio engine with the current (mutated) Arc.
- **Pattern**:
  ```rust
  self.ensure_module_ownership();
  if let Some(ref mut module) = self.module {
      if let Some(arc_module) = Arc::get_mut(module) {
          // mutate arc_module safely
      }
  }
  self.sync_module_to_audio();
  ```
- **Why**: The audio engine always holds a clone of `Arc<Module>`, so `Arc::get_mut()` returns `None` unless ownership is ensured first. After mutation, the audio engine must be updated with `sync_module_to_audio()` so it picks up the changes.

## 6. SequencerClock

Timing state (BPM, speed, current_tick, samples_per_tick, sample_counter, auto_tempo_factor) is encapsulated in `SequencerClock` at `src/audio/sequencer/clock.rs`, accessed as `state.clock.xxx`.

- Use `state.clock.set_bpm()`, `state.clock.set_speed()`, `state.clock.reset()` instead of direct field writes — these handle internal recalculation.
- The `process_tick()` method uses `clock.on_tick_processed()` which increments `current_tick` and returns `true` when the row should advance.
- The advance loops in `engine.rs` and `renderer.rs` access `state.clock.samples_per_tick` and `state.clock.sample_counter` directly.

## 7. Processor dispatch pattern

NEVER use `std::mem::replace` directly to call effect processor methods. Instead use the `with_processor_mut` helper:

```rust
// OLD — do not use:
let mut processor = std::mem::replace(&mut self.processor, EffectProcessor::from_module(&Module::default()));
processor.apply_effect(self, channel, effect, true);
self.processor = processor;

// NEW:
self.with_processor_mut(|processor, engine| processor.apply_effect(engine, channel, effect, true));
```

The helper (`sequencer_engine.rs:470`) temporarily swaps the processor, calls the closure, and restores it — avoiding the borrow conflict between `self.processor` and `&mut self`.

## 8. Effect Architecture: Universal, Format-Specific, and Native

### Layer 1 — Universal Effects
The `Effect` enum (`src/sequencer/effect.rs`) defines the universal internal representation shared across all formats. These are format-agnostic commands (Arpeggio, VolumeSlide, TonePortamento, etc.) that every format loader decodes INTO and every format encoder decodes FROM.

### Layer 2 — Format-Specific Effects
When a legacy format has commands or quirks that cannot be cleanly represented in the universal `Effect` enum, use `Effect::FormatSpecific(FormatEffect)` with the appropriate sub-enum (`XmEffect`, `ModEffect`, `S3mEffect`, `ItEffect`). This isolates format quirks:
- A MOD-specific effect must never interfere with XM playback and vice versa.
- The sequencer dispatches format-specific effects only when the matching format is active.
- Each format's loader is responsible for encoding its native commands into the correct universal OR format-specific effect.

### Layer 3 — Native Effects (HTRK Superset)
Beyond the universal and format-compatibility layers, HTRK provides a superset of native commands that users composing in HTRK (`FormatType::Htk`) can treat as the standard toolkit. These include extended filter control, fine-grained MIDI parameter automation, and other modern features that have no legacy equivalent. Native effects should:
- Be available regardless of which legacy format is loaded (they enhance, not replace).
- Never conflict with or override format-specific compatibility behavior.
- Serve as the recommended command set for users whose interest is native HTRK composition rather than accurate legacy reproduction.

### Format-Conditional Processing Rules
When the sequencer processes a universal effect that needs format-specific behavior (e.g., vibrato uses period-domain math for MOD but frequency-domain for XM), use `module.flags.linear_slides` or `self.use_xm_model` to branch:
- Each branch MUST be tested independently.
- Changes to the non-XM path (`!linear_slides`) MUST NOT alter the XM path (`linear_slides`).
- Changes to the XM path MUST NOT alter the non-XM path.
- After modifying any format-conditional code, run the FULL test suite and verify both XM and MOD/S3M playback.

## 10. File-Browser Columns and Preview

### Column Layout (List / Details View)
- **Mandate**: Detail columns (duration, type, size, modified) must use fixed-width cells with truncation to prevent overlapping text.
- **Rule**: Always use `ui.add_sized([width, 14.0], Label::new(...).truncate())` (or a `detail_cell` helper) instead of `ui.set_width()` + `right_to_left` layout for right-aligned columns. The first column (name) uses `selectable_label` and fills remaining width.
- **Widths**: Dur=56px, Type=44px, Size=64px, Modified=76px (List view); same widths are used for Details view header + data rows.
- **Why**: `right_to_left` + `set_width` does not clip text when filenames are long; fixed-size `add_sized` enforces clipping.

### Audio Preview in File Browser
- **Architecture**: `AudioCommand::PreviewBuffer` carries raw PCM (`Arc<Vec<f32>>`) + sample rate. The audio engine's `trigger_preview_buffer` method decodes via `compute_playback_frequency` into the preview voice (index 255), same path as `trigger_preview_note`.
- **Caching**: `FileBrowser.get_preview_data()` stores `Option<(PathBuf, Arc<Vec<f32>>, u32)>` in `self.preview_sample`. WAV is decoded once per path via `formats::wav::import_wav`. Cache is cleared on `refresh()` (navigation).
- **Wiring**: `HtrkApp::preview_browser_sample(note_key)` checks `self.file_browser.show && mode == Samples`, loads the selected entry, decodes via `get_preview_data`, sends `PreviewBuffer`. Keyboard handler calls this before `preview_note` in `handle_text_input`; if it returns `true`, note recording is skipped. The Preview button in the browser footer sets `preview_requested` flag, handled after render.
- **Verification**: No existing audio tests should break; `trigger_preview_buffer` shares the same voice-pool infrastructure as `trigger_preview_note`.

## 11. Keyboard Focus Gate and Note-Preview Path

### The Problem
When any egui widget has keyboard focus, the `handle_keyboard_input` function in `src/actions/keyboard.rs` must still allow `Event::Text` to reach note preview (for qwerty keyboard preview), while blocking cursor/editing keys (arrows, backspace, delete, insert, space, etc.) that would corrupt the widget's own editing state.

### The Fix (commit `51604c8`)
- **Scope `any_dialog_open` at function level**, before the `ctx.input()` closure, so all branches (including `Event::Text` and Ctrl+Shift modifiers) see the same value.
- **Process `Event::Text` in a separate early `ctx.input()` pass** — before the main focus gate — so text events reach `note_key_preview_only()` regardless of widget focus.
- **Gate destructive keys** (`Arrow*`, `Space`, `Insert`, `Backspace`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, `Tab`) on `!any_dialog_open` inside their match arms.
- **Extract the Delete arm** into `delete_row()` and `delete_cell_or_automation()` helpers to eliminate deep nesting that obscured the `any_dialog_open` scoping bug.
- **Remove the stale `Event::Text` handler** from the main match block (the early `ctx.input()` pass now handles it).

### Rule for Future Changes
- `Event::Text` must always be handled **before** the focus gate, because it carries both widget text input AND note-preview keystrokes.
- Never compute `any_dialog_open` inside `ctx.input()` — compute it outside so all branches agree on the same dialog-state snapshot.
- When adding a new keyboard shortcut that should work regardless of widget focus, add it to a pre-gate `ctx.input()` pass, not the main match block.

## 12. Save-on-Exit Confirmation

- **`show_exit_confirm: bool`** on `HtrkApp` gates the dialog.
- Intercept `ctx.input(|i| i.viewport().close_requested())` early in `ui()`; if dirty and not already confirming, `CancelClose + show_exit_confirm = true`.
- The dialog in `draw_dialogs()` has three buttons: Save → `save_current_file + Close`, Don't Save → `Close`, Cancel → `show_exit_confirm = false`.
- File > Quit Ctrl+Q menu item and `Ctrl+Q` keybind follow the same path: if dirty, set flag; if clean, `Close` immediately.
- `module_dirty()` returns `false` safely when no module is loaded.

## 13. Session Pitfalls (2026-06-16)

### Avoid
- **`#[serde(default)]` on `AppConfig` fields does NOT exempt you from adding the field to `Default::default()`**: The `Default` impl for `AppConfig` is hand-written (not derived), so new fields must be added both with `#[serde(default = "...")]` and in the `Default` block. The compiler error is an immediate `missing field` in the struct literal.
- **Playback tab grid must always be visible**: If the pattern grid is only rendered when `playback_pattern` is `Some`, the channel blocks and info footer jump downward when playback starts and back up when it stops. Always resolve a fallback pattern (e.g. the selected editing pattern) so the layout is stable regardless of playback state.
- **Ctrl+ shortcuts and note preview keys share `Key` variants**: `Ctrl+Q` and plain `Q` are independent because `Ctrl` suppresses `Event::Text`. New Ctrl+ shortcuts added via `Event::Key` inside the `modifiers.ctrl` block will never collide with note preview (which runs on `Event::Text`).

## 14. Virtual Pattern Auto-Creation

### The Problem
When the order list references a pattern index that doesn't exist in `module.patterns` (e.g., user types a pattern number beyond the allocated range), the pattern editor shows a blank grid, edits silently fail, and playback stops at that position.

### The Solution
- **`ensure_pattern_exists(&mut self)`** on `HtrkCore` grows `module.patterns` with blank 64-row `Pattern`s until the currently selected pattern index is valid, then syncs to audio. Call this **before any edit operation** that writes to a pattern (set cell, paste, transpose, insert/delete row, fill instrument, interpolate, etc.).
- **`current_pattern_or_default(&self)`** on `HtrkCore` returns a reference to the real pattern if it exists, or a static default 64-row blank `Pattern`. Use this for **read-only** access (cursor bounds, copy, grid rendering) when a blank fallback is acceptable.
- **Edit commands** (`SetCellCommand`, `BulkSetCellsCommand`, etc.) use `ensure_pattern()` / `ensure_pattern_by_index()` helpers in `commands.rs` that grow `module.patterns` before accessing it. Undo also calls these helpers, so undoing an edit on a previously-virtual pattern works correctly.
- **Pattern view** always renders using `current_pattern_or_default()`, so the grid is always visible even when the pattern hasn't been materialized yet.

### Rule for Future Changes
- Any code path that **writes** to a pattern must call `ensure_pattern_exists()` before the write.
- Any code path that **reads** a pattern for display/copy can use `current_pattern_or_default()`.
- Never use `current_pattern().unwrap()` — always use `current_pattern_or_default()` or handle the `None` case.

## 15. Phrase Generator Architecture

### Modes
`GenMode` enum in `src/tools/phrase_generator.rs`: `Melodic`, `Euclidean`, `Drum`, `Chord`. Each mode has its own `generate_*()` function that returns `Vec<(row, channel, Cell)>`.

### Chord Mode
- `ChordType` enum: `Triad` `[0,4,7]`, `Seventh` `[0,4,7,11]`, `Sus2` `[0,2,7]`, `Sus4` `[0,5,7]`.
- `Progression` enum: `OneFourFiveOne`, `OneFiveSixFour`, `OneSixFourFive`, `OneThreeFourFive`, `Circle`.
- `generate_chord()` places each chord on separate channels via `chord_channels` (default `[0,1,2,3]`), with a mid-bar retrigger.
- `chord_progression_degrees()` maps progressions to scale degree indices, branching on major/minor scale.

### Parameter Persistence
All phrase generator parameters persist via `egui::Id` temp storage (`ui.data()` / `ui.data_mut()`), NOT through `AppConfig`. Changes are lost on app restart.

### Adding New Modes
1. Add variant to `GenMode` and `GenMode::all()`.
2. Add any new param structs/enums with `name()`, `all()`, and `Default` impl.
3. Add fields to `PhraseParams` and its `Default`.
4. Implement `generate_*()` function returning `Vec<(usize, usize, Cell)>`.
5. Wire into `generate_phrase()` match arm.
6. Add UI controls in `src/ui/phrase_generator_dialog.rs` inside the `match mode` block.
7. Add persistent state IDs and `ui.data_mut()` save/restore for new fields.

## 16. Sample Editor Architecture

### Selection Model
- `SampleEditor.selection: Option<(usize, usize)>` stores (start, end) sample indices. The pair is unordered — always normalize with `.min()/.max()` before use.
- `handle_sample_edit()` returns `Option<SelectionUpdate>`: `Clear` (for Cut/Crop) or `Set(start, end)` (for operations that change sample length like Paste). Returning `None` preserves the existing selection (Normalize, Reverse, Fade, Silence).
- Non-destructive edits (Normalize, Reverse, Amplify, Silence, FadeIn, FadeOut) preserve selection. Destructive length-changing edits (Cut, Crop, Paste) update or clear selection.

### Waveform Rendering & Interaction
- `draw_waveform()` in `src/ui/waveform.rs` renders the waveform with zoom/scroll support.
- Zoom model: `zoom == 0.0` means fit-to-view (see entire sample). `zoom > 0` is the number of visible samples.
- Scroll model: `scroll_offset` is `[0.0, 1.0]` — position within the sample. 0 = start, 1 = end minus visible window.
- Zoom resets to fit-to-view when switching samples (`last_sample_index` tracking).
- Mouse wheel zooms, keeping the cursor position under the mouse stable.
- Drag creates selection. Shift+drag reserved for future scroll pan. Right-click opens context menu.
- `WaveformEvent` enum handles all interactions: loop marker drags, selection, and context menu actions (Cut, Copy, Paste, Crop, Silence, Normalize, Reverse, TrimSilence, SetLoopFromSelection, FadeIn, FadeOut, ZoomToSelection, ZoomFit).

### Zoom Controls
- Info bar shows position (sample + ms), selection range, peak/RMS dB, sample rate, and zoom percentage.
- **Fit** button: resets `zoom = 0.0` (fit-to-view).
- **Sel** button: sets `zoom` to selection length and positions `scroll_offset` to center selection.

### Keyboard Shortcuts (Sample Tab)
- `Ctrl+C`: Copy selection
- `Ctrl+X`: Cut selection
- `Ctrl+V`: Paste at cursor position
- `Ctrl+A`: Select all
- `Delete`: Silence selection

### Context Menu (Right-click on Waveform)
- Cut, Copy, Paste (enabled only when clipboard has data), Crop to Selection, Silence Selection, Normalize, Reverse, Trim Silence, Set Loop from Selection, Fade In, Fade Out, Zoom to Selection, Zoom Fit.

### Fade Processing
- `FadeIn(start, end)`: multiplies each sample by `i/len` (linear 0→1 ramp).
- `FadeOut(start, end)`: multiplies each sample by `1 - i/len` (linear 1→0 ramp).
- Both are undoable via `SetSampleDataCommand`.
- Non-destructive to sample length; selection is preserved.

### Cursor Position
- `SampleEditor.cursor_pos: Option<usize>` tracks the sample index under the mouse cursor.
- Reset to `None` when the waveform panel is hidden.
- The info bar displays cursor position in both sample index and milliseconds.
