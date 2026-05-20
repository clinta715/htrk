# htrk Refactoring: Detailed Phase-by-Phase Development Plan

This plan breaks down the refactoring of **htrk** into five logical phases. Each phase is designed to be incremental, allowing the application to remain functional (or near-functional) throughout the process.

---

## Phase 1: The Core Foundation (Decoupling Logic)
**Goal:** Extract the "brain" of the application from the UI loop.

1.  **Define `HtrkCore`:** Create `src/core/mod.rs` and move the `Module`, `UndoManager`, and `CommandSender` into a new `HtrkCore` struct.
2.  **Lift Editing Logic:** Move high-level editing functions (e.g., `set_cell`, `insert_row`, `copy_selection`) from `app.rs` and UI widgets into `HtrkCore`.
3.  **Refactor `HtrkApp`:** Update `HtrkApp` to hold a `HtrkCore` instance. Redirect all menu and shortcut actions to call `HtrkCore` methods.
4.  **Milestone:** A "headless" test script that can load a module and perform a series of edits without initializing `eframe`.

## Phase 2: Sequencer Decomposition (Effect Trait System)
**Goal:** Break the 5,000-line `sequencer_engine.rs` into format-specific handlers.

1.  **Define `EffectHandler` Trait:** Create the trait in `src/audio/effects/mod.rs`.
2.  **Extract Clock Logic:** Move BPM, Speed, and Tick calculation into `src/audio/sequencer/clock.rs`.
3.  **Implement Handlers:** 
    - Move IT effects to `src/audio/effects/it_handler.rs`.
    - Move XM effects to `src/audio/effects/xm_handler.rs`.
    - Repeat for S3M and MOD.
4.  **Refactor `SequencerEngine`:** Replace the massive `match` statements in the tick loop with a call to the active `EffectHandler`.
5.  **Milestone:** Playback works identically for all formats, but with the logic cleanly separated into four files.

## Phase 3: UI Modularization (Controller Pattern)
**Goal:** Slim down `app.rs` by delegating responsibilities.

1.  **Input Controller:** Create `src/ui/controllers/input.rs` to handle keyboard mapping and "tracker-style" navigation logic.
2.  **Project Controller:** Create `src/ui/controllers/project.rs` to handle file dialogs, auto-save timers, and "Save/Save As" logic.
3.  **View Controller:** Create `src/ui/controllers/view.rs` to manage state for the different editors (Pattern, Sample, etc.).
4.  **Widget Isolation:** Refactor `PatternGrid` and `SampleEditor` to take a `&mut HtrkCore` instead of a `&mut HtrkApp`.
5.  **Milestone:** `app.rs` is reduced to < 500 lines, acting only as the top-level layout orchestrator.

## Phase 4: Advanced Testing & Regression Harness
**Goal:** Ensure playback stability through automation.

1.  **Headless Renderer:** Create a small utility (or `bin/render`) that uses `SequencerEngine` and `HtrkCore` to render a module to a WAV file without a GUI.
2.  **State Snapshots:** Implement a system to dump the state of all 64 channels (note, volume, period) at specific rows into a JSON format.
3.  **Regression Suite:** Create a suite of "Reference Modules" (one for each format) and write tests that compare current state snapshots against known-good baselines.
4.  **Milestone:** CI fails if a change to the `SequencerEngine` alters the output of any reference module.

## Phase 5: Optimization & Final Polish
**Goal:** Re-verify performance and clean up remaining debt.

1.  **Performance Audit:** Use a profiler (like `flamegraph` or `tracy`) to ensure the new trait-based effect system hasn't introduced significant overhead in the audio callback.
2.  **Inlining & Devirtualization:** Use Rust's `#[inline]` or static dispatch where appropriate to ensure the audio thread remains efficient.
3.  **Documentation:** Update `docs/architecture.md` and `GEMINI.md` to reflect the new modular structure.
4.  **Milestone:** Application achieves stable 60 FPS UI and < 5% CPU usage during complex IT module playback.

---

## Risk Assessment & Mitigation
- **Risk:** Audio thread performance hit due to trait dispatch.
  - **Mitigation:** Use an Enum of handlers rather than `Box<dyn EffectHandler>` to keep dispatch static and fast.
- **Risk:** UI/Logic sync issues.
  - **Mitigation:** Ensure `HtrkCore` remains the "Source of Truth" and the UI only reacts to its state.
- **Risk:** Regression in obscure tracker effects.
  - **Mitigation:** Phase 4 (Testing) is critical. Do not start Phase 5 until Phase 4's regression suite is solid.
