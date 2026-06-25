# Phase 3 — Plugin Instruments Design

## Overview

Allow any instrument slot in the module to be backed by a CLAP plugin instead of
sample-based voices. When a note-on hits a channel whose instrument has a
`plugin` set, the sequencer queues a MIDI note event for that plugin rather
than allocating a voice. The plugin processor runs in the audio callback after
voice mixing and before send bus processing.

## Scope

This phase covers:
1. Data model: `plugin` field on `Instrument`, `midi_base_channel`
2. Sequencer: queue note-on/off events instead of voice allocation
3. Audio engine: per-instrument plugin processors, note event dispatch
4. Lifecycle: main-thread handles + AudioCommand swap (same pattern as send bus)
5. UI: plugin slot selector in Instrument tab
6. Save/load: PluginSlot serialized on Instrument, auto-reload on .htk open

Explicitly deferred to Phase 4:
- Parameter macros (volume column / effect column → plugin param mapping)
- Automation lane support for plugin params

## 1. Data Model

### `src/sequencer/instrument.rs`

Add two fields to `Instrument`:

```rust
/// Optional plugin backing. `None` = traditional sample instrument.
#[serde(default)]
pub plugin: Option<PluginSlot>,

/// Base MIDI channel for multi-timbral routing (0–15).
/// When multiple sequencer channels use the same plugin instrument,
/// they are distinguished by `midi_base_channel + channel_index`.
/// Only meaningful when `plugin` is `Some`.
#[serde(default = "default_midi_channel")]
pub midi_base_channel: u8,

fn default_midi_channel() -> u8 { 0 }
```

`PluginSlot` already exists in `src/sequencer/plugin.rs` with serde derives —
no new types needed.

### Default

`Instrument::default()` keeps `plugin: None` and `midi_base_channel: 0`, so
existing (blank) instruments remain sample-based. No migration needed for `.htk`
files.

## 2. Sequencer Changes

### New type

In `src/audio/sequencer_engine/mod.rs` (or a separate `plugin_events.rs`):

```rust
pub struct PluginNoteEvent {
    pub instrument_idx: u8,
    pub midi_channel: u8,
    pub key: u8,
    pub velocity: u8,
    pub note_on: bool,
}
```

Add field to `SequencerEngine`:

```rust
pub pending_plugin_note_events: Vec<PluginNoteEvent>,
```

Initialise in `SequencerEngine::new()` as `Vec::new()`. Reset on row advance
(cleared after drain in audio callback, but init to empty for safety).

### `process_cell_unified` (cell.rs)

In the `Note::On(key)` branch (`cell.rs:139`), check whether the instrument has
a plugin:

```rust
// Current voice path (when no plugin):
if is_tone_portamento {
    self.with_processor_mut(|p, e| p.setup_portamento(...));
} else {
    self.with_processor_mut(|p, e| p.trigger_note(...));
}
```

becomes:

```rust
let has_plugin = instrument_idx > 0
    && instrument_idx < module.instruments.len()
    && module.instruments[instrument_idx].plugin.is_some();

if has_plugin {
    let inst = &module.instruments[instrument_idx];
    let midi_ch = inst.midi_base_channel + (channel as u8).min(15 - inst.midi_base_channel);
    let velocity = 100; // default velocity; can be refined later
    self.pending_plugin_note_events.push(PluginNoteEvent {
        instrument_idx: instrument_idx as u8,
        midi_channel: midi_ch,
        key,
        velocity,
        note_on: true,
    });
} else if is_tone_portamento {
    // existing portamento path
    ...
} else {
    // existing trigger_note path
    ...
}
```

For `Note::Off`:

```rust
if has_plugin {
    self.pending_plugin_note_events.push(PluginNoteEvent {
        instrument_idx: instrument_idx as u8,
        midi_channel: midi_ch,
        key: 0,  // key is not meaningful for note-off from a cell
        velocity: 0,
        note_on: false,
    });
} else {
    // existing note_off handler
}
```

### Collecting events

Add a drain method parallel to `collect_plugin_param_automation`:

```rust
pub fn collect_plugin_note_events(&mut self) -> Vec<PluginNoteEvent> {
    std::mem::take(&mut self.pending_plugin_note_events)
}
```

The audio engine calls this after `process_tick()` in `process_callback`.

## 3. Audio Engine Changes

### `AudioEngine` (engine.rs)

New field:

```rust
instrument_plugin_processors: Vec<Option<Box<dyn HostedPluginProcessor>>>,
```

