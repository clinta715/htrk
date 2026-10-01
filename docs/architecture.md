# Architecture

## High-Level System Diagram

```
┌────────────────────────────────────────────────────────────────────┐
│                         UI Thread (eframe)                         │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │                     App State (owned)                        │  │
│  │  ┌────────────┐ ┌────────────┐ ┌──────────────────────────┐ │  │
│  │  │ EditHistory│ │ ModuleState│ │ PlaybackState (shared)    │ │  │
│  │  │ (undo/redo)│ │ (patterns, │ │ Arc<AtomicPlaybackState> │ │  │
│  │  │            │ │  samples)  │ │ → read by UI for cursor  │ │  │
│  │  └────────────┘ └────────────┘ └──────────────────────────┘ │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                              │                                     │
│                    CommandSender (ringbuf Producer)                 │
│                              │ (lock-free)                         │
╞══════════════════════════════╪════════════════════════════════════╡
│                              ▼                                     │
│                    CommandReceiver (ringbuf Consumer)               │
│                              │                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │                  Audio Thread (cpal callback)                 │  │
│  │                                                              │  │
│  │  Sequencer ──→ Voice Pool ──→ Mixer ──→ Output Buffer       │  │
│  │  (tick/row     (active       (sum all    (stereo f32         │  │
│  │   advancement)  voices)       voices)     to cpal)           │  │
│  └──────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────┘
```

## Thread Architecture

### Thread Responsibilities

#### UI Thread (main thread)
- Owns the `eframe` event loop
- Owns the `Module` data (patterns, samples, instruments)
- Owns the `EditHistory` stack
- Renders all egui widgets
- Sends commands to the audio thread via lock-free ring buffer
- Reads shared atomic state for playback position display

#### Audio Thread (real-time priority)
- Runs inside `cpal`'s audio callback
- **Never allocates** memory (pre-allocated buffers)
- **Never blocks** (no mutexes, no file I/O)
- Processes `AudioCommand`s from ring buffer at start of each callback
- Advances sequencer state (tick/row advancement)
- Mixes all active voices into output buffer
- Updates `AtomicPlaybackState` for UI consumption

### Communication: Lock-Free SPSC Ring Buffer

```
UI Thread                          Audio Thread
─────────                          ────────────
CommandSender ──[ringbuf]──→ CommandReceiver
    (Producer)                       (Consumer)

Shared (Arc):
  AtomicPlaybackState {
      current_order:   AtomicU16
      current_row:     AtomicU16
      current_pattern: AtomicU16
      bpm:             AtomicU16
      speed:           AtomicU8
      playing:         AtomicBool
      active_voices:   AtomicU8
      cpu_usage_pct:   AtomicU8        // 0-100, updated each callback
  }
```

#### AudioCommand Enum

All commands sent from UI to audio thread:

```rust
enum AudioCommand {
    // Transport
    Play,
    PlayFrom { order: u16, row: u16 },
    Stop,
    Pause,
    SetBPM(u16),
    SetSpeed(u8),
    SetGlobalVolume(u8),

    // Module loading
    LoadModule(Arc<Module>),   // Arc to avoid copies

    // Channel muting/solo
    SetChannelMuted { channel: usize, muted: bool },
    SetChannelSolo { channel: usize, solo: bool },

    // Live editing (changes during playback)
    SetPatternCell { order: usize, row: usize, channel: usize, cell: Cell },

    // Audio settings
    SetMasterVolume(f32),       // 0.0 - 1.0
    SetInterpolation(InterpolationType),

    // Seek
    SeekTo { order: u16, row: u16 },
}
```

#### Ring Buffer Sizing

- Buffer capacity: **256 commands** — generous to prevent overflow during rapid edits
- If the buffer overflows, commands are dropped (audio thread never blocks)
- Overflow is logged via a non-blocking counter (`AtomicU32::fetch_add`)

### Module Data Sharing

The `Module` struct is shared between threads using `Arc`:

- **During editing**: UI clones the `Arc<Module>`, mutates a copy, then sends the new `Arc<Module>` via `LoadModule` command
- **During playback**: Audio thread reads from `Arc<Module>` immutably
- **Pattern cell updates during playback**: Sent as individual `SetPatternCell` commands so the audio thread can apply live edits without a full module reload

