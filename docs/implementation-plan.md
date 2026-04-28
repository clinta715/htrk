# Implementation Plan

## Overview

Implementation is divided into 8 phases. Each phase produces a testable milestone.
Dependencies between phases are noted.

## Phase 1: Project Scaffold & Data Model

**Goal**: Compiling Rust project with all core data types defined.

**Estimated effort**: 2-3 days

### Tasks

- [x] **1.1** Initialize Cargo project
  - Create `Cargo.toml` with all dependencies (eframe, cpal, ringbuf, hound, midir, rfd)
  - Create `src/main.rs` with minimal `eframe::run_native()` setup
  - Create `src/app.rs` with `HtrkApp` struct implementing `eframe::App`
  - Verify build compiles and opens a window

- [x] **1.2** Define core data types
  - `src/sequencer/note.rs` — `Note` enum, frequency calculations, period table, display formatting
  - `src/sequencer/pattern.rs` — `Cell`, `Pattern` with constructor and accessors
  - `src/sequencer/sample.rs` — `Sample`, `LoopType`, `SampleFlags`, `VibratoWaveform`
  - `src/sequencer/instrument.rs` — `Instrument`, `Envelope`, `EnvelopePoint`, `EnvelopeFlags`, `NewNoteAction`, `DuplicateCheckType`
  - `src/sequencer/module.rs` — `Module`, `ModuleFormat`, `ModuleFlags`, constants
  - `src/sequencer/effect.rs` — `Effect` enum with all variants, `ExtendedEffect`
  - `src/sequencer/mod.rs` — Re-exports

- [x] **1.3** Define playback state types
  - `src/audio/voice.rs` — `Voice` struct, `EnvelopeState`
  - `src/sequencer/player.rs` — `SequencerState`, `ChannelState`
  - Define `AudioCommand` enum
  - Define `AtomicPlaybackState` struct with atomic fields

- [x] **1.4** Define error types
  - `FormatError`, `AudioError`, `EditError` enums
  - `Result` type aliases

### Deliverable

```
Cargo build succeeds
Window opens showing empty egui app
All data types compile and have basic unit tests
```

### Dependencies

None (starting point).

---

## Phase 2: IT Format Loader

**Goal**: Load and parse IT module files into `Module` struct.

**Estimated effort**: 5-7 days

### Tasks

- [x] **2.1** Format detection and trait
  - `src/formats/mod.rs` — `FormatHandler` trait, `detect_format()` function
  - `src/formats/common.rs` — Binary parsing helpers (`read_u8`, `read_u16_le`, `read_u32_le`, `read_string`)

- [x] **2.2** IT header parsing
  - `src/formats/it.rs` — Parse 192-byte IT header
  - Validate magic "IMPM"
  - Extract: song name, counts, flags, BPM, speed, volumes, channel panning/volume
  - Extract: order list, parapointers for instruments/samples/patterns

- [x] **2.3** IT sample loading
  - Parse 80-byte sample headers
  - Handle sample flags (16-bit, compressed, loop type)
  - Load raw 8-bit and 16-bit sample data
  - Convert to f32 normalized (-1.0 to 1.0)
  - Handle loop point parsing

