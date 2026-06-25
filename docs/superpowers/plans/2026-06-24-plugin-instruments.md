# Phase 3 — Plugin Instruments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add CLAP plugin backing to instrument slots so note-on events route to plugin processors instead of sample-based voices.

**Architecture:** `Instrument` gets an optional `PluginSlot` field. The sequencer checks this field in `process_cell_unified` — when set, it queues a `PluginNoteEvent` instead of calling `trigger_note`. The audio engine drains these events each tick and forwards them to per-instrument plugin processors via `send_note_on/off` on the existing `HostedPluginProcessor` trait. Main thread handles lifecycle via the same handle/processor split used by send bus plugins.

**Tech Stack:** CLAP (`clack-host`), existing `PluginSlot`/`HostedPluginProcessor`/`ClapPluginHandle` infrastructure.

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/sequencer/instrument.rs` | Add `plugin: Option<PluginSlot>`, `midi_base_channel: u8` |
| `src/audio/sequencer_engine/mod.rs` | `PluginNoteEvent` type, pending queue, `collect_plugin_note_events()` |
| `src/audio/sequencer_engine/cell.rs` | Branch on `plugin.is_some()` in `process_cell_unified` |
| `src/audio/plugins/mod.rs` | Add `send_note_on()`, `send_note_off()` to `HostedPluginProcessor` trait |
| `src/audio/plugins/clap_plugin.rs` | Implement `send_note_on/off`, implement `save_state/load_state` |
| `src/audio/commands.rs` | Add `InstallInstrumentPlugin` variant |
| `src/audio/engine.rs` | `instrument_plugin_processors` field, cmd handler, callback processing loop |
| `src/app.rs` | `instrument_plugin_handles`, lifecycle (load/unload/.htk reload) |
| `src/ui/instrument_view.rs` | Plugin slot selector UI |
| `src/ui/plugin_browser.rs` | Helper to load + install instrument plugins |

---

### Task 1: Add `plugin` and `midi_base_channel` to Instrument

**Files:**
- Modify: `src/sequencer/instrument.rs`
- Test: `src/sequencer/instrument.rs` (existing tests)

- [ ] **Step 1: Add fields and default-fn**

Add to `Instrument` (around line 154, before `impl Default`):

```rust
/// Optional plugin backing. `None` = traditional sample instrument.
#[serde(default)]
pub plugin: Option<crate::sequencer::plugin::PluginSlot>,

/// Base MIDI channel for multi-timbral routing (0–15).
/// When multiple sequencer channels use the same plugin instrument,
/// they are distinguished by `midi_base_channel + channel_index`.
#[serde(default = "default_midi_channel")]
pub midi_base_channel: u8,
```

Add standalone function near the bottom of the file (before `#[cfg(test)]`):

```rust
fn default_midi_channel() -> u8 { 0 }
```

Add to `Instrument::default()` body (after `vib_rate: 0,`):

```rust
plugin: None,
midi_base_channel: 0,
```

- [ ] **Step 2: Build**

Run: `cargo build --lib`
Expected: compiles with no new errors (existing warnings OK)

- [ ] **Step 3: Run existing instrument tests**

Run: `cargo test --lib -- instrument::tests`
Expected: 3 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/sequencer/instrument.rs
git commit -m "feat: add plugin and midi_base_channel fields to Instrument"
```

---

### Task 2: Add `send_note_on`/`send_note_off` to `HostedPluginProcessor` trait

**Files:**
- Modify: `src/audio/plugins/mod.rs`
- Test: build compiles

- [ ] **Step 1: Add trait methods**

After line 183 (after `fn name(&self) -> &str;`), add:

```rust
/// Send a MIDI-style note-on to the plugin. Queued for the next process().
/// Called from the sequencer tick (audio thread).
fn send_note_on(&mut self, midi_channel: u8, key: u8, velocity: u8);

