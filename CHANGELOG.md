# Changelog

All notable changes to htrk will be documented in this file.

## [Unreleased]

### Added

- **MIDI file import (`.mid` / `.midi`).** Open a Standard MIDI File from the in-app file browser (now accepts `.mid`/`.midi`) and it imports into the *current* song rather than replacing it. Each MIDI track becomes one tracker channel (track 0 → ch 0, …, capped at 64); note timing is quantized to a `rows_per_beat` grid (default 4 = 16th notes, so `row = round(midi_tick / ticks_per_row)`); velocity maps to the volume column (0–64). The resulting patterns are appended to `module.patterns` and spliced into the order list immediately after the user's current edit position, so playback flows from the cursor into the imported material. The MIDI tempo meta-event is honored only when the current module is still at the default BPM. **Not undoable** (structural merge, like `module.load`). Decoding lives in `src/formats/midi.rs` (`midly` crate); the action is `crate::actions::import_midi`. Limitations (v1): only the first tempo is honored; pitch bend / CC / program changes are dropped; MIDI channel 10 (drums) is not specially mapped; sub-tick timing is quantized away.
- **MCP `midi.import` tool.** Same importer exposed over MCP with params `path` (required), `rows_per_beat` (optional, default 4), and `target_order` (optional, default current position). Registered in `MUTATION_TOOLS` and given a full `tool_def` schema in `list_tools()`.

### Changed

- **Split `src/mcp/mutations.rs` into a per-domain module directory.** The 2,102-line flat file (45 mutation handlers + helpers in one place) is now `src/mcp/mutations/` — a 14-file directory with one file per domain (`module`, `order`, `pattern`, `transform`, `instrument`, `sample`, `envelope`, `automation`, `playback`, `mixer`, `phrase`, `history`, plus `common` for shared helpers and `mod.rs` for dispatch). Each handler retains its uniform `fn(&mut HtrkCore, &serde_json::Value) -> CmdResult` signature and `pub(super)` visibility. The public entry point `crate::mcp::mutations::execute_mutation` is unchanged, so the sole caller (`app.rs`) needed no edits. **No behavior change** — pure code organization verified by the full 401-test suite.

- **De-duplicated the `Lcg` RNG.** A byte-identical numerical-recipes LCG existed in both `tools/phrase_generator.rs` and `mcp/mutations.rs` (with a comment admitting the copy was made "to avoid touching the pub surface"). `phrase_generator::Lcg` is now `pub(crate)` and shared by the `pattern.transform` mutation via `use`. The duplicate copy and its apologetic comment are gone.

## [0.26.0] - 2026-07-07

### Fixed

- **Clippy build blocker (3 deny-level errors)**: `src/formats/s3m.rs` had `channel < MAX_CHANNELS && channel < S3M_MAX_CHANNELS` in three places (note/volume/effect column parsing). Since `MAX_CHANNELS` (64) ≥ `S3M_MAX_CHANNELS` (32), the first clause was dead, and `clippy::redundant_comparisons` (deny-by-default) made `cargo clippy` fail outright. Removed the redundant clause; behavior unchanged (the tighter bound wins).

- **Unguarded buffer indexing in audio realtime loop**: `mix_voices_per_channel` (`src/audio/mixer.rs`) indexed `ch_mix`/`pre_ch_mix` with `base + offset + i` inside the per-sample loop with no in-loop bounds check — correctness depended entirely on the caller sizing the buffer to `num_channels * 2 * stride`. Today's callers are correct, but a future buffer-sizing change would have panicked on the audio thread. Added a `debug_assert!` (zero cost in release) documenting the invariant.

- **`.expect()` panics at CLAP FFI boundary**: `get_parameter` and `has_editor` (`src/audio/plugins/clap_plugin.rs`) called `.expect("instance is null")` on a raw-pointer-to-mut-ref cast. The preceding `as_ref()` check made it safe in practice, but a misbehaving plugin returning a null extension pointer would have panicked the host. Replaced with graceful early returns (`0.0` / `false`).

### Performance

- **MCP snapshot no longer serialized every frame**: when the MCP server is enabled, `draw_preamble` was rebuilding the full module JSON snapshot (every pattern row → `Vec<Vec<Cell>>`, every instrument/sample `serde_json::to_value`, plus the entire module serialized) on **every 60 fps frame**. Now gated behind an `Arc<Module>` pointer-identity dirty flag (`mcp_last_module_ptr`) — the snapshot is rebuilt only when an edit changes the `Arc` pointer (via `ensure_module_ownership`). The cheap playback snapshot (atomic reads) stays live every frame so MCP clients still see current transport position.

