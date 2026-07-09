# Audio Engine

## Overview

The audio engine runs entirely within the cpal audio callback on a real-time thread.
It must never allocate, block, or panic. All buffers are pre-allocated at init time.

## XM Playback Accuracy

The XM format uses **FastTracker 2's** period-based pitch system, which differs
fundamentally from the MIDI frequency-based system used by IT/S3M. The sequencer
maintains a separate code path (`process_effects_tick_xm` / `process_tick_zero_xm`)
for XM format that replicates FT2's exact behavior:

### Period System

Two period tables are generated (1936 entries each):
- **Amiga mode**: `period = 1712 / 2^(index / (12*16))`
- **Linear mode**: `period = 7680 - note*64 + fine/2`, with `logTab[768]` for frequency conversion

The `LINEAR_FREQUENCIES` flag from the XM header is now respected. When set, the
linear period table and logTab-based frequency conversion are used instead of the
Amiga formula.

### Portamento (period-based)

```
XM:   real_period -= speed * 4     (linear period units)
IT:   frequency *= 2^(speed / 768) (exponential frequency)
```

This matches FT2's exact period-sliding behavior where each semitone spans 16
period units, and the command parameter maps directly to period deltas.

### Vibrato

Uses a **32-entry** `vibTab[]` table matching FT2's original, instead of the
64-entry sine table. The vibrato modulates the **period** (not frequency):

```
period_offset = (vibTab[phase] * depth) >> 5
out_period = real_period + period_offset  (additive, signed)
```

### Tremolo (with FT2 bug replication)

The ramp waveform sign check uses `vibPos` instead of `tremPos`, replicating a
known FT2.08/FT2.09 bug for maximum playback accuracy:

```c
// FT2 bug: should be ch->tremPos, not ch->vibPos
if (ch->vibPos < 0)
    tmp_trem = ~tmp_trem;
```

### Envelope Timing

On trigger, envelope position starts at -1 (equivalent to FT2's counter starting at
65535, which wraps to 0 on the first advance). Envelopes advance before effect
processing, matching FT2's `fixaEnvelopeVibrato()` ordering.

### Fadeout

XM fadeout uses a 32768-range integer system matching FT2:

```
fadeOutAmp: 0..32768
fadeOutSpeed: raw from XM header (0..4095)
Volume: (envVal * outVol * fadeOutAmp) >> 18 then * globVol >> 7
```

This contrasts with the non-XM path which uses float multiplication and a 4096
divisor.

### Auto-Vibrato (Instrument Header)

Instrument-level auto-vibrato is now fully implemented with:
- Sine, square, ramp-up, ramp-down waveforms (256-entry `vibSineTab`)
- Sweep: amplitude starts at 0 and ramps linearly to `vibDepth*256`
- Modulates the base period directly

### Volume Column

For XM format, the volume column is decoded by the format loader (not the engine)
into the standard XM effects via `decode_xm_volume_column()`. Volume column
non-zero tick effects (slide down/up, vibrato, pan slide, tone portamento) are
processed in the XM tick-zero effects path using the raw `vol_kol` byte.

## Pipeline

```
                    ┌──────────────┐
                    │   Sequencer  │ (advances ticks/rows, triggers voices)
                    └──────┬───────┘
                           │ voice events
                           ▼
┌──────────────────────────────────────────────────────────┐
│                     Voice Pool                            │
│                                                           │
│  Voice 0: [Sample → Resample → Envelope → Vol/Pan]  ──┐ │
│  Voice 1: [Sample → Resample → Envelope → Vol/Pan]  ──┤ │
│  Voice 2: [Sample → Resample → Envelope → Vol/Pan]  ──┤ │
│  ...                                                   │ │
│  Voice N: [Sample → Resample → Envelope → Vol/Pan]  ──┤ │
│                                                        │ │
└────────────────────────────────────────────────────────┘ │
                                                           ▼
                                                    ┌────────────┐
                                                    │   Mixer    │
                                                    │ (sum all   │
                                                    │  voices)   │
                                                    └─────┬──────┘
                                                          │
                                                    ┌─────▼──────┐
                                                    │  Global FX │
                                                    │ (optional: │
                                                    │  limiter)  │
                                                    └─────┬──────┘
                                                          │
                                                    ┌─────▼──────┐
                                                    │   Output   │
                                                    │  (cpal)    │
                                                    └────────────┘
```