Initialised empty in `create_engine_and_sender`. On `LoadModule`, resized to
`module.instruments.len()` with `None` entries.

### New AudioCommand variants (commands.rs)

```rust
InstallInstrumentPlugin {
    instrument_idx: usize,
    processor: Option<Box<dyn HostedPluginProcessor>>,
},
```

### Command handling (engine.rs)

```rust
AudioCommand::InstallInstrumentPlugin { instrument_idx, processor } => {
    if instrument_idx < self.instrument_plugin_processors.len() {
        self.instrument_plugin_processors[instrument_idx] = processor;
    }
}
```

### process_callback integration

After voice mixing ends (after the `} else { ... }` block for stopped mode),
and before send bus processing:

```rust
// ── Instrument plugin processing ──
if self.sequencer.state.playing {
    let events = self.sequencer.collect_plugin_note_events();
    // Group events by instrument_idx
    // For each instrument with a processor:
    //   Push note events into the processor's input event buffer
    //   Call processor.process(silent_input, plugin_out, events, transport)
    //   Mix plugin_out into ch_mix for the channels using this instrument
}
```

### Note event injection into CLAP processors

The `ClapPluginProcessor::process()` signature needs to accept note events.
Two options:

**Option A (recommended — simpler):** Add a new method on
`HostedPluginProcessor` trait:

```rust
fn send_note_on(&mut self, midi_channel: u8, key: u8, velocity: u8);
fn send_note_off(&mut self, midi_channel: u8, key: u8);
```

These queue events in the processor's internal buffer. On the next `process()`,
the events are drained into the clack input event buffer before calling
`started.process(...)`. This avoids changing the trait's `process()` signature,
keeping the send-bus code path unchanged.

**Option B:** Extend `process()` with an `&[PluginNoteEvent]` parameter. This
requires changing all call sites, including the send-bus path where the slice
is empty.

We go with **Option A** because it's additive (existing call sites unchanged)
and keeps the note event queue close to the param ring buffer pattern already
present.

### Transport info for instrument plugins

For instrument plugins, the input audio is silence (mono or stereo silent
buffers). The `process()` method is called with silent input buffers of the
correct length.

## 4. Lifecycle Management

### Main thread

`HtrkApp` gains:

```rust
instrument_plugin_handles: Vec<Option<Box<dyn HostedPluginHandle>>>,
```

Initialised empty. On module load, resized to `module.instruments.len()` with
`None`. On module unload, all handles are dropped (triggering plugin
deactivation via `Drop`).

### Assigning a plugin to an instrument

Same pattern as `load_and_install_plugin()` in `src/ui/plugin_browser.rs`:

1. User picks plugin in the Instrument tab plugin browser dialog
2. `ClapPluginHandle::load(path)`
3. `handle.activate(sample_rate, max_block)` → returns processor
4. Send `AudioCommand::InstallInstrumentPlugin { instrument_idx, processor }`
5. Store handle in `instrument_plugin_handles[instrument_idx]`

### Removing a plugin

1. Send `AudioCommand::InstallInstrumentPlugin { instrument_idx, processor: None }`
2. Audio thread returns the old processor via the `None` swap (its Drop runs on
   the audio thread — same constraint as send bus plugins)
3. Main thread deactivates the handle: `handle.deactivate(...)` — but since the
   processor is already dropped on the audio thread, we construct a minimal
   stopped processor for deactivation (or skip the `stop()` handshake and let
   Drop handle it, same as send bus removal).

### On module load (.htk open)

After `LoadModule`, iterate over `module.instruments`. For each instrument with
`plugin.is_some()`, call a helper `load_instrument_plugin` that does the same
`load → activate → send processor → store handle` sequence. If a plugin can't
be loaded (missing DLL, wrong path, etc.), log a warning and leave the
instrument as a sample instrument (plugin slot stays in the data model for
re-saving, but no processor is active).

### On module close / new song

All `instrument_plugin_handles` entries are dropped, which drops each
`ClapPluginHandle`. The handle's Drop sends a `RemoveInstrumentPlugin`-style
command if `activated` is true. Since we can't send AudioCommands in Drop
(no CommandSender available), we instead replace the processor with `None`
before dropping the handle:

```rust
for idx in 0..self.instrument_plugin_handles.len() {
    if self.instrument_plugin_handles[idx].is_some() {
        if let Some(ref mut sender) = self.core.command_sender {
            sender.send(AudioCommand::InstallInstrumentPlugin {
                instrument_idx: idx,
                processor: None,
            });
        }
        self.instrument_plugin_handles[idx] = None;
    }
}
```