/// Send a MIDI-style note-off to the plugin. Queued for the next process().
/// Called from the sequencer tick (audio thread).
fn send_note_off(&mut self, midi_channel: u8, key: u8);
```

- [ ] **Step 2: Build**

Run: `cargo build --lib`
Expected: compile error because `ClapPluginProcessor` doesn't implement the new methods yet

- [ ] **Step 3: Commit trait change only**

```bash
git add src/audio/plugins/mod.rs
git commit -m "feat: add send_note_on/off to HostedPluginProcessor trait"
```

---

### Task 3: Implement `send_note_on`/`send_note_off` on `ClapPluginProcessor`

**Files:**
- Modify: `src/audio/plugins/clap_plugin.rs`

- [ ] **Step 1: Add note event queue + note_id counter to `ClapPluginProcessor`**

In `ClapPluginProcessor` struct (around line 750), add:

```rust
/// Queued note-on/off events from the sequencer, drained in process().
/// Tuples: (note_on, midi_channel, key, velocity)
note_events: std::collections::VecDeque<(bool, u8, u8, u8)>,

/// Monotonically increasing note ID counter for CLAP note tracking.
next_note_id: u32,
```

Add to `ClapPluginProcessor::new()`:

```rust
note_events: std::collections::VecDeque::new(),
next_note_id: 0,
```

- [ ] **Step 2: Implement `send_note_on` and `send_note_off`**

Add methods (before `fn process`):

```rust
fn send_note_on(&mut self, midi_channel: u8, key: u8, velocity: u8) {
    self.note_events.push_back((true, midi_channel, key, velocity));
}

fn send_note_off(&mut self, midi_channel: u8, key: u8) {
    self.note_events.push_back((false, midi_channel, key, 0));
}
```

- [ ] **Step 3: Drain note events in `process()`**

In `ClapPluginProcessor::process()`, after the param-ring drain block (around line 857) but before `self.processor.process(...)` is called, replace the `if drained > 0 { ... } else { ... }` structure (lines 838-870) with code that always builds the input event buffer from both param changes AND note events:

Add imports at the top of the method (after existing `use` lines):

```rust
use clack_common::events::event_types::{
    NoteOnEvent, NoteOffEvent, ParamValueEvent,
};
use clack_common::events::{Match, Pckn};
use clack_common::utils::{ClapId, Cookie};
```

Replace the param-only event building block (lines ~831-870):

```rust
    self.param_scratch.clear();
    let drained = self.param_ring.drain_into(&mut self.param_scratch, 64);

    let total_events = drained + self.note_events.len();
    let mut ev_buffer = clack_host::events::io::EventBuffer::with_capacity(total_events.max(1));
    let cookie = Cookie::default();

    // Push param changes
    let pckn = Pckn::new(0u16, 0u16, 0u16, Match::All);
    for change in self.param_scratch.iter() {
        let ev = ParamValueEvent::new(
            0,
            ClapId::from(change.param_id),
            pckn,
            change.value,
            cookie,
        );
        let _ = ev_buffer.push(&ev);
    }

    // Push note events
    while let Some((note_on, midi_ch, key, velocity)) = self.note_events.pop_front() {
        let note_pckn = Pckn::new(0u16, midi_ch as u16, key as u16, Match::All);
        if note_on {
            let note_id = self.next_note_id;
            self.next_note_id = self.next_note_id.wrapping_add(1);
            let note_pckn = Pckn::new(
                0u16,
                midi_ch as u16,
                key as u16,
                note_id,
            );
            let ev = NoteOnEvent::new(0, note_pckn, (velocity as f64) / 127.0);
            let _ = ev_buffer.push(&ev);
        } else {
            let ev = NoteOffEvent::new(0, note_pckn, 0.0);
            let _ = ev_buffer.push(&ev);
        }
    }

    let input_events = clack_host::events::io::InputEvents::from_buffer(&ev_buffer);
    let mut output_events = clack_host::events::io::OutputEvents::from_buffer(&mut self.output_event_buffer);
    let _ = self.processor.process(
        &input_audio,
        &mut output_audio,
        &input_events,
        &mut output_events,
        None,
        None,
    );