- [x] **2.4** IT sample decompression
  - Implement IT214 decompression algorithm
  - Implement IT215 decompression algorithm
  - Handle both 8-bit and 16-bit compressed samples
  - Write unit tests with synthetic compressed data

  **Bug fixes applied (see `docs/formats.md` for full algorithm reference):**

  - **IT214/IT215 detection was using tracker version instead of convert byte.**
    The `cvt` byte bit 2 (`0x04`) is the authoritative IT215 flag, *not*
    `tracker_version >= 0x0215`. Files created by any tool could have either
    compression mode regardless of version. Fix: check `convert_byte & 0x04`
    when the sample is flagged as compressed (`flags_byte & 0x08`).

  - **16-bit decompressor was missing double-delta (IT215) integration.**
    Both 8-bit and 16-bit IT215 require two accumulators: `mem1 += delta;
    mem2 += mem1; output = mem2`. The 8-bit path had this, but the 16-bit path
    only had a single accumulator, producing garbage for all IT215 16-bit
    samples. Fix: added `value1`/`value2` double integration to the 16-bit
    decompressor, matching the 8-bit pattern.

  - **Integrator buffers were not reset between blocks.**
    Per the spec, both `mem1` and `mem2` reset to 0 at the start of every
    compressed block. The old code initialized them once outside the block
    loop, causing accumulation errors that compounded across block boundaries.
    Fix: moved `value1 = 0; value2 = 0` inside the block loop for both
    8-bit and 16-bit decompressors.

  - **Stereo compressed samples were not handled.**
    Stereo compressed IT samples store two independent compressed streams
    back-to-back, each with fully reset state. The old code ignored the
    `_is_stereo` parameter entirely. Fix: `decompress_it_sample` now loops
    over channels, passing `samples_already_decompressed` so the second
    channel's block boundaries are aligned correctly.

  - **Stereo raw sample loading had wrong stride.**
    For interleaved stereo data (L0,R0,L1,R1,...), the code read consecutive
    samples instead of every Nth sample, producing a mix of left and right
    channel data. Fix: `load_raw_sample` now uses `stride =
    bytes_per_sample * num_channels` to skip interleaved channels.

  - **Decompression errors were silently swallowed.**
    `parse_it_sample` matched `Err(_) => Arc::new(Vec::new())`, making
    decompression failures invisible and producing empty samples with no
    indication anything was wrong. Fix: errors now propagate with `?`.

  - **Convert byte flags were not fully handled.**
    Only bit 0 (signed/unsigned) was checked. Bits 1–2 were ignored:
    - Bit 1 (`0x02`): big-endian 16-bit samples — now handled in
      `load_raw_sample` with `u16::from_be_bytes`.
    - Bit 2 (`0x04`): delta PCM (running sum) for uncompressed samples —
      now decoded with accumulator in `load_raw_sample`.

  - **Stereo flag not guarded by version check.**
    Per the spec, the stereo flag in the sample header is only valid for
    files with `cwtv >= 0x0214`. Older files could have the bit set
    spuriously. Fix: `is_stereo` now requires both the flag bit and the
    version check.

- [x] **2.5** IT instrument loading
  - Parse 554-byte instrument headers
  - Extract sample map (120 entries)
  - Parse volume/panning/pitch envelope points
  - Parse NNA, duplicate check, fade-out settings
  - Handle auto-vibrato settings

- [x] **2.6** IT pattern loading
  - Parse packed pattern data
  - Handle channel mask decompression
  - Maintain channel memory for run-length decoding
  - Convert IT note values to `Note` enum
  - Convert IT effect codes to `Effect` enum
  - Handle variable-length pattern rows

- [x] **2.7** IT song message
  - Parse optional song message block
  - Store in `Module.message`

- [x] **2.8** Integration tests
  - Load real IT files (gather test corpus)
  - Verify: correct number of patterns, samples, instruments
  - Verify: sample data loads without errors
  - Verify: pattern data parses completely
  - Verify: note/effect values match reference

### Deliverable

```
Can load any IT file into Module struct
Unit tests pass for header, sample, instrument, pattern parsing
Integration tests with real IT files pass
```

### Dependencies

Phase 1 (data model).

---

## Phase 3: Audio Engine

**Goal**: Produce sound — play samples from loaded IT files.

**Estimated effort**: 5-7 days

### Tasks

- [x] **3.1** Audio device setup
  - `src/audio/device.rs` — cpal initialization
  - Enumerate output devices, select default
  - Configure: 48000 Hz, f32 stereo, buffer size 256
  - Build output stream with callback
  - Play/pause control

- [x] **3.2** Audio engine skeleton
  - `src/audio/engine.rs` — `AudioEngine` struct
  - Pre-allocate: voice pool (256 voices), mix buffers
  - Implement `process_callback()` that outputs silence initially
  - Integrate with cpal stream

- [x] **3.3** Lock-free command channel
  - Set up ringbuf with 256-slot capacity
  - `CommandSender` (UI side) and `CommandReceiver` (audio side)
  - Implement `process_commands()` in audio callback
  - Handle `Play`, `Stop`, `LoadModule` commands

- [x] **3.4** Shared playback state
  - `AtomicPlaybackState` with `AtomicU16`/`AtomicU8`/`AtomicBool` fields
  - Update from audio thread each callback
  - Read from UI thread for display

- [x] **3.5** Resampler implementation
  - `src/audio/resampler.rs`
  - Implement nearest-neighbor interpolation
  - Implement linear interpolation
  - Implement cubic Hermite spline interpolation
  - Unit tests for all three with known input/output

