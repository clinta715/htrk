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
When any egui widget has keyboard focus, or when any dialog is open, the `handle_keyboard_input` function in `src/actions/keyboard.rs` must still allow `Event::Text` to reach note preview (for qwerty keyboard preview), while blocking cursor/editing keys (arrows, backspace, delete, insert, space, etc.) that would corrupt the widget's own editing state or edit cells underneath a visible dialog.

### The Fix (commit `51604c8`)
- **Scope `any_dialog_open` at function level**, before the `ctx.input()` closure, so all branches (including `Event::Text` and Ctrl+Shift modifiers) see the same value.
- **Process `Event::Text` in a separate early `ctx.input()` pass** — before the main focus gate — so text events reach `note_key_preview_only()` regardless of widget focus.
- **Gate destructive keys** (`Arrow*`, `Space`, `Insert`, `Backspace`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, `Tab`) on `!any_dialog_open` inside their match arms.
- **Extract the Delete arm** into `delete_row()` and `delete_cell_or_automation()` helpers to eliminate deep nesting that obscured the `any_dialog_open` scoping bug.
- **Remove the stale `Event::Text` handler** from the main match block (the early `ctx.input()` pass now handles it).

### Rule for Future Changes
- `Event::Text` must always be handled **before** the focus gate, because it carries both widget text input AND note-preview keystrokes.
- When `any_dialog_open` is true, `Event::Text` must call `note_key_preview_only()` (not `handle_text_input()`) to prevent cell editing underneath a visible dialog.
- Never compute `any_dialog_open` inside `ctx.input()` — compute it outside so all branches agree on the same dialog-state snapshot.
- `any_dialog_open` MUST include every dialog in the app: `file_browser.show`, `settings_state.open`, `wav_export_state.open`, `sample_export_dialog.is_some()`, `show_about`, `show_shortcuts`, `show_exit_confirm`, `show_phrase_generator`, `slice_dialog_open`. When adding a new dialog, add it to this list.
- All pattern-editing keys (Ctrl+Z/Y/C/X/V/A, Alt+ editing combos, Delete, Backspace, Insert, arrows, Tab, Home/End, PageUp/Down) MUST be gated on `!any_dialog_open`.
- View-switching keys (F2/F3/F4) MUST be gated on `!any_dialog_open`; playback keys (F5-F9) are NOT gated (useful during dialogs).
- Octave (`[`/`]`), pattern navigation (`-`/`=`/`+`), Alt+Num cursor_skip, Alt+M/S/N channel controls MUST be gated on `!any_dialog_open`.
- Escape closes the topmost open dialog (in priority order) when `any_dialog_open`; only toggles `edit_mode` when no dialog is open.
- When adding a new keyboard shortcut that should work regardless of widget focus, add it to a pre-gate `ctx.input()` pass, not the main match block.
- **Event-strip ordering**: The `ctx.input_mut` that strips Tab/Arrow events from the queue so egui widgets don't react to them MUST run AFTER `handle_plain_key` (which reads those same events to move the cursor), NOT before it. It must also NOT run when a widget has focus (`has_focus == false` is guaranteed by the focus gate's early return). Placing the strip before the handlers breaks arrow navigation and Tab between columns. This was a regression from commit `58265b3` that was fixed by moving the strip to after `handle_plain_key`.

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

## 17. MCP Server Architecture (Phases 1-2)

### Transport
- TCP localhost on configurable port (default 18763).
- JSON-RPC 2.0 over newline-delimited TCP streams.
- Single server thread (`htrk-mcp`) polls for new connections and reads via non-blocking `read_line`.

### Thread Model
- **Server thread** (`src/mcp/server.rs`): Accepts connections, parses JSON-RPC, dispatches to tools/resources.
- **Read-only path**: Tools listed in `tools::call_tool` execute immediately on the MCP thread from `Arc<RwLock<>>` snapshots of module/playback/channel state. No main-thread involvement.
- **Mutation path**: When `tools::call_tool` returns `"Requires mutation dispatch"`, the server thread creates an `McpCommand { method, params, response_tx }` and sends it via `mpsc::Sender<McpCommand>` to the main thread. It then blocks on `response_rx.recv()` waiting for the result.
- **Main thread** (`HtrkApp::draw_preamble`): Drains `server.command_rx.try_recv()` in a loop, calling `crate::mcp::mutations::execute_mutation(&mut self.core, &cmd.method, &cmd.params)` for each pending command. Result is sent back via `cmd.response_tx.send()`.

