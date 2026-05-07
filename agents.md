# Agent Guidelines for htrk

To prevent regressions in the sequencer and audio engine, follow these rules when modifying playback logic:

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
- **Mandate**: Never use `Arc::get_mut()` directly on `self.module` without first ensuring unique ownership.
- **Rule**: Always call `self.ensure_module_ownership()` before any `Arc::get_mut()` call on the module. This method checks `Arc::strong_count()` and, if > 1, clones the module, creates a new Arc, and sends `LoadModule` to the audio engine.
- **Pattern**:
  ```rust
  self.ensure_module_ownership();
  if let Some(ref mut module) = self.module {
      if let Some(arc_module) = Arc::get_mut(module) {
          // mutate arc_module safely
      }
  }
  ```
- **Why**: The audio engine always holds a clone of `Arc<Module>`, so `Arc::get_mut()` returns `None` unless ownership is ensured first.