### Memory Allocation Strategy

| Area | Strategy | Notes |
|------|----------|-------|
| Audio thread output buffer | Pre-allocated on init | Sized to cpal buffer size * 2 (stereo) |
| Voice pool | Pre-allocated array of N voices | N = MAX_VOICES (e.g., 256) |
| Resampler temporary buffers | Pre-allocated per voice | 16-sample window per voice |
| Ring buffer | Pre-allocated on init | 256-slot capacity |
| Sample data | `Arc<Vec<f32>>` | Reference-counted, zero-copy sharing |
| Pattern data edits | Copy-on-write via `Arc` | UI clones, audio reads old Arc until swap |

## Crate/Module Structure

```
htrk/
├── Cargo.toml
├── src/
│   ├── main.rs                        # Binary entry point
│   ├── lib.rs                         # Library root (module declarations)
│   ├── app/                           # === APP SHELL (impl HtrkApp) ===
│   │   ├── mod.rs                     # HtrkApp struct + AppView/BrowserPurpose + constructors
│   │   ├── audio_setup.rs             # device init, rescan, channel/send sync
│   │   ├── editing.rs                 # cursor movement, selection, block ops
│   │   ├── preamble.rs                # per-frame state sync + MCP snapshots
│   │   ├── dialogs.rs                 # modal dialogs (settings, export, help, about)
│   │   ├── panels.rs                  # view tabs + menu/transport/status bars
│   │   ├── plugins.rs                 # plugin install/unload/state persistence
│   │   ├── lifecycle.rs               # exit-time shutdown
│   │   └── eframe_impl.rs             # `impl eframe::App for HtrkApp`
│   ├── app_config.rs                  # Persisted AppConfig (TOML)
│   ├── errors.rs                      # FormatError / AudioError / EditError
│   │
│   ├── audio/                         # === AUDIO ENGINE ===
│   │   ├── mod.rs                     # Public API re-exports
│   │   ├── engine.rs                  # AudioEngine: cpal device/stream + callback
│   │   ├── commands.rs                # AudioCommand enum
│   │   ├── playback_state.rs          # AtomicPlaybackState (shared with UI)
│   │   ├── mixer.rs                   # Mix voices -> stereo output, gain ramping
│   │   ├── renderer.rs                # Tick chunk rendering
│   │   ├── voice.rs / voice_pool.rs   # Per-note voice state + pool
│   │   ├── resampler.rs               # Interpolation (cubic/linear/nearest)
│   │   ├── filter.rs                  # Resonant filter + Amiga LED filter
│   │   ├── sendfx.rs                  # Send-bus effect processors
│   │   ├── effects/                   # Format-conditional effect processors
│   │   │   ├── legacy.rs              # MOD/S3M/IT processor
│   │   │   └── xm.rs                  # XM processor
│   │   ├── sequencer_engine/          # Tick/row state machine
│   │   │   ├── mod.rs
│   │   │   ├── advance.rs             # Row advancement + per-row resets
│   │   │   ├── cell.rs                # Cell decoding / note triggering
│   │   │   ├── period.rs              # Portamento/vibrato math
│   │   │   ├── helpers.rs
│   │   │   └── tests.rs               # Sequencer regression tests
│   │   └── plugins/                   # CLAP plugin hosting
│   │       ├── clap_plugin/           # CLAP host + handle + processor
│   │       │   ├── mod.rs             # shared imports + re-exports
│   │       │   ├── host.rs            # host handler (log/GUI callbacks)
│   │       │   ├── handle.rs          # main-thread ClapPluginHandle
│   │       │   ├── processor.rs       # audio-thread ClapPluginProcessor
│   │       │   └── tests.rs
│   │       ├── discovery.rs / library.rs
│   │       ├── preset_discovery.rs / preset_library.rs
│   │       ├── param_ring.rs
│   │       └── plugin_window.rs
│   │
│   ├── sequencer/                     # === DATA MODEL ===
│   │   ├── module.rs                  # Module, OrderList, ModuleFlags
│   │   ├── pattern.rs                 # Pattern, Row, Cell
│   │   ├── instrument.rs              # Instrument, Envelope, EnvelopePoint
│   │   ├── sample.rs                  # Sample, LoopType, SampleFlags
│   │   ├── note.rs                    # Note enum, frequency/period tables
│   │   ├── effect.rs                  # Universal + format-specific Effect enums
│   │   ├── player.rs                  # ChannelState and transport state
│   │   ├── envelope_generator.rs      # Envelope generation tools
│   │   ├── period.rs                  # Period tables
│   │   ├── plugin.rs                  # PluginSlot persistence
│   │   ├── automation.rs              # AutomationTarget / curves
│   │   └── slice_detector.rs          # Sample slice detection
│   │
│   ├── formats/                       # === FILE FORMAT HANDLERS ===
│   │   ├── mod.rs                     # FormatHandler trait, format detection
│   │   ├── it.rs / xm.rs / s3m.rs     # IT, XM, S3M
│   │   ├── modfile.rs                 # Amiga ProTracker .mod
│   │   ├── c669.rs / mmd.rs / stm.rs / ult.rs
│   │   ├── htk.rs / hti.rs            # Native HTRK song / instrument
│   │   ├── wav.rs                     # WAV import/export
│   │   ├── midi.rs                    # Standard MIDI File import
│   │   └── common.rs                  # Shared parsing utilities
│   │
│   ├── edit/                          # === EDITING SYSTEM ===
│   │   ├── history.rs                 # UndoManager: command stack
│   │   ├── commands.rs                # EditCommand trait + command types
│   │   └── block_ops.rs               # Block selection operations
│   │
│   ├── core/                          # === UI-AGNOSTIC APP CORE ===
│   │   ├── mod.rs                     # HtrkCore: Module + history + sync helpers
│   │   ├── editing.rs                 # Edit dispatch wrappers
│   │   ├── channels.rs                # Channel volume/panning helpers
│   │   └── automation.rs              # Automation edit helpers
│   │
│   ├── mcp/                           # === MCP SERVER ===
│   │   ├── server.rs                  # JSON-RPC over TCP
│   │   ├── http.rs                    # HTTP/SSE transport
│   │   ├── protocol.rs                # JSON-RPC types
│   │   ├── tools.rs / resources.rs    # Read-only tools + resources
│   │   ├── library.rs                 # Sample library + filename heuristics
│   │   ├── plugin_tools.rs / preset_tools.rs
│   │   └── mutations/                 # Main-thread mutation handlers
│   │
│   ├── tools/                         # === COMPOSITION TOOLS ===
│   │   ├── phrase_generator.rs        # Melodic/Euclidean/Drum/Chord generators
│   │   └── scale.rs                   # Scale + Euclidean helpers
│   │
│   ├── actions/                       # === UI ACTIONS ===
│   │   ├── keyboard.rs                # Key dispatch, focus gate, Alt-menu nav
│   │   ├── file_io.rs                 # Load/save/import/export coordination
│   │   ├── instrument_edit.rs
│   │   ├── sample_edit.rs
│   │   └── slice_to_instrument.rs
│   │
│   ├── ui/                            # === UI WIDGETS ===
│   │   ├── mod.rs                     # Layout orchestration
│   │   ├── pattern_grid.rs / pattern_view.rs
│   │   ├── order_list.rs / channel_headers.rs
│   │   ├── sample_editor.rs / waveform.rs
│   │   ├── instrument_editor.rs / envelope_editor.rs
│   │   ├── mixer_view.rs / sendfx_editor.rs
│   │   ├── playback_view.rs / oscilloscope.rs / transport.rs
│   │   ├── automation_editor.rs
│   │   ├── file_browser.rs / sample_library.rs / plugin_browser.rs
│   │   ├── phrase_generator_dialog.rs / settings_window.rs
│   │   ├── wav_export_window.rs / sample_export_dialog.rs
│   │   ├── menu_bar.rs / status_bar.rs / help_screen.rs
│   │   └── style.rs / theme.rs
│   │
│   └── bin/
│       └── analyze_mod.rs             # Developer diagnostic binary
│
└── tests/
    ├── format_conversions.rs          # HTK roundtrip + automation persistence
    └── mcp_integration.rs             # MCP protocol smoke tests
```
```

### Module Dependency Graph

```
main.rs
  └── app/                             (HtrkApp)
        ├── core::HtrkCore           (Module + UndoManager)
        ├── audio::engine            (cpal init + AudioEngine)
        ├── audio::commands          (AudioCommand -> ring buffer)
        ├── ui::*                    (all UI widgets)
        ├── actions::*               (keyboard + file I/O + edits)
        ├── formats::*               (load/save/import/export)
        └── mcp::server              (optional automation surface)
