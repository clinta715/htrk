# htrk Refactoring & Decoupling Plan

This document outlines a strategic plan to address technical debt in `sequencer_engine.rs` and `app.rs`, decouple high-level logic from the UI, and improve the testing infrastructure.

## 1. Decomposing `sequencer_engine.rs` (The Sequencer)

**Problem:** A 5,000+ line file that mixes row advancement, tick-level timing, and format-specific effect logic (IT/XM/S3M/MOD).
**Why:** It's a maintenance nightmare. A bug fix in XM volume slides shouldn't risk breaking IT portamentos.

### Actions:
- **Introduce `EffectHandler` Trait:**
  ```rust
  trait EffectHandler {
      fn handle_tick(&self, voice: &mut Voice, state: &mut ChannelState, tick: u32);
      fn handle_new_note(&self, voice: &mut Voice, state: &mut ChannelState, cell: &Cell);
  }
  ```
- **Create Format-Specific Modules:** Move logic into `src/audio/effects/it.rs`, `xm.rs`, etc.
- **Extract State Management:** Move `SequencerState` and row/tick advancement logic into a dedicated `SequencerClock` struct.
- **Voice Manager:** Extract voice allocation and envelope processing into a `VoicePool` manager to keep the engine focused on *sequencing* rather than *rendering*.

## 2. Slimming Down `app.rs` (The UI God Object)

**Problem:** `HtrkApp` is a 3,600+ line "God Object" handling everything from keyboard inputs to file coordination.
**Why:** It violates the Single Responsibility Principle and makes the UI logic impossible to unit test.

### Actions:
- **Introduce Sub-Controllers:** Break `HtrkApp` into smaller logical units:
  - `InputController`: Handles keyboard/mouse event mapping to commands.
  - `ProjectController`: Manages module loading, saving, and "dirty" state.
  - `ViewManager`: Handles the logic for switching between Pattern, Sample, and Instrument views.
- **Componentize Widgets:** Ensure each file in `src/ui/` is a self-contained component that receives only the data it needs to render, rather than a reference to the entire `HtrkApp`.

## 3. Decoupling High-Level Logic (Headless Mode)

**Problem:** The UI thread owns the `Module` and high-level editing logic.
**Why:** Prevents CLI rendering (e.g., `htrk-cli render song.it -o song.wav`) and automated playback verification.

### Actions:
- **Create `HtrkCore`:** A non-UI struct that owns the `Module`, `UndoManager`, and `CommandSender`.
- **Logic Lift:** Move "high-level" operations (like `delete_row` or `paste_selection`) out of the UI widgets and into `HtrkCore`.
- **The UI as a Shell:** `HtrkApp` should simply hold an instance of `HtrkCore` and call into it. This allows `HtrkCore` to be instantiated in a CLI tool without any GUI overhead.

## 4. Fixing the Testing Infrastructure

**Problem:** Tests only cover file format round-trips, not playback correctness.
**Why:** Tracker playback is notoriously "fiddly." Changes to the engine often introduce subtle regressions in effect behavior.

### Actions:
- **Regression Snapshots:** Implement a test harness that runs the `SequencerEngine` for X ticks and exports a hash of the mixed audio output or the final voice states.
- **Unit Testing Effects:** With the new `EffectHandler` trait, individual effects can be unit-tested in isolation without spinning up a full engine.
- **Headless Integration Tests:** Use `HtrkCore` to load a module, simulate a "play" command, and verify that the `AtomicPlaybackState` updates correctly over time.

## Benefits Summary
- **Developer Velocity:** Smaller files are faster to navigate and safer to edit.
- **Stability:** Isolated effect logic prevents cross-format regressions.
- **Extensibility:** Adding new features (like MIDI clock sync or VST support) becomes a matter of adding new controllers/handlers rather than bloating existing ones.
- **Portability:** Headless mode enables server-side rendering and CI-driven audio verification.