### Mutation Tool Implementation (`src/mcp/mutations.rs`)
- `execute_mutation()` dispatches by `method` string to per-tool handler functions.
- Each handler follows the pattern:
  1. `core.ensure_module_ownership()`
  2. `if let Some(arc_module) = Arc::get_mut(module)` for direct mutations
  3. Or use `UndoManager::execute(Box::new(command), arc_module)` for undoable edits
  4. `core.sync_module_to_audio()` after mutation
- `MUTATION_TOOLS` constant in `tools.rs` lists all tools requiring main-thread dispatch.
- `mcp_enabled` and `mcp_port` fields in `AppConfig` control server lifecycle.
- Server started in `HtrkApp::default()` if `mcp_enabled == true`; stopped in `on_exit()`.

### Note Parsing
- `parse_note(s)` in `mutations.rs` accepts IT/XM note names (`C-5`, `D#4`, `---`, `===`, `^^^`, `~~~`) and bare MIDI key numbers (`60`).
- Effect hex strings like `"C02"`, `"A04"`, `"H83"` parsed via `parse_hex_effect()` into `Effect` enum variants.

### Snapshots
- Built in `draw_preamble()` after playback state is read.
- `ModuleSnapshot` stores module-level JSON plus pattern/instrument/sample entries (skipping sample PCM data).
- `PlaybackSnapshot` is rebuilt every frame (cheap: a handful of `AtomicU32` reads) so MCP clients see live transport position.
- `ModuleSnapshot` and `ChannelsSnapshot` are **gated behind a dirty flag** (`HtrkApp.mcp_last_module_ptr`): they are only rebuilt when the `Arc<Module>` pointer identity changes, which happens only when `ensure_module_ownership()` deep-clones the Module during an edit. Re-serializing the entire module to JSON every frame was a major per-frame cost and is now skipped when the module is unchanged.
- Read by MCP thread under `RwLock::read()` — never blocks main thread.
- When adding a new module-derived field to the snapshot, it belongs inside the `if module_dirty { ... }` block in `draw_preamble` so it is only rebuilt on edit. Transport/playback fields go outside the gate.

## 18. UI Style Conventions

### Design Tokens (`src/ui/style.rs`)
Use the FONT_* constants instead of inline `.size(N)`:
- `FONT_TITLE` (16.0) — view titles ("INSTRUMENT 0A", "SAMPLE 01")
- `FONT_SECTION` (13.0) — section headers (use `style::section_header()`)
- `FONT_BODY` (11.0) — labels, status bar, dialog body
- `FONT_DATA` (12.0) — pattern cell text
- `FONT_CAPTION` (10.0) — tooltips, hints, info-bar metrics
- `FONT_DETAIL` (9.0) — file meta, axis labels
- `FONT_MICRO` (7.0) — oscilloscope axis marks

Use the SP_* constants instead of inline `.add_space()`:
- `SP_XS` (2.0), `SP_SM` (4.0), `SP_MD` (8.0), `SP_LG` (12.0)

### UI Helpers
- Use `style::section_header(ui, text, theme)` for section headers.
- Use `style::dialog(title, id)` for centered, non-collapsible dialogs.
- Reference the full color token list in `STYLE.md`.

### Hardcoded Colors
- Prefer `theme.*` field access over `Color32::from_rgb(N, N, N)` literals.
- The only exceptions: computed/dynamic colors (HSV oscilloscope, interpolated peaks) and theme definition files.

### Rule for Changes
- When adding a new `.size(N)` call, use the corresponding FONT_* constant.
- When adding a new `section_header`-like label, use `style::section_header()`.
- When adding a colored label outside standard widgets, check if an existing `theme.*` token fits before creating a new literal.