## 5. UI (Instrument Tab)

### Plugin slot assignment

In the Instrument tab (`src/ui/instrument_view.rs`), add a section in the
instrument header area. Display:

```
Plugin: [None                  ] [Browse...]
```

When `None`: clicking Browse opens the plugin browser dialog (reuse
`src/ui/plugin_browser.rs`). When a plugin is set: show the plugin name
(from the descriptor), with a "Remove" button.

The UI calls `load_and_install_plugin(...)` adapted for instruments
(`load_and_install_instrument_plugin`) or `remove_instrument_plugin(idx)`.

### MIDI channel display

Show `MIDI ch: {midi_base_channel + 1}` read-only in the instrument detail area
when a plugin is assigned. This is a derived display, not an editable field for
Phase 3 (the base channel is set on the data model; multi-timbral routing via
channel index is automatic).

## 6. Save/Load

No special handling needed beyond what serde already gives us. `Instrument` has
`#[derive(Serialize, Deserialize)]`, and `PluginSlot` does too. On save to
`.htk`, the plugin field is serialised. On load, it's deserialised; the
application code then calls `load_instrument_plugins()` to re-activate.

The state blob in `PluginSlot` is saved/restored via the CLAP state extension.
`ClapPluginHandle::save_state()` currently returns `Vec::new()` (stub). We
implement it properly as part of this phase: call the plugin's `state.save`
extension and cache the result in `self.saved_state: Vec<u8>`. On `load_state`,
restore from the blob.

## 7. Testing

### Unit tests

1. **`test_plugin_instrument_queues_note_on`** — Create sequencer with a module
   where an instrument has `plugin = Some(PluginSlot::new(...))`. Call
   `process_cell_unified` with a `Note::On`. Verify `pending_plugin_note_events`
   contains the expected event.

2. **`test_plugin_instrument_queues_note_off`** — Same setup with `Note::Off`.
   Verify `note_on: false` event.

3. **`test_sample_instrument_unaffected`** — Verify that when `plugin` is
   `None`, the existing voice allocation path is used and
   `pending_plugin_note_events` is empty.

4. **`test_plugin_instrument_collect_drains`** — Queue an event, call
   `collect_plugin_note_events`, verify returned vec has it and internal vec is
   empty.

5. **`test_audio_command_install`** — Send `AudioCommand::InstallInstrumentPlugin`
   with a real (bit-crusher test) CLAP processor, verify it's callable and the
   processor slot is populated.

### Integration tests (existing)

- Load a module with plugin instruments, play, verify no crash
- Verify existing sample-based tests still pass unchanged

## 8. File Changes Summary

| File | Change |
|------|--------|
| `src/sequencer/instrument.rs` | Add `plugin: Option<PluginSlot>`, `midi_base_channel: u8` fields |
| `src/audio/sequencer_engine/mod.rs` | Add `PluginNoteEvent` struct, `pending_plugin_note_events` field, `collect_plugin_note_events()` |
| `src/audio/sequencer_engine/cell.rs` | Branch on `plugin.is_some()` in `process_cell_unified` |
| `src/audio/engine.rs` | Add `instrument_plugin_processors`, handle `InstallInstrumentPlugin` cmd, process in callback |
| `src/audio/commands.rs` | Add `InstallInstrumentPlugin` variant |
| `src/audio/plugins/mod.rs` | Add `send_note_on()`, `send_note_off()` to `HostedPluginProcessor` trait |
| `src/audio/plugins/clap_plugin.rs` | Implement `send_note_on/off` on `ClapPluginProcessor`, implement `save_state/load_state` |
| `src/app.rs` | Add `instrument_plugin_handles` field, load/unload lifecycle |
| `src/ui/instrument_view.rs` | Add plugin slot selector and MIDI ch display |
| `src/ui/plugin_browser.rs` | Add `load_and_install_instrument_plugin()` helper, or parameterise existing |

## 9. Constraints

- No changes to the send bus plugin code path (it uses `process(audio, ...)`
  without note events and must continue to work).
- The `HostedPluginProcessor` trait's `process()` signature is NOT changed.
  Note events are queued via separate methods.
- All 351 existing tests continue to pass.
- Instrument plugin audio input is silence (mono or stereo). The processor
  generates audio from note events.