- **Per-cell String allocations eliminated in pattern grid**: `format_effect` returned `(String, String)`; the effect-type column (a single hex char) heap-allocated per cell per frame. Changed to `(&'static str, String)` — the type is now a `&str` literal, and dynamic legacy `Raw` codes use a `HEX_DIGITS` lookup table instead of `format!("{:X}", n)`. `draw_cell`'s constant note/instrument/volume sentinels (`"==="`, `"---"`, `".."`) now use `Cow::Borrowed(&'static str)` instead of `.to_string()`. Only dynamic `Note::On` / numeric values allocate.

- **`ui.input()` hoisted out of cell loop**: `draw_pattern_grid` called `ui.input(|i| i.pointer.hover_pos())` inside the row×channels loop — `visible_rows × visible_channels` times per frame, each re-locking egui input state. Now read once before the loop.

- **`channel_panning` clone avoided**: `draw_pattern_view` was cloning the `Vec<u8>` every frame to pass to `draw_channel_headers`; now borrows the slice directly from the module.

### Notes

- **Audio-path `Module.clone()` intentionally left as-is**: the initial audit flagged `m.clone()` in `evaluate_automation`, `cell.rs`, `advance.rs`, `period.rs`, and the effect processors as "deep Module clones every tick." Verified that `SequencerEngine.module` is `Option<Arc<Module>>`, so these are `Arc` refcount bumps (~1ns atomic ops), not deep copies. They exist to release the `&self` borrow so `&mut self` can be used afterward. Refactoring audio-thread code for a nanosecond-scale gain was deemed not worth the regression risk.

## [0.25.0] - 2026-07-06

### Changed

- Instrument list hard cap removed (`module.instruments.len().max(100).min(100)` → `.len()`); all 256 max instrument slots are now shown.
- Sample palette scroll resets to top when switching instruments (tracks `prev_selected_instrument` in egui temp storage).
- Five radio-button clusters in the instrument editor (filter type, NNA, DCT, DNA, vibrato type) wrapped in `egui::Frame::group(...)` for visual grouping.
- Plugin parameter `ScrollArea` given explicit `id_salt("plugin_param_scroll")` so changing filter text no longer resets scroll position.
- Envelope type/visibility moved from ephemeral egui temp storage to `InstrumentEditor` struct fields persisted via `AppConfig`.

## [0.24.0] - 2026-07-04

### Changed

- CLAP plugin GUI fixes.
- Procedural plugin parameter sliders.
- Embedded-fallback popup for plugin editors.
- File browser filter.
- Misc cleanup.

## [0.23.0] - 2026-06-26

### Added

- **Multi-instrument drum mode for `phrase.generate`**: drum mode now accepts per-drum instruments (`kick_instrument`, `snare_instrument`, `hat_instrument`) so kick, snare, and hat can each play a different instrument. Falls back to the shared `instrument` parameter when a per-drum override is omitted, so existing single-instrument drum calls keep working.

- **Per-channel drum density** (`kick_density`, `snare_density`, `hat_density`): each drum voice now has its own density (0.0-1.0) which drives the euclidean pulse count independently. Defaults preserve the prior feel (kick=¼, snare=¼, hat=½) when unspecified.

- **Drum swing**: new `swing` parameter (0.0-0.5) shifts hat hits on off-beats to produce a swung feel. 0.0 = straight, 0.5 = maximum swing.

- **`pattern.transform` MCP tool**: apply a list of transforms to a pattern as a single undoable operation. 13 ops supported: `thin` (remove notes by probability), `transpose` (shift notes by semitones), `rotate` (shift rows), `reverse`, `invert` (mirror pitches around a root), `quantize` (snap to a scale), `echo` (delayed duplicate with velocity decay), `humanize` (random velocity perturbation), `set_instrument` (bulk-set instrument on cells with notes), `shift` (slide notes in time), `swing` and `augment`/`diminish` (placeholders — see Limitations). Returns the list of applied ops, the cell-change count, and an `undo_id`.

- **`undo.last` / `redo.last` / `undo.to` MCP tools**: every mutation now returns a numeric `undo_id`. `undo.to {undo_id}` reverts every command up to and including the given id (useful for "undo that one specific thing" without needing to call undo N times). `undo.last` and `redo.last` round-trip the id and label of what was undone/redone.

- **Per-channel placement counts** in `phrase.generate` response: `notes_placed: {0: 16, 1: 16, 2: 32}` shows exactly how many cells were set on each channel. Helps agents verify the result without re-querying.

- **MCP `phrase.generate` silent-fail fix**: when the requested `order` is beyond the current order list, the command now auto-extends the order list with the highest existing pattern as the fallback (was: silently failed because `BulkSetCellsCommand::execute` returned an error that was discarded). Propagates `EditError` instead of ignoring it.

- **`HtrkCore::with_module_mut<F, R>()` helper**: borrow-conflict-free mutator pattern using `std::mem::swap` (mirrors `with_processor_mut`). 16 simple call sites converted; ~20 complex ones still use the original 4-step pattern.

- **`HtrkApp::any_dialog_open()` method** + `handle_keyboard_input` decomposed into 5 focused sub-handlers + event cleanup moved from `draw_preamble` into the keyboard handler.

### Fixed

- **`cell.set instrument: 0` dropped to `None`**: the MCP `cell.set` handler treated `instrument: 0` as "clear" because `Cell::instrument` is `Option<u8>`. Now `0` is treated as a valid instrument (Some(0)) and only a negative value clears. Verified: kick on `instrument: 17` now triggers voice allocation (was: 0 voices, no sound).

- **Application exit hang (was: looked like a crash)**: the MCP HTTP server's main loop used a **blocking** `listener.accept()`, so its worker thread was stuck in `accept()` even after `shutdown.store(true)`. `McpServer::stop()` called `handle.join()` on this stuck thread, blocking `on_exit` indefinitely — the process appeared not to close. Made the listener non-blocking with a 50 ms WouldBlock retry; clean exit now completes in ~1 second.

- **`audio_debug` feature flag didn't compile**: `debug_log!` macro uses `$crate::debug_log` so the import was missing in three files where the macro was used inside a `#[cfg(feature = "audio_debug")]` block. Added `use crate::debug_log;` to `src/app.rs`, `src/audio/engine.rs`, `src/audio/mixer.rs`.

