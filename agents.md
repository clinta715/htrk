# Agent Guidelines for htrk

To prevent regressions in the sequencer, audio engine, and effect processing pipeline, follow these rules when modifying playback or format logic:

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
- **Mandate**: Never use `Arc::get_mut()` directly on `self.module` without first ensuring unique ownership, and always sync to the audio engine after mutation.
- **Rule**: Always call `self.ensure_module_ownership()` before any `Arc::get_mut()` call on the module. Always call `self.sync_module_to_audio()` after mutation. `ensure_module_ownership()` only clones the module if needed (does NOT send to audio engine). `sync_module_to_audio()` sends `LoadModule` to the audio engine with the current (mutated) Arc.
- **Pattern**:
  ```rust
  self.ensure_module_ownership();
  if let Some(ref mut module) = self.module {
      if let Some(arc_module) = Arc::get_mut(module) {
          // mutate arc_module safely
      }
  }
  self.sync_module_to_audio();
  ```
- **Why**: The audio engine always holds a clone of `Arc<Module>`, so `Arc::get_mut()` returns `None` unless ownership is ensured first. After mutation, the audio engine must be updated with `sync_module_to_audio()` so it picks up the changes.

## 6. Effect Architecture: Universal, Format-Specific, and Native

### Layer 1 — Universal Effects
The `Effect` enum (`src/sequencer/effect.rs`) defines the universal internal representation shared across all formats. These are format-agnostic commands (Arpeggio, VolumeSlide, TonePortamento, etc.) that every format loader decodes INTO and every format encoder decodes FROM.

### Layer 2 — Format-Specific Effects
When a legacy format has commands or quirks that cannot be cleanly represented in the universal `Effect` enum, use `Effect::FormatSpecific(FormatEffect)` with the appropriate sub-enum (`XmEffect`, `ModEffect`, `S3mEffect`, `ItEffect`). This isolates format quirks:
- A MOD-specific effect must never interfere with XM playback and vice versa.
- The sequencer dispatches format-specific effects only when the matching format is active.
- Each format's loader is responsible for encoding its native commands into the correct universal OR format-specific effect.

### Layer 3 — Native Effects (HTRK Superset)
Beyond the universal and format-compatibility layers, HTRK provides a superset of native commands that users composing in HTRK (`FormatType::Htk`) can treat as the standard toolkit. These include extended filter control, fine-grained MIDI parameter automation, and other modern features that have no legacy equivalent. Native effects should:
- Be available regardless of which legacy format is loaded (they enhance, not replace).
- Never conflict with or override format-specific compatibility behavior.
- Serve as the recommended command set for users whose interest is native HTRK composition rather than accurate legacy reproduction.

### Format-Conditional Processing Rules
When the sequencer processes a universal effect that needs format-specific behavior (e.g., vibrato uses period-domain math for MOD but frequency-domain for XM), use `module.flags.linear_slides` or `self.use_xm_model` to branch:
- Each branch MUST be tested independently.
- Changes to the non-XM path (`!linear_slides`) MUST NOT alter the XM path (`linear_slides`).
- Changes to the XM path MUST NOT alter the non-XM path.
- After modifying any format-conditional code, run the FULL test suite and verify both XM and MOD/S3M playback.

## 7. File-Browser Columns and Preview

### Column Layout (List / Details View)
- **Mandate**: Detail columns (duration, type, size, modified) must use fixed-width cells with truncation to prevent overlapping text.
- **Rule**: Always use `ui.add_sized([width, 14.0], Label::new(...).truncate())` (or a `detail_cell` helper) instead of `ui.set_width()` + `right_to_left` layout for right-aligned columns. The first column (name) uses `selectable_label` and fills remaining width.
- **Widths**: Dur=56px, Type=44px, Size=64px, Modified=76px (List view); same widths are used for Details view header + data rows.
- **Why**: `right_to_left` + `set_width` does not clip text when filenames are long; fixed-size `add_sized` enforces clipping.

### Audio Preview in File Browser
- **Architecture**: `AudioCommand::PreviewBuffer` carries raw PCM (`Arc<Vec<f32>>`) + sample rate. The audio engine's `trigger_preview_buffer` method decodes via `compute_playback_frequency` into the preview voice (index 255), same path as `trigger_preview_note`.
- **Caching**: `FileBrowser.get_preview_data()` stores `Option<(PathBuf, Arc<Vec<f32>>, u32)>` in `self.preview_sample`. WAV is decoded once per path via `formats::wav::import_wav`. Cache is cleared on `refresh()` (navigation).
- **Wiring**: `HtrkApp::preview_browser_sample(note_key)` checks `self.file_browser.show && mode == Samples`, loads the selected entry, decodes via `get_preview_data`, sends `PreviewBuffer`. Keyboard handler calls this before `preview_note` in `handle_text_input`; if it returns `true`, note recording is skipped. The Preview button in the browser footer sets `preview_requested` flag, handled after render.
- **Verification**: No existing audio tests should break; `trigger_preview_buffer` shares the same voice-pool infrastructure as `trigger_preview_note`.