## Audio Device Initialization

```rust
// device.rs

struct AudioDevice {
    stream: cpal::Stream,
    output_config: cpal::StreamConfig,
    sample_rate: u32,
    buffer_size: u32,
}

impl AudioDevice {
    fn init(engine: Arc<AudioEngine>) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host.default_output_device()
            .ok_or(AudioError::NoDeviceAvailable)?;

        let supported = device.supported_output_configs()
            .map_err(|e| AudioError::DeviceOpenFailed(e.to_string()))?
            .find(|c| {
                c.channels() == 2
                && c.min_sample_rate().0 <= 48000
                && c.max_sample_rate().0 >= 48000
                && matches!(c.sample_format(), cpal::SampleFormat::F32)
            })
            .ok_or(AudioError::UnsupportedSampleRate {
                requested: 48000,
                available: vec![],
            })?;

        let config = supported.with_sample_rate(cpal::SampleRate(48000))
                              .config();

        let sample_rate = config.sample_rate.0;
        let buffer_size = config.buffer_size.clone();

        let stream = device.build_output_stream(
            &config,
            {
                let engine = Arc::clone(&engine);
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    engine.process_callback(data);
                }
            },
            |err| eprintln!("Audio error: {}", err),
            None,
        ).map_err(|e| AudioError::StreamCreationFailed(e.to_string()))?;

        Ok(AudioDevice {
            stream,
            output_config: config,
            sample_rate,
            buffer_size: match buffer_size {
                cpal::BufferSize::Fixed(n) => n,
                cpal::BufferSize::Default => 256,
            },
        })
    }

    fn play(&self) -> Result<(), AudioError> {
        self.stream.play()
            .map_err(|e| AudioError::StreamCreationFailed(e.to_string()))
    }

    fn pause(&self) -> Result<(), AudioError> {
        self.stream.pause()
            .map_err(|e| AudioError::StreamCreationFailed(e.to_string()))
    }
}
```

## Audio Engine

```rust
// engine.rs

struct AudioEngine {
    // Voice pool (pre-allocated, fixed size)
    voices: Vec<Voice>,
    num_active_voices: usize,

    // Sequencer
    sequencer: SequencerState,
    module: Option<Arc<Module>>,

    // Communication
    command_receiver: CommandReceiver,
    playback_state: Arc<AtomicPlaybackState>,

    // Output mixing buffer (pre-allocated)
    mix_buffer_left: Vec<f32>,
    mix_buffer_right: Vec<f32>,

    // Configuration
    sample_rate: f64,
    master_volume: f32,
    interpolation: InterpolationType,
}

impl AudioEngine {
    fn process_callback(&mut self, output: &mut [f32]) {
        // 1. Process pending commands from UI
        self.process_commands();

        // 2. Clear mix buffer
        self.mix_buffer_left.fill(0.0);
        self.mix_buffer_right.fill(0.0);

        if !self.sequencer.playing {
            output.fill(0.0);
            return;
        }

        let frames = output.len() / 2;  // Stereo

        for frame in 0..frames {
            // 3. Advance sequencer tick timing
            self.advance_sequencer_sample();

            // 4. Mix all active voices for this sample
            let (left, right) = self.mix_voices();

            // 5. Write to output
            output[frame * 2] = left * self.master_volume;
            output[frame * 2 + 1] = right * self.master_volume;
        }

        // 6. Apply global effects (limiter)
        self.apply_global_effects(output);

        // 7. Update shared playback state for UI
        self.update_playback_state();
    }
}
```

## Tick Timing

The fundamental timing unit is the "tick". The relationship between BPM, speed, and
audio sample rate determines how many audio samples constitute one tick.

```
samples_per_tick = (sample_rate * 5.0) / (bpm * 2.0)
```

| BPM | Speed (ticks/row) | Row Duration | Ticks/Second | Samples/Tick (48kHz) |
|-----|-------------------|--------------|--------------|----------------------|
| 125 | 6                 | 120ms        | 50           | 960                  |
| 140 | 6                 | ~107ms       | 56           | ~857                 |
| 125 | 3                 | 60ms         | 50           | 960                  |
| 250 | 6                 | 60ms         | 100          | 480                  |