- [x] **3.6** Voice playback
  - `src/audio/voice.rs` — voice initialization from sample data
  - Implement sample position advancement with loop handling (forward, ping-pong)
  - Implement basic voice mixing (single voice → stereo output)
  - Test: play a sine wave sample, verify output frequency

- [x] **3.7** Mixer
  - `src/audio/mixer.rs`
  - Mix all active voices into stereo output
  - Apply per-voice volume and panning (constant power law)
  - Apply master volume
  - Brick-wall limiter to prevent clipping
  - Test with multiple simultaneous voices

### Deliverable

```
Audio device outputs sound
Can load IT file and trigger a single sample note
Can hear basic playback of a note
Ring buffer commands work between UI and audio threads
```

### Dependencies

Phase 1 (data model), Phase 2 (IT loader for test files).

---

## Phase 4: Sequencer / Playback

**Goal**: Play an entire IT module from start to finish.

**Estimated effort**: 7-10 days

### Tasks

- [x] **4.1** Sequencer state machine
  - `SequencerState` with play/pause/stop transitions
  - Track: current_order, current_row, current_tick, BPM, speed
  - Compute `samples_per_tick` from BPM

- [x] **4.2** Tick timing
  - Implement sample counter and tick advancement in audio callback
  - Tick 0 = row processing, Tick 1..N = mid-row effect processing
  - Row advancement at tick == speed

- [x] **4.3** Row processing (basic)
  - Read pattern cell data for each channel at current row
  - Trigger notes: allocate voice, set sample, compute frequency
  - Handle NoteOff, NoteCut
  - Set channel volume from volume column
  - Handle basic immediate effects: SetVolume (Cxx), SetSpeed (Fxx < 32), SetTempo (Fxx >= 32)

- [x] **4.4** Note triggering
  - Instrument → sample map lookup
  - Compute playback frequency from note + sample relative note + fine tune
  - Initialize voice with correct sample offset, volume, panning
  - Handle retriggering (same note, same channel)

- [x] **4.5** Effect processing — continuous effects (per-tick)
  - Portamento Up (1xx): linear frequency slide up
  - Portamento Down (2xx): linear frequency slide down
  - Tone Portamento (3xx): slide toward target note
  - Volume Slide (Axy): per-tick volume change
  - Implement effect memory (reuse last non-zero parameter)

- [x] **4.6** Effect processing — oscillating effects
  - Vibrato (4xy): sine/square/ramp waveform on pitch
  - Tremolo (7xy): sine/square/ramp waveform on volume
  - Use pre-computed vibrato table
  - Phase accumulation per tick

- [x] **4.7** Effect processing — special effects
  - Arpeggio (0xy): cycle through 3 notes per tick
  - Sample Offset (9xx): set voice start position
  - Position Jump (Bxx): jump to order
  - Pattern Break (Dxx): advance to next order at specific row
  - Set Panning (8xx)

- [x] **4.8** NNA (New Note Action) handling
  - NoteCut: immediately stop previous voice
  - Continue: keep previous voice, add new
  - NoteOff: begin envelope release
  - NoteFade: begin fade-out
  - Duplicate check type and action

- [x] **4.9** Envelope processing
  - Per-tick envelope advancement
  - Interpolation between envelope points
  - Sustain loop handling
  - Release phase (after note-off)
  - Apply to volume, panning, pitch
  - Fade-out processing

- [x] **4.10** Extended effects (IT-specific)
  - Global volume control (S1x)
  - Envelope position (S3x)
  - Panbrello (S4x)
  - Pattern delay (S6x)
  - Note cut after N ticks (ECx)
  - Note delay (EDx)
  - Retrigger (E9x / Q0x)

- [x] **4.11** Song end and looping
  - Detect end of order list
  - Handle position jump loops
  - Implement play modes: once, loop, pattern-loop

- [x] **4.12** Playback testing
  - Test with simple IT files (single pattern, few instruments)
  - Test with complex IT files (many effects, envelopes, NNAs)
  - Compare output with reference players (schism tracker, libopenmpt)
  - Verify timing accuracy

### Deliverable

```
Can play a complete IT module from start to finish
All basic effects work correctly
Envelopes, NNAs, and portamento work
UI displays current playback position
Transport controls (play/stop) work
```

### Dependencies

Phase 2 (IT loader), Phase 3 (audio engine).

---

## Phase 5: Pattern Editor UI

**Goal**: Edit patterns in the grid with keyboard and mouse.