- **Panic swallowing on eframe shutdown**: a panic during `on_exit` (e.g. plugin host destruction, audio engine teardown) used to be silently dropped by eframe, making the process look like a no-op crash. Installed a global panic hook that writes timestamp + location + backtrace to `<config_dir>/crash.log` for post-mortem diagnosis.

- **`on_exit` invisibility**: replaced ad-hoc ordering with explicit `eprintln!` tracing through the whole shutdown sequence (`stopping MCP → audio Stop → drop stream → save config`) so the next hang is debuggable from stderr alone. The 50 ms sleep before stream drop lets the audio thread observe the Stop command and finish any in-flight callback.

### Limitations

- `pattern.transform` `swing` and `humanize` (timing) are placeholders — note-timing data is not stored in the `Cell` struct (it lives in the channel effect column, separate from cell mutation). The velocity-randomization path in `humanize` does work; the per-row shift does not.

- `pattern.transform` `augment`/`diminish` are placeholders — they require resizing the pattern (changing `num_rows`), which is a separate operation not yet implemented for the transform tool.

### Changed

- **Version**: 0.22.0 → 0.23.0
- **`UndoManager::execute()` return type**: `Result<(), EditError>` → `Result<u64, EditError>`. The returned `u64` is the assigned `undo_id`. All 40 existing `let _ = ...execute(...)` call sites remain unchanged because the success value was already being discarded.
- **`UndoManager` entries** now carry an explicit `id: u64` field. Previously `undo_to` parsed the id out of the entry's label string, which failed for non-numeric labels like "phrase.generate drum".

## [0.19.0] - 2026-06-25

### Added

- **CLAP instrument plugins (Phase 3)**: Instruments can now be backed by a CLAP plugin instead of a sample chain. New `plugin` and `midi_base_channel` fields on `Instrument`. Plugin slot loaded from the Instrument tab's "CLAP Instrument" strip (moved to the top of the editor for discoverability). Sequencer routes note-on/off events to instrument plugin processors via `PluginNoteEvent` queue, same tick-based dispatch used by send-bus plugins. Keyboard preview sends note-on/off with held-key release detection. `send_note_on/off` on `HostedPluginProcessor` trait. Instrument plugin browser dialog for selecting and loading plugins.

- **Instrument plugin state persistence + auto-reload**: `load_and_install` restores saved state; `unload` saves state before dropping; `save_all_instrument_plugin_states()` captures patches on project save; `sync_instrument_plugin_state()` re-loads plugins from slots on `.htk` load. Send-bus plugins got the same treatment (`save_all_send_bus_plugin_states`, `sync_send_bus_plugin_state`). Round-trip test confirms state save/load works on real CLAP plugins.

- **Instrument plugin editor window**: Open/close CLAP instrument plugin editor (floating or embedded) from the Instrument tab. Embedded editors are rendered in a global dock at the bottom of any tab. X-close detection polls `IsWindowVisible` every frame regardless of active tab.

- **Parameter automation for instrument plugins**: New `AutomationTarget::InstrumentPluginParam` variant. Sequencer queues values in `pending_instrument_plugin_param_changes`; audio engine routes to the matching processor's param ring. UI shows automatable params in the Automation tab's "+Add Track" sidebar for both send-bus and instrument plugins.

- **Instrument parameter macros**: `ParameterMacro { source, param_id, range_min, range_max }` on `Instrument`. Currently supports `MacroSource::Volume` — the cell's volume column (0-64) is normalized to 0.0-1.0 and linearly remapped to the plugin param's range. Queued through the same channel as automation values.