### Per-Frame Allocation Discipline (Pattern Grid)
`draw_pattern_grid` runs every frame and iterates every visible cell (visible_rows × visible_channels × 5 sub-columns). String allocations inside this loop are the dominant per-frame heap traffic. Rules:
- `format_effect` returns `(&'static str, String)` — the effect-type column is always a `&str` literal; only the param column allocates. Use the `HEX_DIGITS: [&str; 16]` table in `pattern_grid.rs` for single hex-digit lookups instead of `format!("{:X}", n)`.
- `draw_cell` uses `Cow<'_, str>` for note/instrument/volume text: `Cow::Borrowed` for constant sentinels (`"==="`, `"---"`, `".."`), `Cow::Owned` only for dynamic `Note::On` / numeric values. `painter.text` accepts both.
- `ui.input(...)` must be read ONCE before the row×channel loop, never inside it — it locks egui input state per call.
- When borrowing module data for rendering (e.g. `channel_panning`), pass `&[T]` directly rather than `.clone()`-ing the `Vec` per frame.

## 19. Sample Library (Phase 1)

### Architecture
- `SampleLibrary` in `src/mcp/library.rs` is a per-session, in-memory directory browser with lazy WAV header caching.
- Stored on `McpServer` as `library: Arc<RwLock<SampleLibrary>>` and passed to all `ToolContext` instances.
- `library_roots: Vec<String>` in `AppConfig` persists root paths; applied to the library on startup in `HtrkApp::default()`.

### MCP Tools
- **Read-only** (MCP thread): `sample_library.configure` (set roots), `sample_library.list_dir` (browse + paginate), `sample_library.search` (filename substring match across cached entries).
- **Mutation** (main thread): `sample_library.import` (load WAV into module with optional `target_slot`, `name`, `set_note`).
- Library tools access the library via `ctx.library.write()` (configure/list_dir populate the cache) or `ctx.library.read()` (search reads the cache).

### Filename Heuristic Parser
- `parse_filename(name)` splits on `_`, ` `, `.` (NOT `-` — dashes are part of tracker note notation like `C-4`).
- Note detection: `[A-G][#b-]?[0-9]+` — supports sharps (`D#3`), flats (`Bb2`), and tracker dashes (`C-4`). Case-preserved in output (`Bb2`, not `BB2`).
- BPM detection: `\d+bpm` (case-insensitive suffix) or standalone 3-digit number.
- Category: all non-note, non-BPM tokens joined by space. Only set if at least one note or BPM was found (low confidence otherwise → all fields None).

### WAV Header Reading
- `read_wav_header(path)` reads only the RIFF/fmt/data chunks (no PCM data) to extract `duration`, `sample_rate`, `bit_depth`, `num_channels`. Same algorithm as `wav_duration()` in `file_browser.rs` but returns richer metadata.

### Caching
- `cache: HashMap<PathBuf, LibraryEntry>` — populated lazily on `list_dir` calls. Entry-level cache (per file), not directory-level.
- `dir_cache: HashMap<PathBuf, Vec<PathBuf>>` — child path lists per directory, avoiding repeated `read_dir`.
- Both caches cleared on `set_roots()`.
- No persistent index (SQLite) in Phase 1 — cold cache on every app restart.

### Rule for Future Changes
- When adding a new library tool, add it to `list_tools()` in `tools.rs`. If read-only, add a match arm in `call_tool()`. If mutation, add to `MUTATION_TOOLS` and `execute_mutation()` in `mutations.rs`.
- The `ToolContext` now carries `library: Arc<RwLock<SampleLibrary>>` — any new read-only tool can access it via `ctx.library`.
- Never call `std::fs::read` for full WAV data in a read-only library tool — use `read_wav_header` for metadata only. Full PCM loading only happens in `sample_library.import` (mutation, main thread).

## 20. CLAP/VST Plugin Hosting

### Architecture
- `src/audio/plugins/` — new module containing `HostedPlugin` trait, plugin discovery, library, and format-specific loaders.
- The audio engine talks to plugins via a format-agnostic trait; CLAP is implemented in `clap_plugin.rs`, VST3 (future) in `vst3_plugin.rs`.