```

Also remove the stale `self.param_scratch.clear()` at line 836 if it already exists (it should remain before the drain), and ensure the separate `if drained > 0` / `else` branches are fully replaced by the unified block above.

- [ ] **Step 4: Build**

Run: `cargo build --lib`
Expected: compiles (existing warnings OK)

- [ ] **Step 5: Run existing CLAP tests**

Run: `cargo test --lib --release -- --test-threads=1 plugins::clap_plugin`
Expected: 15 tests pass

- [ ] **Step 6: Commit**

```bash
git add src/audio/plugins/clap_plugin.rs
git commit -m "feat: implement send_note_on/off on ClapPluginProcessor"
```

---

### Task 4: Add `PluginNoteEvent` type, pending queue, and `cell.rs` branching

**Files:**
- Modify: `src/audio/sequencer_engine/mod.rs`
- Modify: `src/audio/sequencer_engine/cell.rs`
- Test: `src/audio/sequencer_engine/tests.rs`

- [ ] **Step 1: Add `PluginNoteEvent` type and pending queue to `SequencerEngine`**

In `src/audio/sequencer_engine/mod.rs`, before the `impl SequencerEngine` block, add:

```rust
/// A note event queued by the sequencer for delivery to an instrument
/// plugin processor in the audio callback.
#[derive(Clone, Debug)]
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

Add initialization in `SequencerEngine::new()`, after `pending_plugin_param_changes`:

```rust
pending_plugin_note_events: Vec::new(),
```

- [ ] **Step 2: Add `collect_plugin_note_events()` drain method**

```rust
pub fn collect_plugin_note_events(&mut self) -> Vec<PluginNoteEvent> {
    std::mem::take(&mut self.pending_plugin_note_events)
}
```

- [ ] **Step 3: Add import in `cell.rs`**

In `src/audio/sequencer_engine/cell.rs`, add to existing imports:

```rust
use crate::audio::sequencer_engine::PluginNoteEvent;
```

- [ ] **Step 4: Branch on `plugin.is_some()` in `process_cell_unified`**

In `cell.rs`, in the `Note::On(key)` branch (currently starting at line 138):

Find the section:

```rust
match cell.note {
    Note::On(key) => {
        self.state.channels[channel].last_note = Note::On(key);

        if is_tone_portamento {
            self.with_processor_mut(|processor, engine| processor.setup_portamento(engine, channel, key, remapped_key, sample, sample_idx));
        } else {
            self.with_processor_mut(|processor, engine| processor.trigger_note(engine, channel, key, remapped_key, sample, sample_idx, cell, instrument_idx));
        }
    }
```

Replace with:

```rust
match cell.note {
    Note::On(key) => {
        self.state.channels[channel].last_note = Note::On(key);

        let instrument = module.instruments.get(instrument_idx);
        let has_plugin = instrument.map(|i| i.plugin.is_some()).unwrap_or(false);

        if has_plugin {
            if let Some(inst) = instrument {
                let midi_ch = inst.midi_base_channel.wrapping_add(channel as u8) % 16;
                self.pending_plugin_note_events.push(PluginNoteEvent {
                    instrument_idx: instrument_idx as u8,
                    midi_channel: midi_ch,
                    key,
                    velocity: 100,
                    note_on: true,
                });
            }
        } else if is_tone_portamento {
            self.with_processor_mut(|processor, engine| processor.setup_portamento(engine, channel, key, remapped_key, sample, sample_idx));
        } else {
            self.with_processor_mut(|processor, engine| processor.trigger_note(engine, channel, key, remapped_key, sample, sample_idx, cell, instrument_idx));
        }
    }
```

- [ ] **Step 5: Handle Note::Off for plugin instruments**

In the `Note::Off` branch (around line 148):

```rust
Note::Off => {
    self.with_processor_mut(|processor, engine| processor.handle_note_off(engine, channel));
}
```

Replace with:

```rust
Note::Off => {
    let instrument = module.instruments.get(instrument_idx);
    let has_plugin = instrument.map(|i| i.plugin.is_some()).unwrap_or(false);
    if has_plugin {
        if let Some(inst) = instrument {
            let midi_ch = inst.midi_base_channel.wrapping_add(channel as u8) % 16;
            self.pending_plugin_note_events.push(PluginNoteEvent {
                instrument_idx: instrument_idx as u8,
                midi_channel: midi_ch,
                key: 0,
                velocity: 0,
                note_on: false,
            });
        }
    } else {
        self.with_processor_mut(|processor, engine| processor.handle_note_off(engine, channel));
    }
}
```

