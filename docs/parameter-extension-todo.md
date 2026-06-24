# CLAP Parameter Extension — Deferred Work

## Goal

Expose per-plugin parameter sliders in the Send FX view and route parameter
changes to the audio thread as `ParamValueEvent`s. This lets users tweak
plugin parameters from the htrk UI without opening the plugin's own editor
window, and is the foundation for plugin parameter automation lanes.

## Reference implementation

`hdaw2/src/audio/clap_instance.rs` — `ClapPluginState` with
`Vec<Arc<AtomicU32>>` of parameter values, lock-free read/write from any
thread.

`hdaw2/src/audio/param_ring.rs` — `ParamRingBuffer` SPSC queue using
`UnsafeCell<Vec>` + `AtomicU64` indices.

`hdaw2/src/audio/clap_effect.rs:370-385` — drain the ring buffer in the
audio thread right before `clack.process()` and feed `ParamValueEvent`s
into the input events.

`hdaw2/src/ui/effect_editor/mod.rs:410-429` — UI sliders for each
`ParameterInfo` with an "A" toggle for automation lane attachment.

## What needs to change in htrk

### 1. `ClapPluginHandle::parameter_info()` in `src/audio/plugins/clap_plugin.rs`

Currently a stub returning `Vec::new()`. Should:

```rust
fn parameter_info(&self) -> Vec<ParamInfo> {
    let instance = self.instance.as_ref().unwrap();
    let mut handle = instance.plugin_handle();
    let Some(params) = handle.get_extension::<PluginParams>() else {
        return Vec::new();
    };
    let count = params.count(&mut handle);
    let mut buf = ParamInfoBuffer::new();
    (0..count)
        .filter_map(|i| params.get_info(&mut handle, i, &mut buf))
        .map(|info| ParamInfo {
            id: info.id.get(),
            name: String::from_utf8_lossy(info.name).into_owned(),
            min: info.min_value as f32,
            max: info.max_value as f32,
            default: info.default_value as f32,
            is_automatable: /* from info.flags */,
            is_modulatable: /* from info.flags */,
        })
        .collect()
}
```

### 2. `ClapPluginProcessor::set_parameter` / `get_parameter` / `parameter_count`

Currently stubs. Should:
- `set_parameter(param_id, value)`: push `(param_id, value)` to a
  `ParamRingBuffer` shared between the handle and the processor.
- `get_parameter(param_id)`: read from an `Arc<AtomicU32>` value table
  (or look up via the handle's shared state).
- `parameter_count`: return the count from the params extension.
- Add `latency()` as well — read from the latency extension.

### 3. `ClapPluginProcessor::process`

Right before calling `started.process(...)`, drain the ring buffer and
push `ParamValueEvent`s into the input event buffer:

```rust
let pckn = Pckn::new(0, 0, 0, 0);
for change in self.ring_buffer.drain(&mut drained, 64) {
    let ev = ParamValueEvent::new(
        0,
        ClapId::from(change.param_id),
        pckn,
        change.value,
        Cookie::default(),
    );
    input_events.push(&ev);
}
input_events.sort();  // CLAP requires events sorted by time
```

### 4. UI in `src/ui/sendfx_editor.rs`

For each send bus with a loaded plugin, add a "Parameters" collapsible
section. Inside, iterate `handle.parameter_info()` and show one
`egui::Slider` per parameter. On change, call
`handle.set_parameter(id, value)` and forward the new value to the audio
thread via the same ring buffer (or directly).

### 5. `ParamInfo` struct

Currently in `src/audio/plugins/mod.rs:106-114`:
```rust
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub is_automatable: bool,
    pub is_modulatable: bool,
}
```

May need to grow fields: `label`, `flags`, `group_id` etc. Mirror
hdaw2's `ParameterInfo` to keep compatibility.

## Estimated scope

300-500 LOC, 2-3 days.

## Blocker

None. The host-side extensions (Phase A) are already in place. The only
remaining prerequisites are:
- A separate `ParamRingBuffer` type (copy from hdaw2 or write our own).
- Wiring the `PluginParams` extension access into the audio thread
  safely (params extension is main-thread only; use the same pattern as
  hdaw2 — drain in audio thread, push in main thread).

## Tracking

The TODO markers are in:
- `src/audio/plugins/clap_plugin.rs` `parameter_info` (line ~337)
- `src/audio/plugins/clap_plugin.rs` `set_parameter` / `get_parameter` /
  `parameter_count` (lines ~721-732)
- `AGENTS.md` §8 (effect architecture)
- This file (`docs/parameter-extension-todo.md`)