### Threading Model
- **Main thread**: `ClapPluginHandle` owns the CLAP `PluginInstance` (which is `!Send` by design).
- **Audio thread**: `ClapPluginProcessor` owns the `StartedPluginAudioProcessor` (Send but !Sync).
- The processor is passed across threads via `AudioCommand`, but the PluginInstance stays on the main thread.
- When deactivating: audio thread returns a `StoppedPluginAudioProcessor` (boxed as `Any + Send`) back to the main thread; the main thread calls `instance.try_deactivate_with(closure)` to finalize.

### Traits
- `HostedPluginProcessor: Send` — audio-thread side. Must not allocate in `process()`.
- `HostedPluginHandle` — main-thread side. NOT `Send` (PluginInstance is !Send).
- All buffer fields are pre-allocated in `activate()` and reused on each `process()` call.

### Discovery (`src/audio/plugins/discovery.rs`)
- `default_search_paths()` returns OS-specific CLAP directories:
  - Windows: `C:\Program Files\Common Files\CLAP`
  - macOS: `/Library/Audio/Plug-Ins/CLAP`, `~/Library/Audio/Plug-Ins/CLAP`
  - Linux: `/usr/lib/clap`, `~/.clap`, `/usr/local/lib/clap`
- `scan_paths(roots)` walks the given directories and collects `.clap` files. Recursive, deduplicated via canonical paths.
- Per-session in-memory cache, cleared on `set_scan_roots()`.

### Library (`src/audio/plugins/library.rs`)
- `PluginLibrary` — in-memory metadata cache for discovered plugins, keyed by `(format, path, plugin_id)`.
- Mirrors `SampleLibrary` pattern. No persistent index yet (Phase 4+).

### Persistence
- `PluginSlot` struct in `mod.rs` (re-exported in sequencer module) holds `{ format, path, plugin_id, state }`.
- Phase 2 send FX integration will add `send_bus_plugins: [Option<PluginSlot>; 4]` to `Module`.

### CLAP Integration Status (Phases 1-5.1)
- **Phase 1-2 (done)**: trait, types, discovery, library, dependencies, real CLAP process() with AudioPorts/EventBuffer, send FX wiring, persistence (`Module.send_bus_plugins`), MCP tools.
- **Phase 5 (done, Windows only)**: plugin editor windows. `open_editor(mode, parent_hwnd)` probes floating first, falls back to embedded. macOS/Linux deferred.
- **Phase 5.1 (done)**: `HtrkHostShared` implements `HostLog` + `HostGui`. `tracing` integration with stderr default and optional file via `AppConfig.log_file_path`. `EditorMode` enum (Floating | Embedded). X-close detection via `IsWindowVisible` polling. Editor errors surfaced as red labels. F6 = Send FX, F7 = Automation.
- **Phase 5.2 (done)**: CLAP plugin parameter extension. `HostedPluginHandle::parameter_info()` enumerates params via clack `PluginParams` ext (id, name, min/max, IS_AUTOMATABLE, IS_MODULATABLE). `get_parameter`/`set_parameter` on the trait, routed through `param_ring: Arc<ParamRingBuffer>` (SPSC, 256 slots, lock-free) shared with `ClapPluginProcessor` which drains the ring in `process()` and pushes `ParamValueEvent`s to the plugin's input. New `AutomationTarget::PluginParam { send_bus, host_index, param_id }` variant (serde-skip) — sequencer queues automation values; audio engine routes them to the right plugin's ring after each `process_tick()`. Send FX view has a "Parameters" collapsible section with one slider per param. Persisted mapping: the `param_index_to_id` cache in `ClapPluginHandle` survives a load (re-enumerated on the next access). Currently: 347 tests pass.
- **Phase 3-4, 6 (future)**: instrument plugins, VST3, plugin param envelopes (sustain/release curves over time). See `docs/parameter-extension-todo.md` for the full plan.

### Editor Threading
- `HostedPluginHandle` is `!Send` (CLAP `PluginInstance` is `!Send`). It must live on the main thread.
- `HtrkApp.send_bus_handles: [Option<Box<dyn HostedPluginHandle>>; NUM_SEND_BUSES]` stores them.
- The audio thread only has the `HostedPluginProcessor` (which IS `Send`); the handle is never sent across threads.
- The `ClapPluginHandle.host_window: Option<PluginHostWindow>` (Windows only) holds the embedded-mode top-level HWND. `Drop` calls `DestroyWindow`.
- Embedded mode parent: `WindowMode::ChildOf(eframe_hwnd)` makes the host window a `WS_CHILD` of the eframe main window. The plugin is parented to it via `set_parent`.