**Estimated effort**: 7-10 days

### Tasks

- [x] **5.1** UI layout framework
  - `src/ui/mod.rs` — top-level panel layout (menu, transport, order list, pattern, bottom)
  - `src/ui/theme.rs` — `TrackerTheme` with default dark modern colors
  - Font setup (monospace for grid)

- [x] **5.2** Transport bar
  - `src/ui/transport.rs` — play/stop/pause/record buttons
  - BPM and speed spinners
  - Pattern and order number display
  - Volume slider
  - Connect buttons to AudioCommand sender

- [x] **5.3** Pattern grid rendering
  - `src/ui/pattern_grid.rs` — paint callback for grid
  - Draw row numbers, cell data, sub-columns
  - Fixed-width monospace rendering per cell
  - Row highlighting (every 4th or 8th row)
  - Playback position highlighting
  - Channel alternating background
  - Horizontal scrolling for channels beyond visible count
  - Vertical scrolling for patterns > visible rows

- [x] **5.4** Cursor and sub-column navigation
  - `CursorPosition` with row, channel, sub-column
  - Keyboard navigation (arrow keys, tab)
  - Cursor rendering (outline rectangle)
  - Follow-playback mode (cursor tracks playback row)

- [x] **5.5** Note entry via keyboard
  - Map lower keyboard row (Z-M) to notes in current octave
  - Map upper keyboard row (Q-U) to notes in octave+1
  - Enter note and advance to next row
  - Octave up/down controls (Ctrl+Z/X or numpad)
  - Clear cell with Delete/Backspace
  - Enter note-off with period key

- [x] **5.6** Hex value entry
  - In instrument sub-column: type 2 hex digits (0-F)
  - In volume sub-column: type 2 hex digits
  - In effect type sub-column: type 1 hex digit or letter
  - In effect param sub-column: type 2 hex digits
  - Auto-advance to next sub-column after entry

- [x] **5.7** Selection system
  - Shift+Arrow to extend selection
  - Click and drag to select
  - Visual highlighting of selected cells
  - Ctrl+A to select all
  - Escape to clear selection

- [x] **5.8** Copy/paste and editing
  - `src/edit/commands.rs` — `SetCellCommand`, `InsertRowCommand`, `DeleteRowCommand`
  - `src/edit/history.rs` — `UndoManager` with undo/redo stacks
  - Ctrl+C/X/V copy/cut/paste to clipboard
  - Ctrl+Z/Y undo/redo
  - Insert row, delete row operations
  - Transpose selection

- [x] **5.9** Order list widget
  - `src/ui/order_list.rs` — display order entries
  - Navigate with arrow keys
  - Edit pattern numbers with +/- or direct entry
  - Insert/delete orders
  - Highlight current playback order
  - Drag to reorder

- [x] **5.10** Channel headers
  - Display channel name/number
  - Mute button (M) per channel
  - Solo button (S) per channel
  - Click to rename

- [x] **5.11** Menu bar
  - File menu: New, Open, Save, Save As
  - Edit menu: Undo, Redo, Cut, Copy, Paste
  - View menu: Follow playback, theme selection
  - Connect all menu items to actions

- [x] **5.12** Status bar
  - Display: file format, current pattern/row, channel count, CPU usage

### Deliverable

```
Full pattern editing with keyboard
Undo/redo works
Copy/paste works
Order list editing works
File open works (Save is TODO)
Can compose a simple song from scratch
```

### Dependencies

Phase 3 (audio engine for playback controls), Phase 4 (sequencer for playback integration).

---

## Phase 6: Sample & Instrument Editors

**Goal**: Edit samples and instruments visually.

**Estimated effort**: 5-7 days

### Tasks

- [x] **6.1** Sample list panel
  - Display list of samples with names
  - Click to select, double-click to rename
  - Import sample from WAV file
  - Delete sample

- [x] **6.2** Waveform display
  - `src/ui/waveform.rs` — render sample waveform using egui Painter
  - Min/max decimation for zoom levels
  - Zoom and scroll with mouse wheel
  - Display loop markers (yellow vertical lines)
  - Display playback position cursor

- [x] **6.3** Sample property editor
  - Volume spinner (0-64)
  - Panning slider (0-64)
  - Loop type selector (None, Forward, PingPong)
  - Loop start/end spinners
  - Relative note spinner (-96 to +95)
  - Fine tune spinner (-128 to +127)
  - All changes create undo commands