```rust
fn advance_sequencer_sample(&mut self) {
    self.sequencer.sample_counter += 1.0;

    if self.sequencer.sample_counter >= self.sequencer.samples_per_tick {
        self.sequencer.sample_counter -= self.sequencer.samples_per_tick;

        // Process tick
        if self.sequencer.current_tick == 0 {
            // New row: process pattern data
            self.process_row();
        } else {
            // Mid-row tick: update effects
            self.process_tick_effects();
        }

        self.sequencer.current_tick += 1;
        if self.sequencer.current_tick >= self.sequencer.speed {
            // Advance to next row
            self.sequencer.current_tick = 0;
            self.advance_row();
        }
    }
}
```

## Voice Management

### Voice Lifecycle

```
                    ┌─────────────────┐
   Note-On  ───────►│     Active       │◄────── Note-On (with NNA: Continue)
                    │                  │
                    │  - Advance pos   │
                    │  - Apply effects │
                    │  - Mix to output │
                    │                  │
                    └──┬──────┬───────┘
                       │      │
              Note-Off │      │ Note-Cut
                       ▼      ▼
                ┌──────────┐  ┌──────────┐
                │ Releasing │  │  Killed   │
                │ (fade-out)│  │ (silenced)│
                │           │  └──────────┘
                │ envelope  │
                │ release + │
                │ fade_out  │
                └─────┬─────┘
                      │
              fade=0.0│
                      ▼
                ┌──────────┐
                │ Inactive  │
                │ (recycled)│
                └──────────┘
```

### Voice Allocation

```rust
impl AudioEngine {
    fn allocate_voice(&mut self) -> Option<&mut Voice> {
        // Priority: find an inactive voice
        for voice in &mut self.voices {
            if !voice.active {
                return Some(voice);
            }
        }

        // If all voices active, find one that is in fade-out (lowest fade_out_volume)
        let mut best_idx = None;
        let mut best_vol = 1.0;
        for (i, voice) in self.voices.iter_mut().enumerate() {
            if voice.fading && voice.fade_out_volume < best_vol {
                best_vol = voice.fade_out_volume;
                best_idx = Some(i);
            }
        }

        if let Some(idx) = best_idx {
            self.voices[idx].active = false;
            return Some(&mut self.voices[idx]);
        }

        // Last resort: steal the oldest active voice (voice 0)
        self.voices[0].active = false;
        Some(&mut self.voices[0])
    }

    fn trigger_note(
        &mut self,
        channel: usize,
        note: Note,
        instrument: Option<u8>,
        sample: Option<u8>,
        module: &Module,
    ) {
        let channel_state = &self.sequencer.channels[channel];

        // Determine sample to use
        let sample_idx = sample
            .or_else(|| instrument.and_then(|i| module.instruments.get(i as usize)
                .map(|inst| inst.sample_map[note_key as usize])))
            .unwrap_or(0);

        if sample_idx == 0 { return; }

        let sample = &module.samples[sample_idx as usize];

        // Handle NNA (New Note Action) for existing voice on this channel
        self.handle_nna(channel, note);

        // Allocate new voice
        let voice = match self.allocate_voice() {
            Some(v) => v,
            None => return,
        };

        // Initialize voice
        let frequency = note.frequency().unwrap_or(440.0);
        let base_freq = frequency / (sample.sample_rate as f64 / 8363.0);

        *voice = Voice {
            active: true,
            sample: Some(sample.data.clone()),
            sample_rate: sample.sample_rate as f64,
            loop_type: sample.loop_type,
            position: 0.0,
            position_end: sample.data.len() as f64,
            direction: 1.0,
            base_frequency: base_freq,
            current_frequency: base_freq,
            sample_delta: base_freq / self.sample_rate,
            base_volume: sample.default_volume as f32 / 64.0,
            envelope_volume: 1.0,
            tremolo_volume: 0.0,
            channel_volume: channel_state.channel_volume as f32 / 64.0,
            global_volume: 1.0,
            fade_out_volume: 1.0,
            final_volume: 0.0,
            base_panning: sample.default_panning as f32 / 64.0,
            envelope_panning: 0.0,
            final_panning: sample.default_panning as f32 / 64.0,
            fading: false,
            note_off: false,
            instrument_index: instrument,
            sample_index: Some(sample_idx),
            note,
            nna: instrument
                .and_then(|i| module.instruments.get(i as usize))
                .map(|inst| inst.nna)
                .unwrap_or(NewNoteAction::NoteCut),
            fade_out_rate: instrument
                .and_then(|i| module.instruments.get(i as usize))
                .map(|inst| inst.fade_out)
                .unwrap_or(0),
            // Initialize envelopes from instrument...
            vol_env: None,
            pan_env: None,
            pitch_env: None,
            // Initialize effect state...
            vibrato_phase: 0.0,
            vibrato_speed: 0,
            vibrato_depth: 0,
            vibrato_waveform: VibratoWaveform::Sine,
            tremolo_phase: 0.0,
            tremolo_speed: 0,
            tremolo_depth: 0,
            tremolo_waveform: VibratoWaveform::Sine,
            portamento_target: None,
            portamento_speed: 0.0,
            cutoff_tick: None,
        };
    }
}
```