- **Sample Library browser dialog**: In-app GUI dialog (Sample tab → "Library" button) for the MCP-driven SampleLibrary. Browse/search library roots, view WAV metadata (duration, sample rate, bit depth, root note, category), import samples into the current slot. Keyboard focus gate updated to block pattern editing while the dialog is open.

- **Round-trip CLAP state test**: `test_state_save_load_round_trip` in `clap_plugin.rs` — loads TAL-Reverb-4, sets a parameter, saves state, drops, reloads, loads state, asserts the value matches.

### Changed

- **Version**: 0.18.0 → 0.19.0
- **Refactored `Instrument::envelope()`/`envelope_mut()`**: Deletes 150 lines of 4-arm `EnvelopeType` → field dispatch boilerplate (23 call sites replaced).
- **Refactored plugin load/save helpers**: `load_and_activate_clap_plugin`, `save_all_plugin_states`, `write_plugin_state_to_slot`, `sync_plugin_slots_from_module` — unifies the parallel send-bus and instrument plumbing.
- **Sync functions abstraction**: `sync_instrument_plugin_state` and `sync_send_bus_plugin_state` share the same iteration/lookup/load pattern via `sync_plugin_slots_from_module`.
- **Inline function split**: `handle_playback_tab`, `handle_mixer_tab`, `handle_sample_tab` extracted from `HtrkApp::ui()` (UI Shrink Phase 2).
- **Sequencer engine module split**: Monolithic `sequencer_engine.rs` split into directory (`mod.rs`, `cell.rs`, `period.rs`, `helpers.rs`, `advance.rs`).

### Fixed

- **HTTP MCP transport**: Non-blocking event loop used `BufReader::new(stream.try_clone())` — the BufReader buffered the entire HTTP request into its internal buffer during first-line detection, then dropped the buffer. Replaced with blocking accept + per-thread HTTP handler.
- **Context menu effect commands**: `SetEffect`/`ClearEffect` silently did nothing when no selection was active. Now falls back to the cursor cell as a single-cell "selection".
- **Context menu missing items**: Added Copy, Paste, Cut, Select All, Transpose +1/-1 to the pattern grid's right-click menu.
- **Sub-column click detection**: `InstrumentOnes`/`VolumeOnes` unreachable (first threshold exceeded column width). Fixed to midpoint split (`inst_width * 0.5`).
- **Keyboard preview channel routing**: Plugin instrument preview now uses `midi_base_channel + selected_channel` matching the pattern playback routing.
- **Compiler warnings**: Deprecated `egui::Rounding` → `CornerRadius`, `Frame::none()` → `Frame::NONE`, unused imports across `sequencer_engine/` submodules.
- **`save_state` trait signature**: Changed `&self` to `&mut self` to satisfy CLAP's `plugin_handle()` requirement (new Rust 1.95 `invalid_reference_casting` lint). Added missing `MainThreadHandler` impl for `HtrkMainThread`.

## [0.18.0] - 2026-06-24

### Added

