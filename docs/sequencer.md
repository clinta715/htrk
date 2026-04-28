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

## Tick/Row Processing Flow

```
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
```

## Row Advancement

```rust
impl SequencerState {
    fn advance_row(&mut self, module: &Module) {
        // Handle pattern break
        let break_row = self.pattern_break_row.take();
        let jump_order = self.position_jump_order.take();

        if let Some(order) = jump_order {
            if (order as usize) < module.order_list.len() {
                self.current_order = order as u16;
                self.current_row = break_row.unwrap_or(0);
                self.current_pattern = module.order_list[order as usize];
                return;
            }
        }

        if let Some(row) = break_row {
            self.current_order += 1;
            if (self.current_order as usize) >= module.order_list.len() {
                // Song end
                self.playing = false;
                return;
            }
            self.current_pattern = module.order_list[self.current_order as usize];
            self.current_row = row.min(self.get_current_pattern(module).num_rows as u8 - 1);
            return;
        }

        // Normal advancement
        self.current_row += 1;
        let pattern = self.get_current_pattern(module);

        if self.current_row as usize >= pattern.num_rows {
            // End of pattern, advance order
            self.current_order += 1;
            if (self.current_order as usize) >= module.order_list.len() {
                self.playing = false;
                return;
            }
            self.current_pattern = module.order_list[self.current_order as usize];
            self.current_row = 0;
        }
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
            engine.set_note_delay(channel, ticks);
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
    // Note delay check
    if let Some(target_tick) = engine.get_note_delay_tick(channel) {
        if engine.sequencer.current_tick == target_tick {
            engine.trigger_delayed_note(channel);
        }
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

    // Retrigger
    if channel_state.last_retrigger_interval > 0 {
        if engine.sequencer.current_tick % channel_state.last_retrigger_interval == 0 {
            engine.retrigger_channel(channel);
        }
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