```
```

## Error Handling Strategy

| Layer | Strategy | Details |
|-------|----------|---------|
| Format parsing | `Result<T, FormatError>` | Non-critical: show error dialog in UI |
| Audio device | `Result<T, AudioError>` | Critical at startup, non-critical during runtime |
| Audio callback | No panics, no errors | All errors handled gracefully (skip voice, etc.) |
| UI | egui handles its own errors | Application-level: show in status bar |
| Edit commands | `Result<T, EditError>` | Undoable — invalid edits return error, don't panic |

### Error Types

```rust
enum FormatError {
    Io(std::io::Error),
    InvalidHeader { expected: &'static str, found: [u8; 4] },
    TruncatedFile { expected_size: usize, actual_size: usize },
    UnsupportedVersion { version: u16 },
    InvalidPatternIndex { index: usize, max: usize },
    InvalidSampleIndex { index: usize, max: usize },
    DecompressionFailed(String),
    Utf8Error(std::str::Utf8Error),
}

enum AudioError {
    NoDeviceAvailable,
    DeviceOpenFailed(String),
    UnsupportedSampleRate { requested: u32, available: Vec<u32> },
    StreamCreationFailed(String),
}

enum EditError {
    NoSelection,
    CannotPasteDifferentChannels,
    PatternFull,
    InvalidNoteValue,
}
```

