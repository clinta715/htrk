# Changelog

All notable changes to htrk will be documented in this file.

## [0.11.0] - 2026-05-22

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