### Editor Modes
- **Floating (default)**: plugin creates its own top-level window. No parent needed. Best for plugins that don't handle DPI.
- **Embedded (opt-in)**: host provides an HWND as the plugin's parent. Best for plugins that handle DPI. Use the "Edit (in htrk)" button to enter this mode.

### Rule for Future Changes
- When adding a new plugin format, create `vst3_plugin.rs` (or similar) implementing both `HostedPluginHandle` and `HostedPluginProcessor`.
- All `process()` implementations must be allocation-free. Allocate all buffers in `activate()`.
- Parameter changes from the UI thread go through the audio thread via an SPSC ring buffer; never set parameters directly on the audio-thread processor.
- The PluginSlot is the only state that needs to be persisted to `.htk`; everything else is rebuilt on load.
- Editor windows default to floating; embedded is opt-in via "Edit (in htrk)" button.
- Plugin log messages go through `tracing` (stderr default; configure via `RUST_LOG` or `AppConfig.log_file_path`).

## 21. CLAP Preset Discovery (PresetLibrary)

### Architecture
- `PresetLibrary` (`src/audio/plugins/preset_library.rs`) is an in-memory cache of `PresetEntry` structs, mirroring `PluginLibrary`/`SampleLibrary` patterns. Supports `search()` with pagination, `list_plugin_presets()`, and JSON persistence (`save_to_file`/`load_from_file`).

- `preset_discovery::scan_plugin_presets(path)` in `src/audio/plugins/preset_discovery.rs` loads the .clap library, gets a `PresetDiscoveryFactory`, iterates providers, and calls `Provider::get_metadata()` for each declared location. Uses the clack host-side `IndexerImpl`/`MetadataReceiverImpl` traits.

### Wiring
- `HtrkApp.preset_library: PresetLibrary` — direct app cache. `rescan_presets()` collects plugin paths from `plugin_library`, runs the scan, populates both the app and MCP libraries.
- `ToolContext.preset_library`, `McpServer.preset_library` — shared via `Arc<RwLock<>>`, same pattern as plugin/sample libraries.
- MCP tools: `preset.scan` (runs on MCP thread, writes via RwLock), `preset.list`, `preset.info`, `preset.list_by_plugin`, `preset.status`.

### Rule for Future Changes
- When adding new MCP preset tools, add tool defs in `list_tools()` in `tools.rs` and handlers in `call_tool()`. Read-only tools go in `src/mcp/preset_tools.rs`; mutations go in `MUTATION_TOOLS` + `mutations.rs`.
- `preset.scan` is explicitly NOT in `MUTATION_TOOLS` — it writes to `preset_library` via RwLock from the MCP thread, matching the `plugin.scan` pattern.
- The `PresetDiscoveryFactory` is only available from `PluginEntry::get_factory()` — plugins that don't implement it are silently skipped during scan.

## 22. Sub-Column Navigation and Cursor Indicator

### Keyboard Navigation
- `ArrowLeft`/`ArrowRight`: cycle sub-columns (Note → InstrTens → InstrOnes → VolTens → VolOnes → EffectType → EffectParamH → EffectParamL).
- `Alt+ArrowLeft`/`Alt+ArrowRight`: move between channels.
- `Tab`/`Shift+Tab`: move between channels.
- Sub-column navigation is defined in `SubColumn::next()` / `prev()` / `next_visible()` / `prev_visible()` in `src/ui/pattern_grid.rs:150-217`. Handlers are in `src/app.rs:995-1007` (`step_sub_column_forward`/`step_sub_column_backward`).

### Cursor Indicator
- The active sub-column is shown by a 2.5px-high bright bar (`theme.cursor_outline`) at the bottom of the active sub-column within the cell, computed by `sub_column_rect()` in `pattern_grid.rs`.
- This is necessary because the full-cell cursor highlight (`cursor_fill`) covers the entire channel width, making it impossible to distinguish between VolumeTens vs VolumeOnes or other adjacent sub-columns without the indicator.

