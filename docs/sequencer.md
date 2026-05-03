# Sequencer

## Overview

The sequencer drives playback by advancing through the module's order list and patterns,
processing effect commands on each tick, and triggering/cutting voices on the audio engine.

## State Machine

```
                           ┌──────────────────────────────────────┐
                           │                                      │
                           ▼                                      │
┌─────────┐  Play    ┌──────────┐  Stop   ┌──────────┐  Play    │
│ Stopped │─────────►│ Playing  │────────►│ Stopped  │──────────┘
└─────────┘          └────┬─────┘         └──────────┘
                          │
                     Pause │
                          ▼
                     ┌──────────┐
                     │ Paused   │─────────► Playing (resume)
                     └──────────┘
```

## Tick/Row Processing Flow (non-XM)

For each audio sample:
  sample_counter++
  if sample_counter >= samples_per_tick:
    sample_counter -= samples_per_tick
    current_tick++

    ┌──────────────────────────────────────────────────┐
    │ TICK 0 (new row):                                │
    │   1. Read pattern cell for each channel          │
    │   2. Handle position jump / pattern break        │
    │   3. Handle note delay (EDx) - defer to later tick│
    │   4. Trigger new notes (allocate voices)         │
    │   5. Apply immediate effects:                    │
    │      - Set volume (Cxx)                          │
    │      - Set panning (8xx)                         │
    │      - Set sample offset (9xx)                   │
    │      - Set speed (Fxx < 32)                      │
    │      - Set tempo (Fxx >= 32)                     │
    │      - Set global volume                         │
    │   6. Initialize continuous effects:              │
    │      - Portamento (1xx, 2xx, 3xx)                │
    │      - Vibrato (4xy)                             │
    │      - Tremolo (7xy)                             │
    │      - Arpeggio (0xy)                            │
    │      - Volume slide (Axy)                        │
    └──────────────────────────────────────────────────┘

    ┌──────────────────────────────────────────────────┐
    │ TICK 1..N (mid-row):                             │
    │   1. Process note delays (EDx) at target tick    │
    │   2. Process note cuts (ECx) at target tick      │
    │   3. Advance continuous effects:                 │
    │      - Portamento: slide pitch                   │
    │      - Vibrato: oscillate pitch                  │
    │      - Tremolo: oscillate volume                 │
    │      - Volume slide: change volume               │
    │      - Arpeggio: cycle through note offsets      │
    │   4. Advance envelopes                           │
    │   5. Advance fade-outs                           │
    └──────────────────────────────────────────────────┘

    if current_tick >= speed:
      current_tick = 0
      advance_row()

## XM-Specific Tick Processing

When `module_format == ModuleFormat::XM`, a **separate code path** is used
(`process_tick_zero_xm` / `process_effects_tick_xm`) that implements FT2-accurate
playback:

### Tick Zero (XM)

1. The raw effect type byte (`eff_typ_xm`) and parameter byte (`eff_xm`) are stored
   on the `ChannelState` for per-tick effect dispatch.
2. **Volume column** effects are processed through the format loader (not the engine).
   `Cell.volume` holds decoded volume values (0-64) or None for effect-based columns.
3. **Note triggering** uses pitch computed from the Amiga/Linear period table:
   - `period = get_note_period(note, fine_tune, linear_slides)`
   - `frequency = period_to_frequency(period, linear_slides, 8363)`
4. Tone portamento stores the target period (`want_period`) and direction (`porta_dir`)
   without retriggering the voice.
5. **Portamento up/down** apply immediately on tick 0 for XM (not deferred like MOD).
6. Effects are dispatched by their raw type byte (`eff_typ_xm`) via a match block
   that mirrors FT2's function pointer array structure.

### Ticks 1..N (XM)

1. Continuous effects are dispatched by `eff_typ_xm` byte value:
   - `0` → Arpeggio (uses `get_arp_tab()` 256-entry table matching FT2 overflow)
   - `1` → Portamento up (period-based)
   - `2` → Portamento down (period-based)
   - `3` → Tone portamento (period slide toward target, optional glissando)
   - `4` → Vibrato (period modulation with 32-entry vibTab)
   - `5` → Tone portamento + volume slide
   - `6` → Vibrato + volume slide
   - `7` → Tremolo (with FT2 vibPos bug)
   - `0xA` → Volume slide
   - `0xE` → Extended effects (retrig, note cut, note delay)
   - `0x1D` → Tremor
   - `0x1B` → Multi retrig (Rxy)