- [x] **6.4** Sample operations
  - Normalize (scale to ±1.0)
  - Amplify (scale by factor)
  - Reverse
  - Crop to selection
  - All operations create undo commands with old/new sample data

- [x] **6.5** Loop marker dragging
  - Drag loop start/end markers in waveform view
  - Visual feedback during drag
  - Snap to zero-crossing (optional)
  - Undo support

- [x] **6.6** Instrument list panel
  - Display list of instruments with names
  - Click to select, double-click to rename
  - Create new instrument
  - Delete instrument
  - Duplicate instrument

- [x] **6.7** Envelope editor
  - `src/ui/instrument_editor.rs` — graphical envelope display
  - Draw envelope curve as connected line segments
  - Render draggable control points
  - Click to select, drag to move (constrained)
  - Double-click to add point
  - Right-click to delete point
  - Show sustain point (yellow)
  - Show loop points (green)
  - Enable/disable, sustain, loop checkboxes

- [x] **6.8** Sample mapping editor
  - Display note range → sample assignment
  - Visual grid: X = notes (C-0 to B-9), Y = velocity or split
  - Click to assign sample to note range
  - Right-click for context menu

- [x] **6.9** Instrument properties
  - NNA selector (Cut/Continue/Off/Fade)
  - Duplicate check type and action
  - Fade-out spinner (0-4095)
  - Pitch-pan separation and center
  - Global volume
  - Auto-vibrato settings

### Deliverable

```
Waveform display with zoom/scroll
Sample editing (properties, operations)
Loop point editing with drag
Envelope editor with drag
Instrument sample mapping
Complete instrument editing
```

### Dependencies

Phase 5 (UI framework, edit system).

---

## Phase 7: Additional Formats & Save

**Goal**: Load and save IT, XM, S3M, MOD files.

**Estimated effort**: 7-10 days

### Tasks

- [x] **7.1** IT writer
  - Serialize `Module` to IT binary format
  - Pack pattern data (RLE compression)
  - Write sample data (raw, optionally IT214 compressed)
  - Write instrument headers with envelopes
  - Write order list and headers
  - Round-trip test: load → save → reload, compare

- [x] **7.2** MOD loader
  - `src/formats/modfile.rs`
  - Parse 1084-byte header
  - Decode 4-byte note words (period + sample + effect)
  - Load 8-bit sample data
  - Convert: period → Note, 4 channels → 64 channels with hard-panning
  - Create one instrument per sample

- [x] **7.3** S3M loader
  - `src/formats/s3m.rs`
  - Parse 96-byte header, validate "SCRM" magic
  - Load order list, instrument pointers, pattern pointers
  - Decode packed pattern data
  - Load sample data (para-pointer based)
  - Convert: S3M note encoding → Note, samples → instruments

- [x] **7.4** XM loader
  - `src/formats/xm.rs`
  - Parse header (336+ bytes)
  - Load pattern data (packed format)
  - Load instruments with sample key maps
  - Decode delta-packed sample data
  - Handle 16-bit samples
  - Convert: XM envelopes → IT envelopes, XM key map → IT sample map

- [x] **7.5** XM writer
  - Serialize `Module` to XM format
  - Delta-encode samples
  - Handle constraints (max 32 channels, max 128 instruments)
  - Write pattern data in XM packed format

- [x] **7.6** S3M writer
  - Serialize `Module` to S3M format
  - Handle constraints (max 32 channels, no envelopes)
  - Convert notes to S3M encoding

- [x] **7.7** WAV import/export
  - Import WAV files as samples using `hound`
  - Support 8-bit, 16-bit, 24-bit, 32-bit float WAV
  - Stereo → mono mixdown option
  - Export individual samples as WAV

- [x] **7.8** File dialog integration
  - Use `rfd` for native file open/save dialogs
  - Filter by supported formats
  - Recent files list
  - Drag-and-drop files onto window

- [x] **7.9** Format conversion testing
  - Round-trip tests for each format
  - Cross-format conversion tests (load XM → save as IT, compare playback)
  - Edge case tests: empty patterns, max channels, large samples

### Deliverable

```
Load IT, XM, S3M, MOD files
Save IT, XM, S3M files
WAV import for samples
File dialogs work
Round-trip tests pass
```

### Dependencies

Phase 2 (IT loader), Phase 5 (file menu integration).

---

