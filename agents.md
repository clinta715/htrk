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
- Built per egui frame in `draw_preamble()` after playback state is read.
- `ModuleSnapshot` stores module-level JSON plus pattern/instrument/sample entries (skipping sample PCM data).
- `PlaybackSnapshot` and `ChannelsSnapshot` store lightweight playback state.
- Read by MCP thread under `RwLock::read()` — never blocks main thread.

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