### NNA (New Note Action) Handling

NNA is determined per-instrument (parsed from file for IT, and from extended XM
headers). Previously XM NNAs were hardcoded to `NoteCut`, causing premature sample
cutoff. Now NNA, DCT, and DCA are read from the XM extended header when available:

| XM Value | NNA Enum |
|----------|----------|
| 0 | NoteCut |
| 1 | Continue |
| 2 | NoteOff |
| 3 | NoteFade |

```rust
impl AudioEngine {
    fn handle_nna(&mut self, channel: usize, new_note: Note) {
        // Find existing voice on this channel
        let existing_voice = self.voices.iter_mut().find(|v| {
            v.active && !v.fading  // Only handle "primary" voice per channel
        });

        if let Some(voice) = existing_voice {
            match voice.nna {
                NewNoteAction::NoteCut => {
                    voice.active = false;
                }
                NewNoteAction::Continue => {
                    // Voice continues as-is, new voice also plays
                    // Mark this voice as "background" so it won't get NNAs again
                }
                NewNoteAction::NoteOff => {
                    voice.note_off = true;
                    voice.fading = true;
                    // Envelope enters release phase
                }
                NewNoteAction::NoteFade => {
                    voice.fading = true;
                    // Begin fade-out without envelope release
                }
            }
        }
    }
}
```

## Resampling

Each voice reads from its assigned sample buffer using fractional position indexing
and the selected interpolation algorithm.

### Interpolation Algorithms

#### Nearest Neighbor (fastest, most "lo-fi")

```rust
fn resample_nearest(data: &[f32], position: f64) -> f32 {
    let index = position.round() as usize;
    if index < data.len() {
        data[index]
    } else {
        0.0
    }
}
```

#### Linear Interpolation (good balance)

```rust
fn resample_linear(data: &[f32], position: f64) -> f32 {
    let index = position as usize;
    let frac = position - index as f64;

    if index + 1 < data.len() {
        data[index] * (1.0 - frac as f32) + data[index + 1] * frac as f32
    } else if index < data.len() {
        data[index]
    } else {
        0.0
    }
}
```

#### Cubic Spline Interpolation (highest quality)

Uses a 4-point cubic Hermite spline. Provides the smoothest interpolation
with minimal aliasing artifacts.

```rust
fn resample_cubic(data: &[f32], position: f64) -> f32 {
    let index = position as usize;
    let frac = (position - index as f64) as f32;

    // 4-point cubic Hermite (Catmull-Rom style)
    let y0 = if index > 0 { data[index - 1] } else { data[index] };
    let y1 = data.get(index).copied().unwrap_or(0.0);
    let y2 = data.get(index + 1).copied().unwrap_or(0.0);
    let y3 = data.get(index + 2).copied().unwrap_or(0.0);

    let a = (y1 - y0) * 0.5
          + (y2 - y1) * 0.5;
    let b = y0
          - y1 * 2.5
          + y2 * 2.0
          - y3 * 0.5;
    let c = (y3 - y0) * 0.5
          + (y1 - y2) * 1.5;

    // Horner's method evaluation
    let mut result = y1 + frac * (a + frac * (b + frac * c));

    // Clamp to prevent rare overshoot
    result = result.max(-1.0).min(1.0);
    result
}
```