## Phase 8: Polish & Advanced Features

**Goal**: Production-quality application.

**Estimated effort**: 10-15 days

### Tasks

- [ ] **8.1** Configuration persistence
  - Save/load config TOML file
  - Remember: window size, last opened file, audio settings, theme
  - `dirs` crate for platform-specific config directory

- [ ] **8.2** Keyboard shortcut customization
  - Settings dialog for keybindings
  - Load/save custom keybindings
  - Default keybindings as fallback

- [ ] **8.3** MIDI input
  - `src/midi/handler.rs` — midir integration
  - Enumerate MIDI input devices
  - Map MIDI note-on → tracker note entry
  - Map MIDI velocity → volume
  - Map MIDI CC → effects (CC1=modwheel→vibrato, CC7→volume)
  - Live recording mode (record MIDI input to pattern)

- [ ] **8.4** Song export to WAV
  - Render entire song to WAV file offline
  - Progress bar during render
  - Support for loop rendering (N loops or first loop only)

- [ ] **8.5** Advanced sample operations
  - Crossfade loops
  - Sample loop tuning
  - DC offset removal
  - Silence generation
  - Mix paste (mix clipboard with existing)

- [ ] **8.6** Advanced pattern operations
  - Interpolate selection (gradient between first and last values)
  - Randomize selection
  - Scale selection (multiply volumes, transpose notes)
  - Humanize (add random timing/velocity variation)
  - Pattern template library

- [ ] **8.7** Soft-knee limiter
  - Replace brick-wall clipping with proper lookahead limiter
  - Configurable threshold, ceiling, attack, release
  - Visual feedback on gain reduction

- [ ] **8.8** Theme system
  - Multiple built-in themes (Dark Modern, Dark Retro, Light)
  - Custom theme editor
  - Import/export theme files
  - Theme preview

- [ ] **8.9** Accessibility
  - Keyboard-only operation for all features
  - High-contrast mode
  - Screen reader support (egui's built-in)

- [ ] **8.10** Performance optimization
  - Profile audio callback (ensure < 10ms budget)
  - SIMD acceleration for mixing (std::simd or manual)
  - Lazy waveform rendering (only redraw on change)
  - Pattern grid virtualization (only render visible rows/channels)
  - Memory optimization for large samples

- [ ] **8.11** Crash recovery
  - Auto-save every N minutes
  - Recovery file on startup if crash detected
  - Temp file for unsaved changes

- [ ] **8.12** Documentation
  - In-app help system (F1)
  - README.md with build instructions
  - CONTRIBUTING.md for contributors

### Deliverable

```
Polished, production-ready tracker
MIDI input works
WAV export works
Configuration persists between sessions
Good performance with complex modules
```

### Dependencies

All previous phases.

---

## Milestone Summary

| Milestone | Phase | Key Achievement |
|-----------|-------|-----------------|
| **M1: Types** | Phase 1 | All data types defined, project compiles |
| **M2: Loading** | Phase 2 | IT files load correctly |
| **M3: Sound** | Phase 3 | Audio output works, can play a sample |
| **M4: Playback** | Phase 4 | Full IT module playback with effects |
| **M5: Editing** | Phase 5 | Pattern editing with keyboard, undo/redo |
| **M6: Samples** | Phase 6 | Sample/instrument editing with visual editors |
| **M7: Formats** | Phase 7 | Multi-format support, save/export |
| **M8: Release** | Phase 8 | Production quality, MIDI, export |

## Critical Path

```
Phase 1 (3d) → Phase 2 (7d) → Phase 3 (7d) → Phase 4 (10d) → Phase 5 (10d)
                                                          ↘ Phase 6 (7d)
                                                          ↘ Phase 7 (10d)
                                                                            → Phase 8 (15d)
```

**Critical path total**: ~52 days (10 weeks) of focused development.

## Risk Areas

| Risk | Impact | Mitigation |
|------|--------|-----------|
| IT format edge cases | High | Build test corpus early; compare against schism/libopenmpt |
| Audio thread safety | Critical | No allocation policy; extensive testing with sanitizers |
| Effect command accuracy | High | Test against reference implementations; use module files as test data |
| egui performance with large grids | Medium | Virtualize rendering; benchmark early |
| Cross-platform audio | Medium | Test on Windows, macOS, Linux; use cpal which handles platform differences |
| XM/S3M format quirks | Medium | Start with IT; other formats can be delayed if needed |