2. Volume column non-zero tick effects are applied from the raw `vol_kol` byte:
   - `0x6x` → Volume slide down
   - `0x7x` → Volume slide up
   - `0xDx` → Pan slide left
   - `0xEx` → Pan slide right
   - `0xFx` → Tone portamento
3. Envelopes are advanced **after** all effects (matching FT2's `fixaEnvelopeVibrato`).
```

## Row Advancement

```rust
impl SequencerEngine {
    fn advance_row(&mut self) {
        self.state.current_tick = 0;

        // Clear per-voice cutoff ticks
        for voice in &mut self.voices {
            if voice.active {
                voice.cutoff_tick = None;
            }
        }

        // Reset per-channel row-level state
        for ch in &mut self.state.channels {
            ch.retrigger_set_this_row = false;
            ch.delayed_cell = None;
            ch.note_delay_ticks = 0;
            ch.active_effects = ActiveEffects::default();  // Prevent carryover between patterns
        }

        // Handle pattern delay
        if self.state.row_delay_active && self.state.pattern_delay_ticks > 0 {
            self.state.pattern_delay_ticks -= 1;
            return;
        }
        self.state.row_delay_active = false;
        self.state.pattern_delay_ticks = 0;

        // Handle position jump / pattern break
        // ... (same logic as before)
    }
}
```

## Effect Command Processing

### Effect Memory

Many tracker effects use "memory" — if the parameter is 0, the last non-zero value
for that effect on the same channel is used instead. This is critical for authentic
tracker playback.

```rust
impl ChannelState {
    fn apply_effect_memory(&mut self, effect: &mut Effect) {
        match effect {
            Effect::PortamentoUp { ref mut speed } => {
                if *speed == 0 { *speed = self.last_portamento_up_speed; }
                else { self.last_portamento_up_speed = *speed; }
            }
            Effect::PortamentoDown { ref mut speed } => {
                if *speed == 0 { *speed = self.last_portamento_down_speed; }
                else { self.last_portamento_down_speed = *speed; }
            }
            Effect::TonePortamento { ref mut speed } => {
                if *speed == 0 { *speed = self.last_tone_portamento_speed; }
                else { self.last_tone_portamento_speed = *speed; }
            }
            Effect::Vibrato { ref mut speed, ref mut depth } => {
                if *speed == 0 { *speed = self.last_vibrato_speed; }
                else { self.last_vibrato_speed = *speed; }
                if *depth == 0 { *depth = self.last_vibrato_depth; }
                else { self.last_vibrato_depth = *depth; }
            }
            Effect::Tremolo { ref mut speed, ref mut depth } => {
                if *speed == 0 { *speed = self.last_tremolo_speed; }
                else { self.last_tremolo_speed = *speed; }
                if *depth == 0 { *depth = self.last_tremolo_depth; }
                else { self.last_tremolo_depth = *depth; }
            }
            _ => {}
        }
    }
}
```

### Effect Processing: Row (Tick 0)

```rust
fn process_row_effects(
    engine: &mut AudioEngine,
    channel: usize,
    cell: &Cell,
    channel_state: &mut ChannelState,
    module: &Module,
) {
    // 1. Handle instrument change
    if let Some(inst) = cell.instrument {
        channel_state.last_instrument = inst;
    }

    // 2. Handle note
    match cell.note {
        Note::On(key) => {
            // Determine sample from instrument sample map
            let sample_idx = if let Some(inst_idx) = cell.instrument.or(Some(channel_state.last_instrument)) {
                module.instruments.get(inst_idx as usize)
                    .map(|inst| inst.sample_map[key as usize])
                    .unwrap_or(0)
            } else {
                channel_state.last_sample
            };

            engine.trigger_note(channel, cell.note, cell.instrument, Some(sample_idx), module);
            channel_state.last_note = cell.note;
            channel_state.last_sample = sample_idx;
        }
        Note::Off => {
            engine.note_off(channel);
        }
        Note::Cut => {
            engine.note_cut(channel);
        }
        Note::Fade => {
            engine.note_fade(channel);
        }
        Note::None => {}
    }

    // 3. Volume column effects
    if let Some(vol) = cell.volume {
        match vol {
            0..=64 => {
                channel_state.channel_volume = vol;
            }
            65..=74 => {
                // Fine volume slide up
                let amount = vol - 65;
                channel_state.channel_volume = (channel_state.channel_volume + amount).min(64);
            }
            75..=84 => {
                // Fine volume slide down
                let amount = vol - 75;
                channel_state.channel_volume = (channel_state.channel_volume as i8 - amount as i8).max(0) as u8;
            }
            85..=94 => {
                // Volume slide up
                channel_state.last_volume_slide_up = vol - 85;
            }
            95..=104 => {
                // Volume slide down
                channel_state.last_volume_slide_down = vol - 95;
            }
            105..=114 => {
                // Tone portamento
                channel_state.last_tone_portamento_speed = vol - 105;
            }
            115..=124 => {
                // Vibrato
                channel_state.last_vibrato_depth = vol - 115;
            }
            _ => {}
        }
    }

    // 4. Main effect column
    let mut effect = cell.effect;
    channel_state.apply_effect_memory(&mut effect);

    match effect {
        Effect::Arpeggio { note1, note2 } => {
            channel_state.last_arpeggio = (note1, note2);
        }
        Effect::PortamentoUp { speed } => {
            // Set portamento speed (applied each tick)
            channel_state.last_portamento_up_speed = speed;
        }
        Effect::PortamentoDown { speed } => {
            channel_state.last_portamento_down_speed = speed;
        }
        Effect::TonePortamento { speed } => {
            // Set target to current note frequency, slide at speed
            channel_state.last_tone_portamento_speed = speed;
            if let Note::On(key) = cell.note {
                let target_freq = key.frequency().unwrap_or(440.0);
                engine.set_portamento_target(channel, target_freq, speed);
            }
        }
        Effect::SetPanning { pan } => {
            channel_state.channel_panning = pan.min(64);
        }
        Effect::SetSampleOffset { offset } => {
            engine.set_sample_offset(channel, (offset as usize) * 256);
        }
        Effect::VolumeSlide { up, down } => {
            channel_state.last_volume_slide_up = up;
            channel_state.last_volume_slide_down = down;
        }
        Effect::PositionJump { order } => {
            engine.sequencer.position_jump_order = Some(order);
        }
        Effect::SetVolume { volume } => {
            channel_state.channel_volume = volume.min(64);
        }
        Effect::PatternBreak { row } => {
            engine.sequencer.pattern_break_row = Some(row);
        }
        Effect::SetSpeed { speed } => {
            engine.sequencer.speed = speed.max(1);
            engine.sequencer.samples_per_tick =
                (engine.sample_rate * 5.0) / (engine.sequencer.bpm as f64 * 2.0);
        }
        Effect::SetTempo { bpm } => {
            engine.sequencer.bpm = bpm.max(32) as u16;
            engine.sequencer.samples_per_tick =
                (engine.sample_rate * 5.0) / (engine.sequencer.bpm as f64 * 2.0);
        }
        Effect::SetGlobalVolume { volume } => {
            engine.sequencer.global_volume = volume.min(128);
        }
        Effect::NoteCutAfter { ticks } => {
            engine.set_note_cut_after(channel, ticks);
        }
        Effect::NoteDelay { ticks } => {
            // NoteDelay is handled in process_cell, not in apply_effect.
            // The cell is stored in channel_state.delayed_cell and triggered
            // on the target tick via trigger_delayed_note.
        }
        Effect::Retrigger { interval } => {
            channel_state.last_retrigger_interval = interval;
        }
        _ => {}
    }
}
```

### Effect Processing: Tick (Tick 1..N)

```rust
fn process_tick_effects(
    engine: &mut AudioEngine,
    channel: usize,
    channel_state: &mut ChannelState,
) {
    // Note delay check (per-channel, not per-voice)
    if channel_state.delayed_cell.is_some()
        && engine.sequencer.current_tick == channel_state.note_delay_ticks
    {
        engine.trigger_delayed_note(channel);
    }

    // Note cut check
    if let Some(target_tick) = engine.get_note_cut_tick(channel) {
        if engine.sequencer.current_tick == target_tick {
            engine.note_cut(channel);
        }
    }

    // Portamento
    if channel_state.last_portamento_up_speed > 0 {
        engine.apply_portamento_up(channel, channel_state.last_portamento_up_speed);
    }
    if channel_state.last_portamento_down_speed > 0 {
        engine.apply_portamento_down(channel, channel_state.last_portamento_down_speed);
    }
    if channel_state.last_tone_portamento_speed > 0 {
        engine.apply_tone_portamento(channel, channel_state.last_tone_portamento_speed);
    }

    // Vibrato
    if channel_state.last_vibrato_depth > 0 {
        engine.apply_vibrato(
            channel,
            channel_state.last_vibrato_speed,
            channel_state.last_vibrato_depth,
        );
    }

    // Tremolo
    if channel_state.last_tremolo_depth > 0 {
        engine.apply_tremolo(
            channel,
            channel_state.last_tremolo_speed,
            channel_state.last_tremolo_depth,
        );
    }

    // Volume slide
    if channel_state.last_volume_slide_up > 0 {
        channel_state.channel_volume =
            (channel_state.channel_volume + channel_state.last_volume_slide_up).min(64);
    }
    if channel_state.last_volume_slide_down > 0 {
        channel_state.channel_volume =
            (channel_state.channel_volume as i8 - channel_state.last_volume_slide_down as i8)
            .max(0) as u8;
    }

    // Arpeggio
    let (n1, n2) = channel_state.last_arpeggio;
    if n1 != 0 || n2 != 0 {
        let tick = engine.sequencer.current_tick % 3;
        let semitone_offset = match tick {
            0 => 0,
            1 => n1,
            _ => n2,
        };
        engine.apply_arpeggio(channel, semitone_offset);
    }

    // Retrigger (based on effect memory, continues across rows until overridden)
    if channel_state.last_retrigger_interval > 0
        && engine.sequencer.current_tick > 0
        && engine.sequencer.current_tick % channel_state.last_retrigger_interval == 0
    {
        engine.retrigger_channel(channel);
    }
}
```

## Waveform Generators for Effects

### Vibrato Waveform

```rust
fn vibrato_offset(phase: f32, depth: u8, waveform: VibratoWaveform) -> f32 {
    let depth_f = depth as f32 / 15.0;  // Normalize depth to 0-1 range
    let value = match waveform {
        VibratoWaveform::Sine => {
            (phase * std::f32::consts::TAU).sin() * 0.5 + 0.5
        }
        VibratoWaveform::Square => {
            if (phase * 2.0).fract() < 1.0 { 1.0 } else { 0.0 }
        }
        VibratoWaveform::Ramp => {
            phase.fract()  // Sawtooth 0→1
        }
        VibratoWaveform::Random => {
            // Use a simple hash for deterministic randomness
            let hash = ((phase as u32).wrapping_mul(1103515245).wrapping_add(12345)) >> 16;
            (hash & 0xFF) as f32 / 255.0
        }
    };
    (value * 2.0 - 1.0) * depth_f  // Map to -depth..+depth
}
```

### Vibrato Table (Sine)

IT/XM trackers use a pre-computed vibrato table with 64 entries:

```rust
const VIBRATO_TABLE: [u8; 64] = [
      0,  24,  49,  74,  97, 120, 141, 161,
    180, 197, 212, 224, 235, 244, 250, 253,
    255, 253, 250, 244, 235, 224, 212, 197,
    180, 161, 141, 120,  97,  74,  49,  24,
      0, -24, -49, -74, -97,-120,-141,-161,
   -180,-197,-212,-224,-235,-244,-250,-253,
   -255,-253,-250,-244,-235,-224,-212,-197,
   -180,-161,-141,-120, -97, -74, -49, -24,
];
```

### Portamento Calculations

#### Linear Slides (IT default, XM)

```
new_frequency = old_frequency ± slide_speed * (old_frequency / 512)
```

Each tick, the frequency is changed by a fraction of the current frequency.

#### Amiga Slides (MOD, IT old-effects mode)

```
new_period = old_period ± slide_speed
frequency = 8363 * 1712 / period
```

Period-based sliding: each tick adds/subtracts from the period directly.

### Pitch-to-Period Conversion (Amiga Mode)

```rust
fn frequency_to_period(freq: f64) -> f64 {
    8363.0 * 1712.0 / freq
}

fn period_to_frequency(period: f64) -> f64 {
    8363.0 * 1712.0 / period
}
```

## Effect Activity Tracking

Each channel maintains an `ActiveEffects` bitmask that tracks which continuous effects
are currently active. This replaces the old approach of checking effect parameters
(> 0) directly, which caused side effects from inactive effects.

```rust
struct ActiveEffects {
    volume_slide: bool,
    portamento_up: bool,
    portamento_down: bool,
    tone_portamento: bool,
    vibrato: bool,
    tremolo: bool,
    arpeggio: bool,
    panbrello: bool,
    tremor: bool,
}
```

Effects set their corresponding flag in `ActiveEffects` when applied via `apply_effect`.
On tick 0 of each row, the flags are reset.     Only effects whose flag is set are
processed on subsequent ticks (1..N). This prevents stale effect data from a previous
row from unintentionally affecting the current row.

For XM playback, a separate set of effect flags is not used — instead the raw
`eff_typ_xm` byte from the pattern cell is stored on `ChannelState` and effects
fire based on whether their effect type was written to the channel.

For MOD format, some effects (portamento, volume slide) are **not** applied on tick 0
(they only set the flag and store parameters). The effect takes effect starting from
tick 1. For IT/XM, these effects also apply on tick 0.

## Note Delay Processing

Note Delay (EEx / EDx) is handled via per-channel state rather than per-voice:

```rust
struct ChannelState {
    // ...
    delayed_cell: Option<Cell>,     // The cell to trigger on the delayed tick
    note_delay_ticks: u8,           // Which tick to fire the delayed note on
    active_effects: ActiveEffects,  // Which continuous effects are active
}
```

When `process_cell_with_module` encounters a cell with a NoteDelay effect:
1. The full cell is stored in `delayed_cell`
2. `note_delay_ticks` is set from the effect parameter
3. `last_note` is updated immediately (for display / subsequent effect targeting)
4. Volume column and sample offset effects are applied immediately
5. The note trigger itself is deferred

On the target tick, `trigger_delayed_note` re-processes the stored cell:
1. Resolves the instrument and sample (using the current `last_instrument` + key mapping)
2. Resets `channel_volume` to the sample's default volume when a new instrument is specified
3. Applies volume column and SetVolume from the cell
4. Triggers the note via `trigger_channel_note` (which handles sample offset from the cell)

**Critical detail**: The `channel_volume` reset (step 2) was missing in earlier versions,
causing delayed notes to inherit whatever volume was left by previous per-row effects
(e.g., volume slides). For example, at pattern 8 row 30 of `cry4bass.mod`, channel 0's
delayed C-5 played at a reduced volume because `VolSlide(0F)` on row 29 had slid the
volume down, and the delayed trigger used that stale value instead of sample 12's
default volume (64).

This replaces the old approach of storing a `delay_tick` on the `Voice` struct and
re-triggering using `last_note` / `last_sample` without re-evaluating the cell.

## Special Row Events

### Position Jump (Bxx)

```
When encountered: set position_jump_order = xx
Applied at end of row: jump to order xx, row 0
Can combine with Pattern Break: jump to order xx, row yy (from Dxx)
```

### Pattern Break (Dxx)

```
When encountered: set pattern_break_row = xx
At end of current pattern: advance to next order, start at row xx
If combined with Bxx: jump to order from Bxx, start at row from Dxx
```

### Pattern Loop (E6x)

```
E60: Set loop start point (at current row)
E61-E6F: Loop back to start point N times
  - First encounter: set loop counter to N
  - Each subsequent: decrement counter, jump back if > 0
  - When counter reaches 0: continue normally
```

### Pattern Delay (S6x / EEx)

```
Delay the entire row by X ticks
The row is "held" — no advancement occurs for that many extra ticks
Effectively: speed += X for this row only
```

## Song End Detection

```rust
fn check_song_end(&mut self, module: &Module) -> bool {
    // Song ends when:
    // 1. Order list is exhausted (current_order >= order_list.len())
    // 2. AND no position jump is pending
    if self.current_order as usize >= module.order_list.len()
        && self.position_jump_order.is_none()
    {
        self.playing = false;
        return true;
    }

    // Loop detection: if position jump goes to order 0 with no pattern break
    // In "play once" mode, this is the end
    if let Some(order) = self.position_jump_order {
        if order == 0 && self.pattern_break_row.is_none_or(|r| r == 0) {
            // This is a loop — handle based on play mode
            if self.play_mode == PlayMode::Once {
                self.playing = false;
                return true;
            }
        }
    }

    false
}
```

## Play Modes

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayMode {
    Once,         // Stop at song end
    Loop,         // Loop back to beginning
    Pattern,      // Play current pattern in a loop
    Order,        // Play current order entry in a loop
}
```