## 23. Sample Library Heuristics + Persistence (Phase 2)

Extends the Phase 1 sample library (`§19`) with richer filename heuristics, more `DirFilter` fields, and on-disk persistence of the in-memory cache.

### Heuristic parser extensions (`src/mcp/library.rs`)

`parse_filename` now detects four new pieces of metadata per token. The detection order in the per-token loop is: **key → tempo_marking → bpm_range → bpm-suffix → standalone-bpm → note → tag → category fallback**.

- **Musical key** (`LibraryEntry.key`): pattern `^([a-g][#b]?)(maj|min|m7|maj7|min7|m7b5|dim|aug|sus2|sus4|add9|add11)$` (case-insensitive). The first letter is uppercased; the modifier and canonical quality are preserved verbatim. Examples: `Cmaj` → `"Cmaj"`, `A#min` → `"A#min"`, `Dmaj7` → `"Dmaj7"`.
- **Production tags** (`LibraryEntry.tags: Vec<String>`): tokens matching a fixed list of common sample-library tags, lowercased. Includes drum names (`kick`, `snare`, `hat`, `clap`, `crash`, `ride`, `tom`, `perc`), instrument types (`bass`, `sub`, `lead`, `pad`, `pluck`, `stab`), source types (`vox`, `vocal`, `fx`, `riser`, `impact`, `sweep`, `glitch`, `noise`, `drone`, `loop`, `oneshot`), and timbre/process descriptors (`wet`, `dry`, `lofi`, `mono`, `stereo`, `soft`, `hard`, `warm`, `bright`, `dark`, `punchy`).
- **BPM range** (`LibraryEntry.bpm_range: Option<(u32, u32)>`): token shape `(\d+)[-_](\d+)b?pm?` (or `(\d+)bpm[-_](\d+)bpm?`). The two values are stored with `low <= high` regardless of source order. Examples: `120-130bpm` → `Some((120, 130))`, `130-120` → `Some((120, 130))`.
- **Tempo marking** (`LibraryEntry.tempo_marking: Option<String>`): token matches one of `["Largo", "Adagio", "Andante", "Moderato", "Allegro", "Presto", "Vivace", "Lento", "Grave"]` case-insensitively. Always stored in canonical form (e.g. `"Largo"` not `"LARGO"`).

### Interaction with the existing category field

A token that matches the tag list is captured as a **tag** and is **not** added to the category. This means `Kick_C4.wav` now produces `tags=["kick"]` + `root_note="C4"` (and `category=None`) where it previously produced `category="Kick"`. This is a deliberate change: a tag is a single-token annotation, the category is the joined leftover string.

Tags are only persisted on the entry when the existing `found_musical` flag is set (i.e. note, bpm, or key is also detected). So `Kick.wav` alone yields no category and no tags, matching the existing "low confidence" rule.

### DirFilter extensions (`src/mcp/library.rs`)

`DirFilter` grew four new fields, all with the same `Option<T>` shape as the existing ones:

- `min_bpm` / `max_bpm: Option<f32>` — range check against `entry.bpm`. An entry with `bpm = None` simply isn't matched (no panic), matching how `min_duration` / `max_duration` behave.
- `key: Option<String>` — case-insensitive substring match against `entry.key`. An entry with `key = None` is rejected.
- `tag: Option<String>` — case-insensitive substring match against **any** element of `entry.tags`. An entry with no matching tag is rejected.
- `channels_filter: Option<u8>` — exact match against `entry.channels`. `1` = mono only, `2` = stereo only. An entry with `channels = None` is rejected.

`DirFilter::matches` is now `pub` so unit tests can exercise it directly. The MCP `sample_library.list_dir` tool definition and `cmd_sample_library_list_dir` handler in `src/mcp/tools.rs` pick up all four new fields, reachable from MCP clients as a `filter` sub-object.

### Persistence (`src/mcp/library.rs` + `src/app.rs`)

`SampleLibrary` now derives `Serialize`/`Deserialize`. `#[serde(default)]` is applied to every field so older or partial cache files still load. New methods:

