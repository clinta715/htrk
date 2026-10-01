# htrk Application Summary & Critique

## Overview
**htrk** is a modern music tracker built in Rust. It follows the classic pattern-based sequencing workflow familiar to users of Impulse Tracker (IT), FastTracker 2 (XM), and ScreamTracker 3 (S3M). It is designed to be a performant, cross-platform tool for composing and playing back module files.

## Technical Stack
- **Language:** Rust
- **UI Framework:** `egui` / `eframe` (immediate mode GUI)
- **Audio Backend:** `cpal`
- **Concurrency:** `ringbuf` / `rtrb` lock-free SPSC ring buffers for UI-to-audio thread communication
- **Plugin hosting:** `clack-host` / `clack-extensions` (CLAP)
- **MIDI:** `midir` (input), `midly` (Standard MIDI File import)
- **Persistence:** `serde` + `toml` / `serde_json` / `bincode`
- **Automation surface:** built-in MCP server (TCP JSON-RPC + HTTP/SSE transport)

## Architecture

### 1. Thread Separation
The application uses a strict two-thread architecture to ensure audio stability:
- **UI Thread:** Handles user input, state management, undo/redo history, and rendering. It owns the primary `Module` data.
- **Audio Thread:** Runs at real-time priority. It is responsible for sequencer logic, voice management, plugin processing, and mixing. It is designed to be lock-free and allocation-free during its callback.

### 2. Data Flow
- **UI to Audio:** Commands (`Play`, `Stop`, `LoadModule`, `SetCell`, ...) are sent via a lock-free ring buffer.
- **Audio to UI:** Playback state (current row, order, BPM, ...) is shared via atomics in `Arc<AtomicPlaybackState>`.
- **Module Sharing:** `Module` data is shared using `Arc`. Structural edits deep-clone via `HtrkCore::ensure_module_ownership()` and are pushed to the audio thread with `sync_module_to_audio()`; small live edits use targeted commands.

### 3. Core Modules
- `src/sequencer/`: Data model (`Module`, `Pattern`, `Instrument`, `Sample`, `Note`, `Effect`).
- `src/audio/`: Audio engine, mixer, voice pool, resamplers, send FX, and the sequencer engine (split across `sequencer_engine/{mod,advance,cell,period,helpers}.rs`), plus `effects/` (XM vs legacy processors) and `plugins/` (CLAP hosting, discovery, preset library).
- `src/formats/`: Parsers/writers for IT, XM, S3M, MOD, 669, DTM, MMD, STM, ULT, native HTK/HTI, WAV, and Standard MIDI File import.
- `src/ui/`: Widgets and windows for the tracker interface.
- `src/edit/`: `UndoManager` and the `EditCommand` stack.
- `src/mcp/`: MCP server, read-only tools/resources, and per-domain mutation handlers.

## Current Metrics (v0.27.0)
- ~54.5k lines of Rust under `src/`, plus `tests/format_conversions.rs` and `tests/mcp_integration.rs`.
- **417 unit tests + 3 format-conversion tests + 1 golden-render test + 6 MCP integration tests**, all passing.
- Largest source files:

  | File | Lines |
  |------|------:|
  | `src/audio/sequencer_engine/tests.rs` | 1821 |
  | `src/formats/xm.rs` | 1571 |
  | `src/ui/pattern_grid.rs` | 1523 |
  | `src/audio/engine.rs` | 1442 |
  | `src/actions/keyboard.rs` | 1348 |
  | `src/formats/s3m.rs` | 1250 |

## Critiques

### 1. Mega-files
All three former mega-files have been split: `sequencer_engine.rs` into `audio/sequencer_engine/`, `app.rs` (was 3,238 lines) into `app/{mod,audio_setup,editing,preamble,dialogs,panels,plugins,lifecycle,eframe_impl}.rs`, and `clap_plugin.rs` (was 1,850 lines) into `clap_plugin/{mod,host,handle,processor,tests}.rs`. No file now exceeds ~1,800 lines and the two largest entries are tests.
- **Recommendation:** The format loaders (`xm.rs` 1571, `s3m.rs` 1250) and `ui/pattern_grid.rs` (1523) are the next files large enough to benefit from internal splitting.

### 2. UI/Logic Coupling
The UI thread owns the `Module` and a large share of the high-level logic, which makes headless rendering and automated testing harder than it needs to be.
- **Recommendation:** Keep pushing module manipulation into `HtrkCore` / `src/edit/commands.rs` so the headless path does not depend on the `eframe` app.

### 3. Audio Thread Safety & `Option<Arc<Module>>`
The audio thread defensively checks `self.module.as_ref()` throughout the sequencer engine.
- **Recommendation:** Consider an explicit "Idle / Loaded / Playing" state machine so the hot path can assume a loaded module.

### 4. Testing Gaps (narrowed)
The sequencer engine has ~50 dedicated tests covering row-state resets, note delay, envelope stages, format-conditional handling, and XM panning saturation. A bit-exact golden-output test now renders a deterministic module through the full offline pipeline (`WavRenderer` → sequencer + mixer + limiter + WAV) and pins an FNV-1a hash of the result; it is verified to fail when the mixer changes.
- **Recommendation:** Add further golden cases that isolate individual legacy formats (XM vs MOD linear/non-linear, IT NNA, S3M effects) and short effect-dense patterns.

### 5. Tooling and Process Gaps
CI (`.github/workflows/ci.yml`) now runs build, tests, a blocking `clippy -D clippy::correctness` gate, and a blocking `cargo fmt --check`, on Windows. The tree has been formatted with rustfmt (`rustfmt.toml`) and `.gitattributes`/`clippy.toml` are in place. A backlog of ~94 non-machine-applicable stylistic clippy lints remains (`field_reassign_with_default`, `needless_range_loop`, `collapsible_if`).
- **Recommendation:** Work down that stylistic backlog opportunistically; correctness is already gated.

### 6. Hand-rolled Unsafe Ring Buffer (resolved)
`param_ring.rs` now uses `rtrb` instead of a custom `unsafe impl Send/Sync` ring with raw-pointer slot access. The old design also had two unsynchronised producers; the main thread now owns an `rtrb::Producer`, the audio thread owns the `Consumer`, and audio-thread-originated changes use a local queue.

## Future Work
- VST3 hosting alongside CLAP.
- Plugin parameter envelopes (sustain/release curves over time) — see `docs/parameter-extension-todo.md`.

## Conclusion
**htrk** is an ambitious and well-structured project that leverages Rust's strengths for real-time audio. The thread separation and use of lock-free primitives are excellent. All three former mega-files have been split, the custom `unsafe` parameter ring has been replaced with `rtrb`, a bit-exact golden-render test guards the audio pipeline, and CI now enforces build, tests, correctness lints, and formatting. The remaining work is smaller: a stylistic clippy backlog, and further golden coverage per legacy format.