### Mixer Sample Position Advancement

The mixer advances voice positions per-sample and handles loop wrapping with
**float-based boundary checks** (`voice.position >= loop_end as f64`) rather than
truncated integer checks (`voice.position as usize >= loop_end`). This avoids
off-by-loop-boundary errors when the fractional part of position approaches `loop_end`.

```rust
match loop_type {
    LoopType::Forward => {
        if loop_end > loop_start && voice.position >= loop_end as f64 {
            let loop_len = (loop_end - loop_start) as f64;
            voice.position = loop_start as f64
                + (voice.position - loop_start as f64) % loop_len;
        }
    }
    LoopType::PingPong => {
        if loop_end > loop_start {
            if voice.direction > 0.0 && voice.position >= loop_end as f64 {
                voice.position = 2.0 * loop_end as f64 - voice.position;
                voice.direction = -1.0;
            } else if voice.direction < 0.0 && voice.position < loop_start as f64 {
                voice.position = 2.0 * loop_start as f64 - voice.position;
                voice.direction = 1.0;
            }
        }
    }
    LoopType::Backward => {
        if loop_end > loop_start && voice.position < loop_start as f64 {
            let underflow = loop_start as f64 - voice.position;
            voice.position = (loop_end - 1) as f64 - underflow;
            // Safety clamp
            if voice.position < loop_start as f64 {
                voice.position = (loop_end - 1) as f64;
            }
        }
    }
    LoopType::None => {
        if voice.position as usize >= sample_data.len() || voice.position < 0.0 {
            voice.active = false;
        }
    }
}
```

## Envelope Processing

Envelopes are applied per-voice on each tick (not per sample). The envelope value
modifies the voice's volume, panning, or pitch.

```rust
impl Voice {
    fn advance_envelopes(&mut self) {
        if let Some(ref mut env_state) = self.vol_env {
            self.envelope_volume = advance_envelope(env_state);
        }
        if let Some(ref mut env_state) = self.pan_env {
            let val = advance_envelope(env_state);
            self.envelope_panning = (val - 0.5) * 2.0; // Map 0-1 → -1 to +1
        }
        if let Some(ref mut env_state) = self.pitch_env {
            let val = advance_envelope(env_state);
            // Pitch envelope: 0.5 = no change, 0.0 = -2 semitones, 1.0 = +2 semitones
            self.current_frequency *= 2.0_f64.powf((val - 0.5) * 4.0 / 12.0);
        }

        // Fade-out (applied after note-off or NoteFade NNA)
        if self.fading {
            self.fade_out_volume -= self.fade_out_rate as f32 / 4096.0;
            if self.fade_out_volume <= 0.0 {
                self.fade_out_volume = 0.0;
                self.active = false;
            }
        }
    }
}

fn advance_envelope(state: &mut EnvelopeState) -> f32 {
    let envelope = &state.envelope;

    if state.finished {
        return last_envelope_value(state);
    }

    let points = &envelope.points;

    // Advance position
    state.position += 1.0;

    // Check if we've passed the next point
    let next_tick = points.get(state.current_point + 1)
        .map(|p| p.tick as f32)
        .unwrap_or(f32::MAX);

    if state.position >= next_tick {
        state.current_point += 1;

        // Check for loop
        if envelope.flags.loop_ {
            if let Some(loop_end) = envelope.loop_end {
                if state.current_point >= loop_end {
                    if let Some(loop_start) = envelope.loop_start {
                        state.current_point = loop_start;
                        state.position = points[loop_start].tick as f32;
                    }
                }
            }
        }

        // Check for end
        if state.current_point >= points.len() - 1 {
            if state.released || !envelope.flags.sustain {
                state.finished = true;
            } else {
                // Hold at sustain point
                state.current_point = envelope.sustain_point
                    .unwrap_or(points.len() - 1);
            }
        }
    }

    // Interpolate between current point and next
    interpolate_envelope_value(points, state.current_point, state.position)
}

fn interpolate_envelope_value(
    points: &[EnvelopePoint],
    current: usize,
    position: f32,
) -> f32 {
    if current >= points.len() {
        return points.last().map(|p| p.value as f32 / 64.0).unwrap_or(1.0);
    }

    let p0 = points[current];
    let p1 = points.get(current + 1).copied().unwrap_or(p0);

    if p0.tick == p1.tick {
        return p0.value as f32 / 64.0;
    }

    let t = (position - p0.tick as f32) / (p1.tick - p0.tick) as f32;
    let t = t.max(0.0).min(1.0);

    let v0 = p0.value as f32 / 64.0;
    let v1 = p1.value as f32 / 64.0;

    v0 + (v1 - v0) * t
}
```