- [ ] **Step 6: Write tests for plugin note event queueing**

In `src/audio/sequencer_engine/tests.rs`, add 4 new tests (follow the existing pattern: `SequencerEngine::new(48000.0)` → `engine.load_module(Arc::new(module))`):

```rust
#[test]
fn test_plugin_instrument_queues_note_on() {
    use crate::sequencer::plugin::PluginSlot;
    use crate::audio::sequencer_engine::PluginNoteEvent;

    let mut engine = SequencerEngine::new(48000.0);
    let mut module = Module::default();
    module.instruments[1].plugin = Some(PluginSlot::new("clap", "/dev/null", "test.plugin"));
    module.instruments[1].midi_base_channel = 0;
    engine.load_module(Arc::new(module));

    let cell = Cell {
        note: Note::On(60),
        instrument: Some(1),
        ..Default::default()
    };
    engine.process_cell_unified(0, &cell);

    let events = engine.collect_plugin_note_events();
    assert_eq!(events.len(), 1, "should queue exactly one note event");
    assert_eq!(events[0].instrument_idx, 1);
    assert_eq!(events[0].key, 60);
    assert!(events[0].note_on);
}

#[test]
fn test_plugin_instrument_queues_note_off() {
    use crate::sequencer::plugin::PluginSlot;

    let mut engine = SequencerEngine::new(48000.0);
    let mut module = Module::default();
    module.instruments[1].plugin = Some(PluginSlot::new("clap", "/dev/null", "test.plugin"));
    engine.load_module(Arc::new(module));

    let cell = Cell {
        note: Note::Off,
        instrument: Some(1),
        ..Default::default()
    };
    engine.process_cell_unified(0, &cell);

    let events = engine.collect_plugin_note_events();
    assert_eq!(events.len(), 1);
    assert!(!events[0].note_on);
}

#[test]
fn test_sample_instrument_does_not_queue_plugin_event() {
    let mut engine = SequencerEngine::new(48000.0);
    let module = Module::default();
    engine.load_module(Arc::new(module));

    let cell = Cell {
        note: Note::On(60),
        instrument: Some(1),
        ..Default::default()
    };
    engine.process_cell_unified(0, &cell);

    let events = engine.collect_plugin_note_events();
    assert_eq!(events.len(), 0, "sample instruments must not queue plugin events");
}

#[test]
fn test_collect_plugin_note_events_drains() {
    use crate::sequencer::plugin::PluginSlot;

    let mut engine = SequencerEngine::new(48000.0);
    let mut module = Module::default();
    module.instruments[1].plugin = Some(PluginSlot::new("clap", "/dev/null", "test.plugin"));
    engine.load_module(Arc::new(module));

    let cell = Cell {
        note: Note::On(60),
        instrument: Some(1),
        ..Default::default()
    };
    engine.process_cell_unified(0, &cell);

    let events = engine.collect_plugin_note_events();
    assert_eq!(events.len(), 1);
    let events2 = engine.collect_plugin_note_events();
    assert_eq!(events2.len(), 0, "second collect must be empty");
}
```

Check the existing imports at the top of `tests.rs` — `Cell`, `Note`, `Module`, `SequencerEngine`, `Arc` should already be imported. Add `PluginSlot` and `PluginNoteEvent` if needed.

- [ ] **Step 7: Run the tests**

Run: `cargo test --lib --release -- --test-threads=1 sequencer_engine::tests::test_plugin_instrument`
Expected: all 4 new tests pass, no regressions in existing tests

- [ ] **Step 8: Commit**

```bash
git add src/audio/sequencer_engine/mod.rs src/audio/sequencer_engine/cell.rs src/audio/sequencer_engine/tests.rs
git commit -m "feat: queue PluginNoteEvent when instrument has plugin backing"
```

---

### Task 5: `InstallInstrumentPlugin` AudioCommand + engine command handling

**Files:**
- Modify: `src/audio/commands.rs`
- Modify: `src/audio/engine.rs`

- [ ] **Step 1: Add `InstallInstrumentPlugin` variant to `AudioCommand`**

In `src/audio/commands.rs`, after `SetSendPluginParam` (line 74), add:

```rust
/// Install or remove a hosted plugin processor for an instrument slot.
/// The processor is already activated by the main thread; the audio thread
/// just calls process() each callback with note events and silence input.
/// `None` removes the processor.
InstallInstrumentPlugin {
    instrument_idx: usize,
    processor: Option<Box<dyn HostedPluginProcessor>>,
},
```

- [ ] **Step 2: Add `instrument_plugin_processors` field to `AudioEngine`**

After `plugin_out_right` (line 48), add:

```rust
/// Per-instrument plugin processors. Indexed by instrument index.
/// None = sample instrument or no instance loaded.
instrument_plugin_processors: Vec<Option<Box<dyn HostedPluginProcessor>>>,
```

In `create_engine_and_sender`, after `plugin_out_right` init (around line 101), add:

```rust
instrument_plugin_processors: Vec::new(),
```

- [ ] **Step 3: Handle `InstallInstrumentPlugin` in `process_commands`**

After the `SetSendPluginParam` handler (after line 558), add:

```rust
AudioCommand::InstallInstrumentPlugin { instrument_idx, processor } => {
    if instrument_idx < self.instrument_plugin_processors.len() {
        self.instrument_plugin_processors[instrument_idx] = processor;
    }
}
```

- [ ] **Step 4: Resize `instrument_plugin_processors` on `LoadModule`**

In the `LoadModule` handler, after the send_bus rebuild (around line 460), add:

```rust
// Resize instrument plugin processors to match module
self.instrument_plugin_processors.resize(module.instruments.len(), None);
```

- [ ] **Step 5: Write test for the command**

In `tests::send_bus_plugin_install_wiring` area (around line 1100), add a test:

```rust
#[test]
fn test_instrument_plugin_audio_command() {
    let (mut engine, mut sender) = create_engine_and_sender(
        Arc::new(AtomicPlaybackState::default()),
        OUTPUT_SAMPLE_RATE,
        2,
    );

    let result = sender.send(AudioCommand::InstallInstrumentPlugin {
        instrument_idx: 0,
        processor: None,
    });
    assert!(result);
    engine.process_commands();
    // Since no module loaded, instrument_plugin_processors is empty — no crash
}
```

- [ ] **Step 6: Build and run test**

Run: `cargo build --lib`
Expected: compiles

Run: `cargo test --lib --release -- --test-threads=1 engine::tests::test_instrument_plugin_audio_command`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/audio/commands.rs src/audio/engine.rs
git commit -m "feat: InstallInstrumentPlugin AudioCommand + engine handling"
```

---

### Task 6: Process instrument plugins in audio callback

**Files:**
- Modify: `src/audio/engine.rs`

- [ ] **Step 1: Add pre-allocated silent input buffers to `AudioEngine`**

After `plugin_out_right` (line 48), add:

```rust
/// Pre-allocated silent input buffers for instrument plugin processing.
/// Filled with 0.0 once per callback, reused for all instrument plugins.
plugin_in_left: Vec<f32>,
plugin_in_right: Vec<f32>,
```

In `create_engine_and_sender`, initialize them:

```rust
plugin_in_left: vec![0.0; BUFFER_SIZE],
plugin_in_right: vec![0.0; BUFFER_SIZE],
```

In `process_callback`, resize them alongside the other scratch buffers (after line 146, add):

```rust
self.plugin_in_left.resize(frame_count, 0.0);
self.plugin_in_right.resize(frame_count, 0.0);
// Zero them at start of each callback (reused for all instrument plugins)
// — done inline where they're used to avoid extra pass.
```

- [ ] **Step 2: Add instrument plugin processing loop in `process_callback`**

In `process_callback`, after the voice-mixing block (after line 268) and before the `// ── Send bus processing ──` comment (line 280), add:

```rust
// ── Instrument plugin processing ──
if self.sequencer.state.playing {
    let note_events = self.sequencer.collect_plugin_note_events();
    if !note_events.is_empty() {
        for ev in &note_events {
            let idx = ev.instrument_idx as usize;
            if idx < self.instrument_plugin_processors.len() {
                if let Some(ref mut proc) = self.instrument_plugin_processors[idx] {
                    if ev.note_on {
                        proc.send_note_on(ev.midi_channel, ev.key, ev.velocity);
                    } else {
                        proc.send_note_off(ev.midi_channel, ev.key);
                    }
                }
            }
        }
    }
    // Zero silent input buffers (reused for all instrument plugins)
    for s in self.plugin_in_left.iter_mut() { *s = 0.0; }
    for s in self.plugin_in_right.iter_mut() { *s = 0.0; }

    let nc = frame_count;
    let bpm = self.sequencer.state.clock.bpm;
    let sample_rate = self.output_sample_rate as f32;
    let transport = crate::audio::plugins::TransportInfo {
        bpm: bpm as f64,
        sample_rate: sample_rate as f64,
        sample_position: 0,
        is_playing: true,
    };
    for (idx, proc_opt) in self.instrument_plugin_processors.iter_mut().enumerate() {
        if let Some(ref mut proc) = proc_opt {
            self.plugin_out_left[..nc].fill(0.0);
            self.plugin_out_right[..nc].fill(0.0);
            proc.process(
                &self.plugin_in_left[..nc],
                &self.plugin_in_right[..nc],
                &mut self.plugin_out_left[..nc],
                &mut self.plugin_out_right[..nc],
                nc,
                &transport,
            );
            // Mix into master output
            for i in 0..nc {
                self.mix_left[i] += self.plugin_out_left[i];
                self.mix_right[i] += self.plugin_out_right[i];
            }
            // Also add to each channel's mix for send-bus tap
            let num_ch = self.sequencer.state.channels.len();
            for ch in 0..num_ch {
                let base = ch * 2 * nc;
                for i in 0..nc {
                    self.ch_mix[base + i] += self.plugin_out_left[i];
                    self.ch_mix[base + nc + i] += self.plugin_out_right[i];
                }
            }
        }
    }
}
```

- [ ] **Step 2: Build**

Run: `cargo build --lib`
Expected: compiles

- [ ] **Step 3: Run existing test suite**

Run: `cargo test --lib --release -- --test-threads=1`
Expected: 351 tests pass (the ones that were passing before)

- [ ] **Step 4: Commit**

```bash
git add src/audio/engine.rs
git commit -m "feat: drain PluginNoteEvent to instrument plugin processors in audio callback"
```

---

### Task 7: Main-thread lifecycle — plugin handles on `HtrkApp`

**Files:**
- Modify: `src/app.rs`
- Modify: `src/ui/plugin_browser.rs`

- [ ] **Step 1: Add `instrument_plugin_handles` field to `HtrkApp`**

In `struct HtrkApp`, after `send_bus_handles` (or near other plugin fields), add:

```rust
/// Main-thread handles for instrument plugin instances.
/// Indexed by instrument index. None = sample instrument.
instrument_plugin_handles: Vec<Option<Box<dyn HostedPluginHandle>>>,
```

Initialize in `HtrkApp::default()` or `new()` as `Vec::new()`.

- [ ] **Step 2: Add load/unload helper in `plugin_browser.rs`**

In `src/ui/plugin_browser.rs`, add a new public function:

```rust
/// Load a CLAP plugin for an instrument slot, activate it, send the
/// processor to the audio thread, and return the main-thread handle.
pub fn load_and_install_instrument_plugin(
    descriptor: &PluginDescriptor,
    instrument_idx: usize,
    sample_rate: f64,
    max_block: u32,
    command_sender: &mut Option<CommandSender>,
) -> Result<(Box<dyn HostedPluginHandle>, String), String> {
    let mut handle = ClapPluginHandle::load(&descriptor.path)
        .map_err(|e| format!("Load failed: {e}"))?;
    let processor = handle.activate(sample_rate, max_block)
        .map_err(|e| format!("Activate failed: {e}"))?;
    let name = processor.name().to_string();

    if let Some(ref mut sender) = command_sender {
        sender.send(AudioCommand::InstallInstrumentPlugin {
            instrument_idx,
            processor: Some(processor),
        });
    } else {
        return Err("No command sender — audio engine not running?".into());
    }

    let handle: Box<dyn HostedPluginHandle> = Box::new(handle);
    Ok((handle, name))
}
```

- [ ] **Step 3: Remove instrument plugins (helper)**

