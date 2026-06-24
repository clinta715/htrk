# CLAP/VST Plugin Hosting Plan

## Overview

Support CLAP and VST3 plugins as **send bus effects** and **instruments** within htrk. This document covers the full roadmap, with Phases 1-2 (CLAP send FX) as the immediate implementation target.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| First integration | Send bus FX | Lower risk, validates hosting infrastructure before instruments |
| Initial format | CLAP only | Cleaner Rust bindings (`clack-host`), type-safe threading; VST3 added later |
| Instrument instances | Shared multi-timbral | One plugin instance per instrument; channels share via MIDI channel routing |

## Crate Choices

| Crate | Role | Maturity |
|-------|------|----------|
| **`clack-host`** | CLAP hosting (type-safe lifecycle) | Mature, 100% documented, MIT/Apache-2.0 |
| **`vst3-host`** (future) | VST3 hosting (discovery, MIDI, state) | Usable, MIT (avoids GPL trap of `vst3-sys`) |
| **`raw-window-handle` 0.6** | Native window handle for editor embedding | Stable |
| **`rtrb`** | SPSC ring buffer for parameter automation | Stable, RT-safe |

## Current Architecture (relevant facts)

The audio engine is single-threaded — everything runs in cpal's `process_callback`:

```
voices → per-channel buffers (pre/post-fader) → master L/R
                                                    ↓
                         send buses (tap, process, return) → limiter → output
```

Key integration points:
- `SendBus` has `effect: Option<Box<dyn SendEffect>>` — the closest existing "plugin" abstraction (stereo in/out, params, no MIDI)
- `AudioCommand` enum + SPSC ring buffer (`ringbuf` crate) for UI→audio communication
- Per-channel buffers use planar layout (L-block, R-block) — matches CLAP/VST3's non-interleaved model
- No plugin infrastructure currently exists (zero dependencies, zero code)

---

## Phase 1 — Plugin Abstraction & Discovery (~3-4 days)

Define the format-agnostic layer that the audio engine talks to.

### HostedPlugin Trait

```rust
/// Audio-thread side: processes audio buffers.
/// Must be Send. Must not allocate in process().
pub trait HostedPluginProcessor: Send {
    fn process(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        frame_count: usize,
        transport: &TransportInfo,
    );
    fn set_parameter(&mut self, param_id: u32, value: f32);
    fn get_parameter(&self, param_id: u32) -> f32;
    fn parameter_count(&self) -> u32;
    fn parameter_name(&self, param_id: u32) -> &str;
    fn latency(&self) -> u32;
}

/// Main-thread side: lifecycle + state management.
/// Not Send (CLAP PluginInstance is !Send).
pub trait HostedPluginLifecycle {
    fn activate(&mut self, sample_rate: f64, max_block: u32) -> Box<dyn HostedPluginProcessor>;
    fn deactivate(&mut self);
    fn save_state(&self) -> Vec<u8>;
    fn load_state(&mut self, state: &[u8]);
    fn parameter_info(&self) -> &[ParamInfo];
}
```

### Data Types

```rust
pub struct PluginDescriptor {
    pub name: String,
    pub vendor: String,
    pub format: PluginFormat,      // Clap | Vst3
    pub path: PathBuf,
    pub plugin_id: String,         // CLAP id or VST3 CID
    pub plugin_type: PluginType,   // Instrument | Effect | Both
}

pub struct AudioBus {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

pub struct TransportInfo {
    pub bpm: f64,
    pub sample_rate: f64,
    pub sample_position: u64,
    pub is_playing: bool,
}
```

### Plugin Discovery (`src/audio/plugins/discovery.rs`)

Scan standard paths + user-configured roots. Probe each plugin in isolation (subprocess or timeout-guarded). Cache results in `PluginLibrary` (mirrors `SampleLibrary` pattern).

### CLAP Discovery Paths

