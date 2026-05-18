# Changelog

All notable changes to htrk will be documented in this file.

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