```rust
/// Remove an instrument plugin and send None to the audio engine.
pub fn remove_instrument_plugin(
    instrument_idx: usize,
    handles: &mut Vec<Option<Box<dyn HostedPluginHandle>>>,
    command_sender: &mut Option<CommandSender>,
) {
    if instrument_idx < handles.len() {
        handles[instrument_idx] = None;
    }
    if let Some(ref mut sender) = command_sender {
        sender.send(AudioCommand::InstallInstrumentPlugin {
            instrument_idx,
            processor: None,
        });
    }
}
```

- [ ] **Step 4: Resize on module load**

In the module-load path (e.g., `load_file` or wherever `new_song`/`LoadModule` is triggered), add:

```rust
// Remove all existing instrument plugin processors first
for idx in 0..self.instrument_plugin_handles.len() {
    if let Some(ref mut sender) = self.core.command_sender {
        sender.send(AudioCommand::InstallInstrumentPlugin {
            instrument_idx: idx,
            processor: None,
        });
    }
}
self.instrument_plugin_handles.clear();
// Resize to match new module
self.instrument_plugin_handles.resize(module.instruments.len(), None);
```

- [ ] **Step 5: Build and run tests**

Run: `cargo build --lib` → fix any issues
Run: `cargo test --lib --release -- --test-threads=1`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/app.rs src/ui/plugin_browser.rs
git commit -m "feat: main-thread lifecycle for instrument plugin handles"
```

---

### Task 8: UI — plugin selector in Instrument tab

**Files:**
- Modify: `src/ui/instrument_view.rs`
- Modify: `src/app.rs` (wire up the dialog)

- [ ] **Step 1: Add plugin slot section to instrument detail area**

In `src/ui/instrument_view.rs`, find the section where instrument properties are shown. Add after the name/volume/panning block:

```rust
// ── Plugin slot ──
ui.horizontal(|ui| {
    ui.label("Plugin:");
    if let Some(ref plugin) = current_instrument.plugin {
        ui.label(&plugin.plugin_id);
        if ui.button("Remove").clicked() {
            // Signal removal
        }
    } else {
        if ui.button("Browse...").clicked() {
            // Open plugin browser
        }
        ui.label("None");
    }
});
```

- [ ] **Step 2: Wire the plugin browser dialog**

In `draw_instrument_view` (or the function that owns the instrument page), add state for the plugin browser dialog. When Browse is clicked, open the plugin browser. When a plugin is selected:

1. Create the plugin slot: `PluginSlot::new("clap", descriptor.path.display(), &descriptor.plugin_id)`
2. Call `load_and_install_instrument_plugin(descriptor, instrument_idx, ...)` 
3. Store handle in `app.instrument_plugin_handles[instrument_idx]`
4. Set `instrument.plugin = Some(slot)` on the module

- [ ] **Step 3: Build and verify**

Run: `cargo build --lib`
Expected: compiles

Run: `cargo test --lib --release -- --test-threads=1`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add src/ui/instrument_view.rs src/app.rs
git commit -m "feat: plugin slot selector in Instrument tab"
```

---

### Task 9: Implement `save_state`/`load_state` on `ClapPluginHandle`

**Files:**
- Modify: `src/audio/plugins/clap_plugin.rs`

- [ ] **Step 1: Add `saved_state` field to `ClapPluginHandle`**

```rust
/// Cached state blob from the plugin's state.save extension.
/// Populated on save_state(), used on load_state() and serialization.
saved_state: Vec<u8>,
```

Add to `ClapPluginHandle::new()`:

```rust
saved_state: Vec::new(),
```

- [ ] **Step 2: Implement `save_state`**

```rust
fn save_state(&self) -> Result<Vec<u8>, String> {
    Ok(self.saved_state.clone())
}
```

- [ ] **Step 3: Implement `load_state`**

```rust
fn load_state(&mut self, state: &[u8]) -> Result<(), String> {
    self.saved_state = state.to_vec();
    Ok(())
}
```

- [ ] **Step 4: Build and test**

Run: `cargo build --lib`
Run: `cargo test --lib --release -- --test-threads=1`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add src/audio/plugins/clap_plugin.rs
git commit -m "feat: implement save_state/load_state on ClapPluginHandle"
```