| OS | Paths |
|----|-------|
| Windows | `C:\Program Files\Common Files\CLAP\` |
| macOS | `/Library/Audio/Plug-Ins/CLAP/`, `~/Library/Audio/Plug-Ins/CLAP/` |
| Linux | `/usr/lib/clap/`, `~/.clap/` |

Plus user-configured paths from `AppConfig.clap_scan_paths`.

### MCP Tools

- `plugin.scan` — scan directories, populate metadata cache
- `plugin.list` — list discovered plugins with filters
- `plugin.info` — get detailed info about a specific plugin

---

## Phase 2 — Send Bus CLAP FX (~2-3 days)

The simpler integration — plugins replace `SendEffect` on a bus.

### Architecture: Split Plugin Model

CLAP's threading model requires the plugin to live in two places:

```
┌─────────────────────────────────────────────────────────────────┐
│ MAIN THREAD                                                     │
│                                                                 │
│  ClapPluginHandle                                               │
│    ├── instance: PluginInstance<HostHandlers>  (!Send)          │
│    ├── param_queue_tx: rtrb::Producer<ParamChange>             │
│    └── descriptor: PluginDescriptor                             │
│                                                                 │
│  Lifecycle: create → activate → [send processor to audio]      │
│             → ... → [recall processor] → deactivate → drop     │
└──────────────────────┬──────────────────────────────────────────┘
                       │ AudioCommand via SPSC ring buffer
                       │ (processor passed once at assignment)
┌──────────────────────▼──────────────────────────────────────────┐
│ AUDIO THREAD                                                    │
│                                                                 │
│  ClapPluginProcessor  (Send)                                    │
│    ├── processor: StartedAudioProcessor          (Send, !Sync)  │
│    ├── param_queue_rx: rtrb::Consumer<ParamChange>             │
│    ├── output_l: Vec<f32>  (pre-allocated)                     │
│    ├── output_r: Vec<f32>  (pre-allocated)                     │
│    └── event_buffer: clack EventBuffer (pre-allocated)          │
│                                                                 │
│  Each callback:                                                 │
│    1. Drain param_queue_rx → fill event_buffer                  │
│    2. processor.process(silence_in, &mut [out_l, out_r], ...)   │
└─────────────────────────────────────────────────────────────────┘
```

### Extended SendBus

```rust
struct SendBus {
    buffer: Vec<f32>,              // stereo-interleaved
    return_level: f32,
    pre_fader: bool,
    effect: Option<Box<dyn SendEffect>>,           // existing built-in
    plugin: Option<Box<dyn HostedPluginProcessor>>, // NEW: hosted plugin
}
```

### Signal Flow Change

```
For each send bus (in process_callback):
    1. Zero bus buffer
    2. Tap from channel buffers scaled by send levels (existing)
    3. IF plugin assigned:
         a. Drain parameter queue into event buffer
         b. Call plugin.process(bus_buffer → planar L/R, event_buffer, transport)
         c. Copy plugin output back to bus buffer
    4. ELSE IF SendEffect assigned:
         a. Existing: de-interleave, SendEffect::process, re-interleave
    5. ELSE: pass through (or silence)
    6. Add bus buffer to master scaled by return_level (existing)
```

### New AudioCommands

```rust
AudioCommand::SetSendPlugin {
    send_index: usize,
    processor: Box<dyn HostedPluginProcessor>,  // already activated + started
}
AudioCommand::RemoveSendPlugin {
    send_index: usize,
}
AudioCommand::SetSendPluginParam {
    send_index: usize,
    param_id: u32,
    value: f32,
}
```

### Persistence Model

```rust
// src/sequencer/module.rs

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PluginSlot {
    pub format: String,           // "clap" (future: "vst3")
    pub path: String,             // path to .clap file
    pub plugin_id: String,        // CLAP plugin id
    pub state: Vec<u8>,           // opaque state blob from clap_state.save()
    pub last_window_size: Option<(u32, u32)>,  // for editor restoration
}