- `to_json() -> Result<String, String>`
- `from_json(&str) -> Result<Self, String>`
- `save_to_file(&Path)` — atomic write via `.tmp` sibling + rename, matching the `PresetLibrary` pattern. Avoids a half-written cache file if the process is killed mid-write.
- `load_from_file(&Path) -> Result<Self, String>`
- `cache_len() -> usize` and `is_empty() -> bool` accessors for the app wiring.

Wiring in `src/app.rs`:

- New `sample_library_loaded: bool` field on `HtrkApp`, initialised to `false` in both constructors and set to `true` after the first-frame cache load.
- `ui()`: on the first frame, try `SampleLibrary::load_from_file(<config_dir>/sample_library_cache.json)` and merge it into the live library. The cache's `roots` field is **discarded** in favour of the `AppConfig`-managed `configured_roots` so changing the roots in Settings doesn't get clobbered by a stale cache.
- `on_exit()`: if the library has both non-empty `roots` and a non-empty `cache`, `save_to_file(<config_dir>/sample_library_cache.json)`. The non-empty-roots guard prevents an empty cache file being created on first launch when no roots are configured.

### Rule for Future Changes
- All `LibraryEntry` fields added in this phase carry `#[serde(default)]` — preserve that attribute on any future field so old cache files keep loading.
- When adding a new heuristic category (e.g. a new token type), update the detection order in `parse_filename` deliberately. Keys, tempo markings, and bpm ranges must be checked before the generic bpm/note checks, otherwise tokens like `120-130` would be partially consumed by the single-value bpm parser.
- The MCP `sample_library.list_dir` schema must stay in sync with `DirFilter`'s fields. When adding a new filter field, add the JSON-Schema property in `list_tools()` AND parse the field in `cmd_sample_library_list_dir`.
- `DirFilter::matches` returns `false` (not `None`) when an entry is missing the field being checked. A `None` field is treated as "cannot satisfy this filter", not as "vacuous match". This is the convention for every filter field that points at an optional `LibraryEntry` value.
- The atomic-write pattern in `save_to_file` is the only safe way to persist the cache — never write directly to the final path, always `.tmp` + `rename`.

## 24. Menu Bar Keyboard Navigation (Alt-tap / Alt+letter)

### Architecture
Standard Windows/macOS-style menu bar activation via the keyboard. Three behaviours:

1. **Alt tap** (press + release, no intervening key): toggles `menu_bar_active` on `HtrkApp`. When activating, `active_menu = 0` (File) is highlighted. A second Alt tap deactivates.
2. **Alt+letter** (Alt + F/E/V/A/H): opens the corresponding menu directly — sets `menu_bar_active = true`, `active_menu = idx`, `force_open_menu = Some(idx)`.
3. **Menu bar active navigation** (when `menu_bar_active && !popup_open`): Left/Right cycle menus, Down/Enter opens, plain F/E/V/A/H jumps, Escape deactivates.

### State Fields (`HtrkApp`)
- `menu_bar_active: bool` — menu bar is in keyboard-nav mode.
- `active_menu: usize` — index of highlighted menu (0=File, 1=Edit, 2=View, 3=Audio, 4=Help).
- `force_open_menu: Option<usize>` — one-shot: force-open this menu's popup this frame. Consumed (`.take()`) by `handle_menu_bar` before `draw_menu_bar`.
- `alt_prev_frame: bool` — previous frame's `modifiers.alt`, for press/release transition detection.
- `alt_intercepted: bool` — set true when any key was pressed while Alt held (so the release is NOT treated as a tap).

### Handler Flow (`handle_alt_menu` in `src/actions/keyboard.rs`)
Called FIRST in `handle_keyboard_input`, BEFORE `handle_early_text` and the focus gate, so Alt+letter works regardless of widget focus. Events consumed here are stripped from the queue.

### Popup Force-Open Mechanism
`draw_menu_bar` uses `top_menu_button` (not `dev_menu_button`) for the 5 top-level menus. When `force_open_menu == Some(idx)`:
1. The button is created via `ui.add(Button::new(text))`.
2. `Popup::open_id(ctx, response.id.with("popup"))` is called — this inserts the popup into `Memory::popups`.
3. `Popup::menu(&response).show(content)` runs — `show()` sees `set: None` (button not clicked), calls `keep_popup_open(id)`, then `is_open()` returns true. Popup renders.
4. On subsequent frames, `keep_popup_open` in `Popup::show` keeps the popup alive until the user closes it (Escape, click-away, or item selection).