## Initialization Sequence

```
1. main() → parse CLI args (optional file to open)
2. main() → eframe::run_native() with HtrkApp::default()
3. HtrkApp::new():
   a. Load config (window size, last file, theme)
   b. Initialize audio device (cpal)
   c. Create AudioEngine with pre-allocated voice pool
   d. Create ring buffer (Producer → engine, Consumer → callback)
   e. Initialize AtomicPlaybackState (Arc shared)
   f. If CLI arg provided, load module file
   g. Return app instance
4. First frame:
   a. egui layout renders
   b. Audio stream starts (cpal::Stream::play())
   c. If module loaded, display in UI
```

## Performance Budget

| Operation | Budget | Notes |
|-----------|--------|-------|
| Audio callback | < 10ms | At 48kHz / 256 buffer = ~5.3ms per callback |
| Single voice mix | < 20µs | 256 samples * cubic interpolation |
| UI frame render | < 16ms | 60 FPS target |
| Pattern grid draw | < 5ms | 64 rows * 32 channels with scrolling |
| Waveform draw | < 3ms | egui Painter lines, decimated |
| File load (IT) | < 100ms | Must not block audio (pause playback first) |
| Undo/redo | < 1ms | Stack operations, no deep copies |

## Configuration

Stored in a TOML file at platform-specific config directory:

```toml
[audio]
sample_rate = 48000
buffer_size = 256
interpolation = "cubic"         # "cubic" | "linear" | "nearest"
master_volume = 0.8

[ui]
theme = "dark_modern"           # "dark_modern" | "dark_retro" | "light"
font_size = 14
pattern_font_size = 12
show_hex_row_numbers = true
visible_channels = 8            # columns visible without scrolling

[editor]
default_octave = 4
default_bpm = 125
default_speed = 6
default_rows = 64
edit_mode = "overwrite"         # "overwrite" | "insert"

[midi]
enabled = false
input_device = ""
channel = 0                     # 0-15, or "all"

[keybindings]
# User-overrideable keybindings (see ui-design.md for defaults)
```