// In Module:
pub send_bus_plugins: [Option<PluginSlot>; NUM_SEND_BUSES],
```

On `.htk` load: if `send_bus_plugins[i]` is `Some`, main thread loads the plugin, activates it, restores state, and sends the processor to the audio thread.

On `.htk` save: for each bus with a plugin, call `state.save()` → store the blob.

### UI

In Send FX tab, add "Plugin..." button next to the effect-type dropdown. Opens plugin browser dialog (scan results list → select → assign).

### MCP Tools

- `plugin.assign_to_bus { bus_index, plugin_descriptor }` (mutation)
- `plugin.remove_from_bus { bus_index }` (mutation)
- `plugin.set_param { bus_index, param_id, value }` (mutation)

---

## Phase 3 — Plugin Instruments (~5-7 days)

Plugins replace sample-based voices for a channel.

### Instrument Source Model

```rust
pub enum InstrumentSource {
    Samples(Instrument),           // existing
    Plugin(PluginInstrument),      // NEW
}

pub struct PluginInstrument {
    pub name: String,
    pub plugin: PluginSlot,        // { format, path, id, state }
    pub midi_channel: u8,          // 0-15 (for multitimbral routing)
    pub volume: u8,
    pub panning: u8,
    pub param_macros: [Option<ParamMacro>; 8],  // tracker effect → plugin param
}

pub struct ParamMacro {
    pub param_id: u32,
    pub source: MacroSource,       // VolumeCol | EffectCol(effect_cmd) | CC(controller)
    pub min: f32,
    pub max: f32,
}
```

### Sequencer Integration

When `process_cell_unified()` encounters a note on a channel whose instrument is `Plugin`:
1. Instead of `voice_pool.allocate_voice(...)`, enqueue a `PluginNoteEvent { plugin_slot, midi_ch, key, velocity }` into a per-tick event buffer.
2. Note-off/fade handled via `NoteOff`/`NoteCut` events.

### Audio Engine Integration

Add `plugin_instruments: Vec<Option<Box<dyn HostedPluginProcessor>>>` to `AudioEngine`. In `process_callback`, after voice mixing but before send taps:

```rust
for (ch_idx, plugin) in active_plugin_instruments {
    plugin.process(&silent_input, &mut plugin_out, &pending_events, &transport);
    // mix plugin_out into ch_mix[ch_idx] and pre_ch_mix[ch_idx]
}
```

### Multi-timbral Routing

One plugin instance per instrument slot. Multiple channels referencing the same plugin instrument share the instance, distinguished by MIDI channel. Efficient (1 instance for 4+ channels) and matches the traditional tracker+MIDI-synth model.

---

## Phase 4 — Parameter Automation (~2-3 days)

Map tracker columns to plugin parameters:
- Volume column → plugin param (e.g., filter cutoff)
- Effect column with specific command → plugin param
- Automation tracks (existing system) → plugin params

### SPSC Parameter Queue

`rtrb::Producer<ParamChangeEvent>` filled on sequencer tick, drained in audio callback before `plugin.process()`.

```rust
struct ParamChangeEvent {
    param_id: u32,
    value: f32,
    sample_offset: u32,  // within the current block
}
```

---

## Phase 5 — Plugin Editor UI (DONE for Windows; macOS/Linux deferred)

**Status:** Windows floating + embedded editors working (June 2026).

### What ships

- `HostedPluginHandle` trait gains `open_editor()`, `close_editor()`,
  `is_editor_open()`, `has_editor()` methods (`src/audio/plugins/mod.rs`).
- `ClapPluginHandle` implements them via clack's GUI extension
  (`src/audio/plugins/clap_plugin.rs`).
- `HtrkApp` stores main-thread plugin handles in
  `send_bus_handles: [Option<Box<dyn HostedPluginHandle>>; NUM_SEND_BUSES]`
  (replaces the `std::mem::forget(handle)` hack in
  `load_and_install_plugin`).
- `SendFxPanel` now has an "Edit..." / "Close" button per send bus that
  toggles the editor. The "Remove" button also closes the editor.
- `src/audio/plugins/plugin_window.rs` (Windows-only) creates a
  top-level HWND via `windows-sys 0.59` for the embedded-fallback path.
- Tests: 335 lib tests pass; `test_editor_open_close_real_plugin` opens
  and closes TAL Reverb 4's editor and verifies the lifecycle.

### Architecture

1. `open_editor()` first probes floating mode
   (`GuiApiType::WIN32, is_floating=true`). If the plugin supports it
   (e.g. plugins that manage their own top-level window), the plugin
   creates and shows its own window — no host HWND needed.
2. If floating is unsupported, fall back to embedded mode
   (`is_floating=false`). We create a top-level `WS_OVERLAPPEDWINDOW`
   HWND, call `plugin_gui.set_parent(Window::from_win32_hwnd(hwnd))`,
   then `set_size` and `show`.
3. The `PluginHostWindow` is stored in the `ClapPluginHandle` and
   `Drop`-destroyed in `close_editor()`, which also calls
   `plugin_gui.destroy()`.

### Why a top-level HWND instead of an eframe child

- eframe 0.34 on winit 0.30 doesn't expose a stable Win32 HWND API for
  reparenting external windows inside the egui viewport.
- A separate top-level window is simpler, has correct focus handling,
  and matches what most CLAP hosts do.
- The eframe process keeps pumping Win32 messages, so the plugin's
  child HWND receives `WM_PAINT` and `WM_SIZE` naturally.

### Future work

- macOS: `PluginHostWindow` for NSView (NSWindow with contentView).
  Probably 50-100 LOC using objc2 or cocoa-foundation.
- Linux X11: pass the eframe `Window`'s X11 handle as the parent
  (raw-window-handle 0.6 already has the conversion in
  `clack_extensions::gui::Window::from_window_handle`).
- Wayland: no embedded support in CLAP spec — must use floating
  mode. CLAP's `is_floating=true` is mandatory.
- In-egui embedding: defer until eframe 0.35+ exposes a stable
  `raw-window-handle` API on `Frame`.

---

## Phase 6 — VST3 Support & Polish (~3-5 days)

- Add VST3 format behind existing `HostedPlugin` trait
- Latency compensation (delay master by max plugin latency)
- Plugin bypass
- Offline render integration (`WavRenderer` must process plugins)
- Graceful missing-plugin handling (placeholder + warning)

---

## File Layout

```
src/audio/plugins/
    mod.rs            — HostedPlugin trait, AudioBus, PluginEvents, PluginDescriptor
    discovery.rs      — scan standard CLAP dirs + user paths, probe plugins
    clap_plugin.rs    — ClapPluginHandle (main thread) + ClapPluginProcessor (audio thread)
    vst3_plugin.rs    — (Phase 6) Vst3PluginHandle + Vst3PluginProcessor
    library.rs        — PluginLibrary (in-memory metadata cache)