The highlight is applied via `Button::fill(visuals.widgets.open.bg_fill)` when `menu_bar_active && active_menu == index`.

### Shortcut Remapping
Alt+F/E/V/A/H take priority for menu opening. The following pattern-editor shortcuts were remapped to avoid conflicts:
- **Alt+F** (Fill Instrument) → **Alt+G**
- **Alt+E** (Mark Block End) → **Alt+D**
- **Alt+V** (Paste) → removed (Alt+P already does Paste)
- All other Alt+letter shortcuts (Alt+M, Alt+S, Alt+N, Alt+L, Alt+C, Alt+P, Alt+X, Alt+B, Alt+Z, Alt+I, Alt+K, Alt+R, Alt+0-9) remain unchanged.

### Rule for Future Changes
- When adding a new top-level menu, update `NUM_MENUS` and `menu_index_for_key` in `keyboard.rs`, and add a `top_menu_button` call in `menu_bar.rs` with the correct index.
- `handle_alt_menu` must run before the focus gate and before `handle_early_text`.
- The `force_open_menu` field is a one-shot — always consume it via `.take()` in `handle_menu_bar`, never read it directly.
- Sub-menus inside top-level menus (Track, Column, Open Recent, etc.) still use `dev_menu_button` — only the 5 top-level menus use `top_menu_button`.
- When `menu_bar_active` is true and no popup is open, navigation keys (arrows, enter, escape, menu letters) are stripped from the event queue so they don't reach the pattern editor.

## 25. Instrument Editor Session (2026-07-06)

### Instrument List Hard Cap
- **File**: `src/ui/instrument_editor.rs:100`
- Changed `module.instruments.len().max(100).min(100)` → `module.instruments.len()`
- Shows all 256 max instrument slots; eliminates clippy warning about `.min(100)` being a no-op after `.max(100)`.

### Palette Scroll Reset
- **Files**: `src/ui/sample_palette.rs`, `src/ui/instrument_editor.rs`
- When switching instruments, the inline sample palette scroll resets to top so the first samples of the new instrument are immediately visible.
- Implementation: `draw_instrument_editor` tracks `prev_selected_instrument` in egui temp storage. When it changes, `draw_inline_sample_palette` receives `reset_scroll: true` and calls `ui.scroll_to_rect(top_left_pixel, Align::TOP)` on the first frame.

### Radio-Button Grouping
- **File**: `src/ui/instrument_editor.rs`
- Five radio-button clusters (filter type, NNA action, DCT, DNA, vibrato type) are now wrapped in `egui::Frame::group(...)` with `inner_margin(3, 1)`.
- Adds a visual border around each mutually-exclusive group, distinguishing them from adjacent controls.

### Plugin Parameter Scroll Stability
- **File**: `src/ui/sendfx_editor.rs`
- Added `id_salt("plugin_param_scroll")` to the `ScrollArea` inside `draw_plugin_parameter_sliders`.
- Without an explicit `id_salt`, changing the filter text changed the content height, which changed the auto-generated scroll-ID, which reset the scroll position.

### Envelope State Persistence
- **Files**: `src/ui/instrument_editor_panel.rs`, `src/ui/instrument_editor.rs`, `src/app_config.rs`, `src/app.rs`, `src/actions/file_io.rs`
- Moved `env_type` and `env_visible` from ephemeral `ui.data()` / `ui.data_mut()` egui temp storage to `InstrumentEditor` struct fields with `AppConfig` save/load.
- `generator_open` remains non-persistent (dialog state).
- `InstrumentEditor` now has `envelope_type: EnvelopeType` and `envelope_visible: bool` fields with `Default`.
- `AppConfig` has `instrument_envelope_type: Option<u8>` (0=Vol, 1=Pan, 2=Pitch, 3=Flt) and `instrument_envelope_visible: Option<bool>`.
- Saved in `save_app_config()` and loaded in both `HtrkApp` constructors.
- The `Show Envelopes` toggle button reads/writes `envelope_visible` on the struct directly (no intermediate `env_visible_id`).