## Mixer

```rust
// mixer.rs

impl AudioEngine {
    fn mix_voices(&mut self) -> (f32, f32) {
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            // Compute final volume
            voice.final_volume = voice.base_volume
                * voice.envelope_volume
                * voice.channel_volume
                * voice.global_volume
                * voice.fade_out_volume
                * (1.0 + voice.tremolo_volume);

            if voice.final_volume.abs() < 1e-6 {
                continue;
            }

            // Resample
            let sample_value = match self.interpolation {
                InterpolationType::Cubic => {
                    resample_cubic(voice.sample.as_ref().unwrap(), voice.position)
                }
                InterpolationType::Linear => {
                    resample_linear(voice.sample.as_ref().unwrap(), voice.position)
                }
                InterpolationType::Nearest => {
                    resample_nearest(voice.sample.as_ref().unwrap(), voice.position)
                }
            };

            // Apply volume
            let output = sample_value * voice.final_volume;

            // Apply panning (constant power panning)
            let pan = voice.base_panning + voice.envelope_panning;
            let pan = pan.max(0.0).min(1.0);

            // Constant power: left = cos(θ), right = sin(θ)
            // θ = pan * π/2
            let angle = pan * std::f32::consts::FRAC_PI_2;
            left += output * angle.cos();
            right += output * angle.sin();

            // Advance position by one sample
            voice.advance_position(1);
        }

        (left, right)
    }
}
```

### Per-Sample Gain Ramping (v0.27.0)

The pseudocode above shows `output = sample_value * voice.final_volume` with a
flat gain per buffer. The actual mixer now **ramps** the per-voice gain
per-sample rather than holding it flat across the tick chunk:

- `Voice.smoothed_volume` / `smoothed_panning` are the per-sample ramp
  position, advanced each sample toward `final_volume` / `final_panning` by a
  `GainRamp` helper. A linear step sized to reach the target exactly at chunk
  end means there is zero discontinuity at the next tick seam.
- **Note-onset anti-click**: `smoothed_volume` starts at `0.0` on `trigger`, so
  a fresh voice fades in over at most `ONSET_RAMP_SAMPLES` (64 samples / ≈1.3 ms
  @48k) regardless of chunk length. After the onset, `smoothed_volume ≈
  final_volume` and subsequent chunks do gentle inter-tick smoothing.
- The step is clamped to the target so fractional steps never overshoot (in
  either direction — handles rising and falling volume slides).
- Gated by `Voice.ramp_enabled`, set from `SequencerEngine.ramp_enabled` at
  trigger (mirrors `amiga_led_filter`). When `false`, the mixer applies the
  flat per-chunk gain shown in the pseudocode above (bit-exact legacy mode).
  Default ON for all formats, toggleable via `AppConfig.anti_click_ramping`.

This is purely a mixer concern: the effect/envelope engine still computes
`final_volume` once per tick — only its *rendering* is smoothed.

### Constant Power Panning Law

Standard linear panning creates a "hole in the middle" effect. Constant power
panning maintains equal perceived loudness across the stereo field:

```
Pan Value | Left (cos) | Right (sin) | Perceived Position
----------|------------|-------------|-------------------
   0.0    |   1.000    |   0.000     | Full Left
   0.25   |   0.924    |   0.383     | Left-Center
   0.5    |   0.707    |   0.707     | Center (-3dB each)
   0.75   |   0.383    |   0.924     | Right-Center
   1.0    |   0.000    |   1.000     | Full Right
```