- **Conventional Mixer (F11)**: New Mixer view with one channel strip per channel plus master and per-bus send strips. Each channel strip has Mute/Solo, volume slider (0-64), pan slider (0-255), and 4 send-level sliders. Send-bus strips show the loaded CLAP plugin's name and a return-level slider. 'Show all channels' toggle (off = only channels with non-empty cells in the current pattern). 'Reset' button to restore vol=64, pan=128, sends=0.
- **Rich CLI help**: `htrk --help` lists all flags and the most useful key shortcuts. New commands: `--list-effects` (per-effect one-liner for 0-F + P/Z/S/R/X + volume column), `--mcp-help` (JSON-RPC transport, handshake, tool categories, examples), `--config-path` (prints the user's config.toml location), `--reset-config` (deletes config.toml). New runtime flags: `--no-mcp`, `--log-file <PATH>`, `--theme <NAME>`, `--headless` (reserved).
- **Status-bar sub-column breadcrumb**: The status bar now shows the current sub-column (Col:Note/Inst/Vol/Fx) with a hover tooltip that lists what characters the column accepts. Helps users find the volume column without reading the manual.
- **Per-column tooltips in the pattern grid**: A column header strip above the data rows shows the column name in each sub-column. Hovering any header shows a description of the column, how to edit it, and which Ctrl+N toggle hides it. When a column is hidden, the header tooltip changes to a 'hidden, toggle with Ctrl+N' message.
- **Effect hover popups**: The FX column hover popup now shows the hex code, the standard name, the param range, and the current values. E.g. `4  Vibrato / X = speed (0-F), Y = depth (0-F) / current: 4 8`.
- **Expanded help screen (F1)**: New collapsible sections for Sub-column navigation, Volume column FAQ, Effect commands (with a per-effect reference table), Send FX, Mixer, Automation, MCP scripting server, and Command-line flags. The window is now resizable (was fixed) and starts at 820x640.
- **Volume column tests**: 3 new tests in `actions::keyboard::tests` covering the volume column keyboard path (tens/ones digit entry, note preservation) — the underlying code is correct, and the new tooltips/breadcrumb/header make it discoverable.

### Changed

- **Version**: 0.17.0 → 0.18.0

## [0.17.0] - 2026-06-23

### Added

- **Design Token System**: Complete `TrackerTheme` color token set with 80+ semantic tokens replacing all hardcoded UI colors. `STYLE.md` documents all tokens, color contributions, and naming conventions.
- **MCP Server (Phases 1-2)**: JSON-RPC 2.0 over TCP (port 18763) with read-only snapshots (module/playback/channels), mutation dispatch via main-thread command queue, and 20+ tools for querying and editing module state.
- **Sample Editor**: Selection model (cut/copy/paste/crop/silence), zoom with scroll support, loop marker manipulation, Fade In/Out processing, waveform context menu with Normalize/Reverse/TrimSilence. All operations undoable.
- **Slice-to-Instrument**: Slice samples to create multi-sampled instruments, with per-slice mapping to note ranges.
- **Envelope Generator**: Automation curve generator for envelope points with configurable shapes.
- **CLAP Plugin Editor Windows (Phase 5, Windows only)**: "Edit..." button in the Send FX panel opens the plugin's GUI as a top-level OS window. Supports both floating mode (plugin manages its own window) and embedded mode (host provides a Win32 HWND via `windows-sys 0.59`). Tested with TAL Reverb 4. macOS/Linux deferred to a future release.
- **CLAP Host Extensions (Phase 5.1)**: Replaced the `()` shared handler with `HtrkHostShared` implementing `HostLogImpl` and `HostGuiImpl`. Plugin log messages now route through `tracing` (stderr by default, optional file via `AppConfig.log_file_path`). GUI callbacks (`resize_hints_changed`, `request_resize`, `request_show`, `request_hide`, `closed`) are wired.
- **CLAP Editor: Embedded Mode**: New "Edit (in htrk)" button parents the plugin's GUI as a `WS_CHILD` of the eframe main window. Recommended only for plugins that handle DPI; floating is the default.
- **CLAP Editor: Error Display**: `open_editor` failures now show a red label below the button instead of `eprintln!`.
- **CLAP Editor: X-Close Detection**: Polling `IsWindowVisible` on the plugin's container HWND each frame; if the user closes the floating window externally, the Send FX UI updates immediately.
- **F6 = Send FX view, F7 = Automation view**: Keyboard shortcuts (F2-F5 unchanged).
- **CLAP plugin parameters**: `HostedPluginHandle::parameter_info()` now uses the clack `Param` extension to enumerate exposed parameters (id, name, min/max, is_automatable, is_modulatable). A new "Parameters" collapsible section in the Send FX view shows one Slider per parameter, with the param's actual range. Slider changes call `set_parameter()` which pushes to a lock-free SPSC ring shared with the audio thread; the plugin sees the new value on the next `process()` call.
- **CLAP plugin parameter automation**: New `AutomationTarget::PluginParam { send_bus, host_index, param_id }` variant. The sequencer queues plugin-param automation values; the audio engine routes them to the right plugin's param ring after each `process_tick()`. Combine with the Send FX sliders for full per-param automation.
- **Effects column fixes**: hex typing in the FX column always writes the cell's `Effect`, regardless of whether an automation lane is overlaid. Clicking in the FX column with an automation overlay now creates an automation point by default; `Ctrl+click` bypasses automation to enter an effect value. The keyboard hex-digits no longer auto-route to `enter_automation_hex` (which was a UX trap).
- **Right-click context menu**: New "Effect" section in the pattern editor's right-click menu with a "Set Effect" submenu (all 16 hex effects 0-F with names: Arpeggio, Portamento, Vibrato, SetVolume, SetSpeed, etc.) and a "Set Param (P/Z/S/R/X)" submenu (the named CLAP-style effect commands). "Clear Effect" sets the effect to `None`. All actions work on the cursor cell or the full selection.

### Changed

- **Version**: 0.16.0 → 0.17.0
- **Help screen rewritten**: Full reference for all keyboard shortcuts, effect commands, and navigation, organized into collapsible sections with theme-aware colors.
- **Playback tab layout**: Pattern grid now always rendered (fallback to editing pattern when stopped), preventing channel blocks from jumping on playback start/stop.
- **Save-on-exit confirmation**: Intercepts close requests and Ctrl+Q when module is dirty with Save/Don't Save/Cancel dialog.
- **File browser columns**: Fixed-width truncation cells prevent overlapping text. Column resize drag handles with persisted widths.
- **Window size persistence**: Viewport dimensions saved to `AppConfig` every frame and restored on launch.
- **Instrument editor persistence**: `list_width` and `envelope_height` panel sizes saved to/from `AppConfig`.
- **Phrase Generator**: Chord mode with configurable chord types, progressions, and per-channel placement.

### Fixed

- **Exit deadlock**: Audio stream dropped in `on_exit` before egui teardown, preventing freeze on close.
- **Sample preview in edit mode**: File browser preview notes play correctly even when edit mode is enabled.
- **Deadlock in module mutation**: Removed bare `.unwrap()` calls on `self.module`/`engine.module` with safe match guards throughout sequencer engine, XM loader, and legacy format paths.
- **Channel header contrast**: Mute/solo/auto fills use `rgba_premultiplied` instead of `gamma_multiply` for correct semi-transparency across all themes.
- **Various pattern/layout/playback regressions**: Tab navigation in dialogs, playback row highlighting in Playback tab, order list drop-target highlight.

### Performance

- **Mixing buffer flattening**: Pre-allocated `Vec<f32>` mixing buffers replaced with flat `[f32; 2]` per voice, eliminating per-sample allocations in the hot path. `WavRenderer::render_full_song` reuses buffers across chunk iterations.
- **Precomputed `sample_length_bg` cache**: Background length indicator computed once per cell outside the inner loop in pattern grid rendering.
- **Real-time thread allocation elimination**: Muted/solo/effective-mute masks and per-channel peak caches pre-allocated on `AudioEngine` init instead of per-callback allocations.
- **`u16` widening**: `current_row`, `current_pattern`, `pattern_break_row`, `position_jump_order`, `pattern_loop_start` widened from `u8` to `u16` for patterns >255 rows.

## [0.14.1] - 2026-06-16

### Added

- **Confirm-on-exit preference**: New checkbox in Settings > Editor > General ("Confirm before exiting with unsaved changes") persists between sessions via `confirm_on_exit` config field. When unchecked, close and Ctrl+Q exit immediately without prompting.

### Fixed

- **Playback tab layout shift**: Pattern grid now always renders (shows the selected editing pattern when stopped, the playing pattern during playback), preventing channel blocks and info footer from jumping when playback starts or stops.

### Added

- **Save-on-exit confirmation**: When closing (OS close button, Alt+F4, or File > Quit Ctrl+Q) with unsaved changes, a Save / Don't Save / Cancel dialog appears. Save triggers `save_current_file`; Don't Save exits immediately; Cancel dismisses. File > Quit Ctrl+Q added to menu and keyboard handler.
- **Window size persistence**: Window dimensions saved to `AppConfig` every frame via `ctx.viewport_rect()`, restored on next launch.
- **Instrument editor persistence**: `list_width` and `envelope_height` panel sizes saved to/loaded from `AppConfig`.
- **File browser column widths**: Drag handles between Details header columns (Name, Duration, Type, Size, Modified) allow resizing. Widths persisted in `AppConfig`.

### Fixed

- **Sample Map +/- visibility**: Moved "Fill All" out of `right_to_left` sub-layout so +/-/Browse/Fill All flow left-to-right without wrapping behind the right edge.
- **Double-borrow in sample editor**: `sample_editor.rs` now takes `&mut SampleEditor` only (removed redundant individual params).

### Added

- **Pattern data display in Playback tab**: The Playback tab now shows the currently playing pattern as a read-only grid with playback-row highlighting and auto-scroll to follow the playhead. Scroll state is independent from the Pattern tab.
- **Oscilloscope and Spectrum labels**: Added "Oscilloscope" and "Channel Spectrum" labels to clarify the real-time monitoring widgets.

### Changed

- **Version**: 0.10.0 → 0.11.0
- **Version strings are now dynamic**: About dialog, status bar, and help screen now use `env!("CARGO_PKG_VERSION")` instead of hardcoded `v0.6.0`.
- **Oscilloscope cells scale dynamically**: Cell dimensions adapt to channel count — ≤4 channels get one row of wide cells, ≥5 channels use up to 8 compact columns. `CELL_HEIGHT` reduced from 45 to 32 for the fixed-size path.
- **VU meter blocks compacted**: Block width reduced from ~110px to ~70px, bar width from 100 to 55, fonts from 11/12 to 9/10.
- **Monitoring section scrollable**: VU meters, oscilloscope, and spectrum are now wrapped in a `ScrollArea`. Pattern grid takes ~55% of available height; monitoring section uses the remainder with internal scroll if needed.

### Fixed

- **Spectrum energy always zero**: Read channel scope returns `f32` samples, but the energy calculation was dividing by `u32::MAX` (giving ~0). Fixed to use `s.abs()` directly.

## [0.10.0] - 2026-05-19

### Added

#### Automation Curves
- **Data model**: `AutomationTrack`, `AutomationPoint`, `AutomationTarget`, `InterpolationMode` types in `src/sequencer/automation.rs`. Each track has a target (per-channel or global), optional channel assignment, ordered points with (order, row, value, interpolation), and enable/disable toggle.
- **Interpolation engine**: `evaluate()` method on `AutomationTrack` with 4 modes — Hold (step), Linear, Smooth (cosine ease), Exponential. Song-level addressing using (order, row) pairs. 14 unit tests covering all modes and edge cases.
- **Sequencer integration**: `evaluate_automation()` called at top of every tick in `process_tick()`. Per-channel auto-factor fields (`auto_volume_factor`, `auto_panning_offset`, `auto_filter_cutoff_factor`, `auto_filter_resonance_factor`, `auto_send_a_factor`, `auto_send_b_factor`) on `ChannelState`. Global fields (`auto_global_volume_factor`, `auto_tempo_factor`, `auto_speed_factor`) on `SequencerState`. Applied as multipliers/offsets to channel and global parameters.
- **HTK persistence**: Module version bump 4→5. `automation_tracks` and `next_automation_id` fields added with `#[serde(default)]` for backward compatibility. Two integration tests (round-trip and backward compat).
- **Per-channel automation UI**: Channel header cycle-button selects automation target per channel (left-click forward, right-click backward through Vol→Pan→FltCut→FltRes→SendA→SendB→...). Effect column renders automation overlay when target is set — shows curve points with hex values and interpolation indicators.
- **Mouse interaction in overlay**: Click to create points, Shift+drag for freehand drawing. Points stored in the matching `AutomationTrack`.
- **Hex keyboard entry**: When cursor is in automation overlay column, hex digits (0-9, A-F) set point values. Delete key removes points at cursor position.
- **Global Automation tab**: New `AppView::Automation` tab alongside Pattern/Sample/Instrument/SendFx/Playback. Track list sidebar shows all automation tracks with enable/disable toggle, select, and delete. Lane editor renders curve with all 4 interpolation modes (Hold=step, Linear=straight line, Smooth=cosine ease, Exponential). Points displayed as dots with hex value labels.
- **Lane editor click-to-create**: Clicking in the lane editor creates automation points at the computed row/value. Drag to move existing points. Right-click to delete points.
- **Channel picker**: Per-channel track creation from the Automation tab uses a `DragValue` spinner for channel selection (0-indexed). Per-channel targets now correctly receive `Some(channel)` instead of `None`.
- **Interpolation mode UI**: Selectable buttons in lane editor change `default_interp` on the selected track. Ctrl+5/6/7/8 keybindings switch default interpolation mode (Hold/Linear/Smooth/Exponential) when Automation tab is active.
- **Song-level order tracking**: `AutomationEditorState.selected_order` synced from `HtrkApp.selected_order` so lane editor creates points at the correct song position.
- **`remap_automation_orders()`**: Utility function to renumber automation point orders when order list or patterns change.

#### Format Loaders
- **669 (Composer 669)**: Full loader with pattern, instrument, and effect parsing. 8-bit samples with loop support. Effects mapped: portamento, volume slide, vibrato, tempo.
- **MMD (Medley Module Description)**: Loader for MMD0/MMD1/MMD2/MMD3 variants used by OctaMED. Multi-mode patterns, block/song structures, instrument and sample parsing with precise and approximate BPM calculations.
- **STM (ScreamTracker 2)**: Full loader with pattern, sample, and effect mapping. Supports both v1 and v2 header formats.
- **ULT (UltraTracker)**: Full loader with multi-pattern tracks, sample parsing, and extended effect mapping including retrigger and multi-note support.

### Changed
- **Version**: 0.9.0 → 0.10.0
- **HTK format version**: 4 → 5 (backward compatible)

## [0.9.0] - 2026-05-17

### Added

#### File Browser
- **Selection position persistence**: File browser now remembers scroll position and selected index per directory, persisted across sessions in `last_selections` config field (key format: `"mode:/full/path"` → `(selected_index, page)`).
- **List/Details view modes**: Toggle between compact list view and detailed columnar view via toolbar button (☰/≡). Details view shows columns: Name, Duration, Type, Size, Modified. Persisted in `file_browser_view_mode` config.
- **Sort options**: Sort files by Name, Date, Size, or Type with ascending/descending toggle (↑/↓). Sort preference persisted in `file_browser_sort_by` and `file_browser_sort_desc` config fields.
- **Extended audio format duration support**: Duration now shown for wav, mp3, ogg, flac, it, xm, s3m, mod, and 669 files (previously only wav).
- **Modified date column**: Details view shows file modification date (xx/xx/xx format).

#### Sample Export
- **Individual sample export to WAV**: Right-click context menu on sample list (index > 0) with "Export Sample..." option.
- **Bit depth selection**: Combined file dialog with bit depth options (8-bit unsigned, 16-bit, 24-bit, 32-bit float). Choice persisted in `sample_export_bit_depth` config.
- **Path persistence**: Exports default to last used `default_wav_path` directory, which updates after each successful export.
- **Sample name sanitization**: Invalid filename characters replaced with `_` when suggesting default filename.

#### Pattern Grid
- **Spacing modes**: New configurable spacing options affecting both row height and column width:
  - **Compact**: Minimal spacing (font_size × 1.3 row height, no column gaps)
  - **Normal**: Default spacing (font_size × 1.6 row height, 0.3 char_width gaps)
  - **Wide**: Extra spacing (font_size × 1.8 row height, 0.6 char_width gaps)
  - **Extra Wide**: Maximum spacing (font_size × 2.1 row height, 1.0 char_width gaps)
- **Keyboard shortcut**: `Ctrl+Shift+Space` cycles through spacing modes.
- **Settings UI**: 4-button selector in Settings > Editor tab.
- **Persistence**: `spacing_mode` stored in config (compact/normal/wide/extra_wide).
- **Independent of zoom**: Spacing mode operates independently from zoom_factor.

#### Instrument Export
- **Export .hti instruments via context menu**: Right-click on instrument list (index > 0) shows "Export..." and "Import..." options.
- **Path persistence**: Both export and import dialogs open in `default_instrument_path` directory, which updates after each operation.
- **Export specific instrument**: `ExportInstrument(usize)` event carries instrument index, separate from main "Save..." button which saves selected instrument.

### Changed
- **Version**: 0.8.0 → 0.9.0

## [0.8.0] - 2026-05-17

### Added

#### Sequencer Engine
- **Extra-fine portamento (S3M)**: `Effect::ExtraFinePortamentoUp/Down` variants with `(speed + 2) >> 2` scaling (1/4 speed). S3M loader now maps 0xF0-FF → ExtraFine, 0xE0-EF → Fine. Includes roundtrip test.
- **Global volume slide (XM)**: `global_volume_slide` flag in `ActiveEffects`. Per-tick handler applies slide each tick (XM: 0-64 range, non-XM: 0-128 range). Memory preserved across ticks.
- **FunkIt effect**: `FUNK_TRACK` constant, `funk_toggle` in `ChannelState`, per-tick handler modulates voice position when `funk_speed > 0`. Triggers at intervals of `FUNK_TRACK[speed]` ticks.
- **Karplus-Strong synthesis**: KS fields (`karplus_strong`, `ks_delay_line`, `ks_pos`, `ks_feedback`) on `Voice`. Initialization in `trigger_channel_note` and `trigger_channel_note_period` (buffer length = sample_rate / freq, filled with noise, decay = 1.0 - param/16 * 0.5). Synthesis branch in both `mix_voices` and `mix_voices_per_channel`.
- **9 new unit tests**: `global_volume_slide_xm_applies_each_tick`, `global_volume_slide_non_xm_applies_each_tick`, `global_volume_slide_memory_accumulates_per_tick`, `extra_fine_portamento_slows_by_factor_4`, `funkit_modulates_voice_position_on_tick`, `funkit_speed_zero_disables_modulation`, `karplus_strong_initializes_buffer_on_trigger`, `karplus_strong_disabled_when_param_zero`, `karplus_strong_mixer_produces_output`.

#### UI
- **Playback tab**: New `AppView::Playback` with channel status grid showing Note, Instrument, Peak Meter for each channel. Includes transport controls, oscilloscope, and channel spectrum energy bars.
- **Column visibility**: `ColumnVisibility` struct with `note/instrument/volume/effect` booleans. Toggle buttons in pattern toolbar (N/I/V/E). View > Columns submenu with checkboxes. `col_vis: ColumnVisibility` field on `HtrkApp`. `GridMetrics::new` now accepts `col_vis` parameter and computes dynamic `channel_width`.

#### Audio Engine
- **Channel note/instrument display**: `channel_note` and `channel_instrument` arrays in `AtomicPlaybackState` (64 channels each). Methods: `set_channel_note_instr(ch, note, instr)`, `channel_note_str(ch)`, `channel_instrument_str(ch)`. Synced in `update_playback_state` each callback.

### Fixed
- **S3M extra-fine portamento mapping**: 0xF0-FF range now correctly maps to `ExtraFinePortamento` (was incorrectly mapped to `FinePortamento`). 0xE0-EF range maps to `FinePortamento`.

### Changed
- **Version**: 0.7.0 → 0.8.0

## [0.7.0] - 2024-XX-XX

### Added
- Send effects system with dynamic channel count and per-channel send levels
- WAV export with loop-guarded sample processing