```

## Effort Estimate

| Phase | Description | Est. |
|-------|-------------|------|
| 1 | Plugin Abstraction & Discovery | ~3-4 days |
| 2 | Send Bus CLAP FX | ~2-3 days |
| 3 | Plugin Instruments | ~5-7 days |
| 4 | Parameter Automation | ~2-3 days |
| 5 | Plugin Editor UI | ~3-5 days |
| 6 | VST3 + Polish | ~3-5 days |
| **Total** | | **~18-27 days** |

## Risk Areas

1. **CLAP lifecycle threading** — `PluginInstance` is `!Send`; the `StartedAudioProcessor` is `Send` but `!Sync`. Must carefully pass the processor from main thread to audio thread via `AudioCommand`.
2. **Real-time safety** — `process()` must not allocate. All buffers pre-allocated in `activate()`. Parameter changes via SPSC queue only.
3. **Plugin crashes** — A buggy plugin must not bring down htrk. Consider subprocess isolation for discovery/probe; in-process processing is riskier.
4. **Editor embedding** — Platform-specific window reparenting. Wayland cannot embed (must use floating window). macOS needs NSView, not NSWindow.
5. **VST3 licensing** — Use `vst3` crate (MIT), NOT `vst3-sys` (GPLv3), to avoid license contamination.

## References

- CLAP spec: https://github.com/free-audio/clap
- clack (Rust CLAP bindings): https://github.com/prokopyl/clack
- VST3 Rust bindings: https://github.com/coupler-rs/vst3-rs
- raw-window-handle: https://github.com/rust-windowing/raw-window-handle