## Global Effects

A simple brick-wall limiter to prevent clipping when many voices play simultaneously.

```rust
impl AudioEngine {
    fn apply_global_effects(&mut self, output: &mut [f32]) {
        // Simple brick-wall limiter
        // Any sample exceeding 1.0 is hard-clamped
        // A more sophisticated approach would use a lookahead limiter
        for sample in output.iter_mut() {
            if *sample > 1.0 {
                *sample = 1.0;
            } else if *sample < -1.0 {
                *sample = -1.0;
            }
        }
    }
}
```

### Future: Soft-Knee Limiter (Phase 8)

```rust
struct Limiter {
    threshold: f32,      // e.g., 0.95
    ceiling: f32,        // e.g., 1.0
    attack: f32,         // seconds
    release: f32,        // seconds
    envelope: f32,       // current gain reduction
}

impl Limiter {
    fn process(&mut self, samples: &mut [f32], sample_rate: f32) {
        for frame in samples.chunks_exact_mut(2) {
            let peak = frame[0].abs().max(frame[1].abs());
            let gain = if peak > self.threshold {
                self.ceiling / peak
            } else {
                1.0
            };

            // Smooth gain reduction
            let attack_coeff = (1.0 - (-1.0 / (self.attack * sample_rate)).exp());
            let release_coeff = (1.0 - (-1.0 / (self.release * sample_rate)).exp());

            if gain < self.envelope {
                self.envelope += (gain - self.envelope) * attack_coeff;
            } else {
                self.envelope += (gain - self.envelope) * release_coeff;
            }

            frame[0] *= self.envelope;
            frame[1] *= self.envelope;
        }
    }
}
```

## Pitch Calculations

### Note to Frequency

Standard equal temperament: `freq = 440 * 2^((note - 69) / 12)`

| Note | MIDI# | Frequency |
|------|-------|-----------|
| C-0  |   0   |   8.18 Hz |
| A-4  |  69   | 440.00 Hz |
| C-5  |  72   | 523.25 Hz |
| C-8  | 108   | 8372.02 Hz |

### Sample Delta (Playback Rate)

The sample delta determines how many source samples to advance per output sample:

```
sample_delta = desired_frequency / output_sample_rate
```

Where `desired_frequency` = note frequency * (relative_note correction) * (pitch effects)

### IT Linear Frequency Mode

In IT format with linear slides enabled (most common mode):

```
frequency = period_to_frequency(period)
period = 10 * 12 * 16 * 4 - note * 16 * 4 + finetune / 2
```

This gives a linear mapping where each semitone = 64 period units.

### Amiga Frequency Mode (MOD/S3M)

```
frequency = 8363 * 1712 / period
```

Where period comes from the period table (see data-model.md).

### XM Frequency Mode

For XM with `LINEAR_FREQUENCIES` disabled (Amiga mode):

```
frequency = 8363 * 1712 / period
```

For XM with `LINEAR_FREQUENCIES` enabled:

```
logTab[x] = floor(65536 * 2^(x / 768))
invPeriod = 7680 - period
quotient = invPeriod / 768
remainder = invPeriod % 768
frequency = logTab[remainder] * 8363 / 65536 * 2^(5 - quotient)
```

## Reference Implementation

The XM playback accuracy work is based on the `ft2play` reference implementation
(in the `ft2play-main/` directory of this repository), which replicates FastTracker 2
version 2.08/2.09 behavior. Key differences corrected by this work:

| Area | Before | After |
|------|--------|-------|
| Pitch system | MIDI frequency (all formats) | Period-based tables for XM |
| Portamento | Exponential frequency slide | Linear period slide (`speed*4`) |
| Vibrato | 64-entry table, frequency multiplier | 32-entry FT2 `vibTab`, period additive |
| Tremolo ramp | Uses correct `tremPos` sign | Uses `vibPos` (FT2 bug) |
| Fadeout | float 0..1, divisor 4096 | integer 0..32768, divisor from header |
| Envelope timing | position=0 on trigger | position=-1 (counter 65535 wraparound) |
| Auto-vibrato | not implemented | Full implementation with sweep |
| LINEAR_FREQUENCIES flag | stored but ignored | respected for period/frequency lookup |
