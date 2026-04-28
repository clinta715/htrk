# Testing Strategy

## Overview

Testing a tracker requires verifying: correct file parsing, accurate audio playback,
proper effect processing, and reliable editing operations. Our approach combines unit
tests, integration tests, and comparison testing against reference implementations.

## Test Categories

### 1. Unit Tests

Small, focused tests within each module using `#[cfg(test)]` and `#[test]`.

#### Data Model Tests

```rust
// sequencer/note.rs tests
#[test]
fn note_frequency_middle_a() {
    let note = Note::On(69); // A-4
    assert!((note.frequency().unwrap() - 440.0).abs() < 0.01);
}

#[test]
fn note_frequency_middle_c() {
    let note = Note::On(60); // C-4
    assert!((note.frequency().unwrap() - 261.63).abs() < 0.01);
}

#[test]
fn note_from_tone_octave() {
    let note = Note::from_tone_octave(0, 4); // C-4
    assert_eq!(note, Note::On(60));
}

#[test]
fn note_display() {
    assert_eq!(Note::On(60).display(), "C-4");
    assert_eq!(Note::On(61).display(), "C#4");
    assert_eq!(Note::On(72).display(), "C-5");
    assert_eq!(Note::None.display(), "---");
    assert_eq!(Note::Off.display(), "^^^");
    assert_eq!(Note::Cut.display(), "===");
}

#[test]
fn cell_is_empty() {
    assert!(Cell::default().is_empty());
    assert!(!Cell { note: Note::On(60), ..Default::default() }.is_empty());
}

#[test]
fn pattern_resize() {
    let mut p = Pattern::new(64);
    assert_eq!(p.num_rows, 64);
    p.resize_rows(128);
    assert_eq!(p.num_rows, 128);
    assert_eq!(p.data.len(), 128);
}
```

#### Resampler Tests

```rust
// audio/resampler.rs tests

#[test]
fn nearest_neighbor_basic() {
    let data = vec![0.0, 0.5, 1.0, 0.5, 0.0];
    assert_eq!(resample_nearest(&data, 0.0), 0.0);
    assert_eq!(resample_nearest(&data, 1.0), 0.5);
    assert_eq!(resample_nearest(&data, 2.0), 1.0);
    assert_eq!(resample_nearest(&data, 4.0), 0.0);
}

#[test]
fn nearest_neighbor_out_of_bounds() {
    let data = vec![0.5, 0.5];
    assert_eq!(resample_nearest(&data, 5.0), 0.0);
}

#[test]
fn linear_midpoint() {
    let data = vec![0.0, 1.0];
    let result = resample_linear(&data, 0.5);
    assert!((result - 0.5).abs() < 0.001);
}

#[test]
fn linear_quarter() {
    let data = vec![0.0, 1.0];
    let result = resample_linear(&data, 0.25);
    assert!((result - 0.25).abs() < 0.001);
}

#[test]
fn cubic_matches_known_output() {
    // Generate a sine wave and verify cubic interpolation is accurate
    let data: Vec<f32> = (0..100).map(|i| (i as f32 * 0.1).sin()).collect();

    // Cubic interpolation at integer positions should match original
    for i in 1..99 {
        let result = resample_cubic(&data, i as f64);
        assert!((result - data[i]).abs() < 0.001,
            "Cubic mismatch at {}: expected {}, got {}",
            i, data[i], result);
    }
}

#[test]
fn cubic_smoother_than_linear() {
    let data = vec![0.0, 0.0, 1.0, 0.0, 0.0];

    // At the discontinuity (position 1.5), cubic should give smoother result
    let linear_val = resample_linear(&data, 1.5);
    let cubic_val = resample_cubic(&data, 1.5);

    // Both should be positive, but cubic should handle the transition better
    assert!(linear_val > 0.0);
    assert!(cubic_val > 0.0);
}
```

#### Pitch Calculation Tests

```rust
#[test]
fn frequency_to_period_roundtrip() {
    let freq = 440.0;
    let period = frequency_to_period(freq);
    let freq_back = period_to_frequency(period);
    assert!((freq - freq_back).abs() < 0.01);
}

#[test]
fn linear_slide_rate() {
    // At 125 BPM, speed 6: samples_per_tick should be 960 at 48kHz
    let samples_per_tick = (48000.0 * 5.0) / (125.0 * 2.0);
    assert!((samples_per_tick - 960.0).abs() < 0.01);
}

#[test]
fn note_to_sample_delta() {
    // A-4 (440 Hz) at 44100 Hz output → delta = 440/44100 ≈ 0.00998
    let delta = 440.0 / 44100.0;
    assert!((delta - 0.00998).abs() < 0.001);
}
```

### 2. Format Parsing Tests

#### IT Format Tests

```rust
#[test]
fn it_detect_magic() {
    let data = b"IMPM\x00\x00..."; // minimal header
    assert_eq!(detect_format(data), Some(ModuleFormat::IT));
}

#[test]
fn it_header_parse() {
    let data = include_bytes!("../tests/resources/simple.it");
    let module = ItFormat.load(data).unwrap();
    assert_eq!(module.name, "Test Song");
    assert_eq!(module.initial_bpm, 125);
    assert_eq!(module.initial_speed, 6);
    assert!(module.flags.linear_slides);
}

#[test]
fn it_sample_count() {
    let data = include_bytes!("../tests/resources/multisamp.it");
    let module = ItFormat.load(data).unwrap();
    assert_eq!(module.samples.len(), 5); // including index 0 placeholder
}

#[test]
fn it_pattern_rows() {
    let data = include_bytes!("../tests/resources/patterns.it");
    let module = ItFormat.load(data).unwrap();
    assert_eq!(module.patterns[0].num_rows, 64);
    assert_eq!(module.patterns[1].num_rows, 128); // extended pattern
}

#[test]
fn it_sample_data_length() {
    let data = include_bytes!("../tests/resources/samples.it");
    let module = ItFormat.load(data).unwrap();
    assert_eq!(module.samples[1].data.len(), 16384);
}

#[test]
fn it_compressed_sample() {
    let data = include_bytes!("../tests/resources/compressed.it");
    let module = ItFormat.load(data).unwrap();
    // Verify compressed sample decompresses to expected length
    assert_eq!(module.samples[1].data.len(), 8192);
    // Verify first few samples match expected values
    assert!((module.samples[1].data[0] - 0.0).abs() < 0.01);
}

#[test]
fn it_envelope_parse() {
    let data = include_bytes!("../tests/resources/envelopes.it");
    let module = ItFormat.load(data).unwrap();
    let env = module.instruments[1].volume_envelope.as_ref().unwrap();
    assert!(env.flags.enabled);
    assert_eq!(env.points.len(), 4);
    assert_eq!(env.sustain_point, Some(1));
}

#[test]
fn it_roundtrip() {
    let original_data = include_bytes!("../tests/resources/roundtrip.it");
    let module = ItFormat.load(original_data).unwrap();
    let saved_data = ItFormat.save(&module).unwrap();
    let module2 = ItFormat.load(&saved_data).unwrap();

    // Compare key fields
    assert_eq!(module.name, module2.name);
    assert_eq!(module.initial_bpm, module2.initial_bpm);
    assert_eq!(module.order_list, module2.order_list);
    assert_eq!(module.patterns.len(), module2.patterns.len());
    assert_eq!(module.samples.len(), module2.samples.len());
}
```

#### MOD Format Tests

```rust
#[test]
fn mod_detect_magic() {
    let mut data = vec![0u8; 1084];
    data[1080..1084].copy_from_slice(b"M.K.");
    assert_eq!(detect_format(&data), Some(ModuleFormat::MOD));
}

#[test]
fn mod_4_channels() {
    let data = include_bytes!("../tests/resources/test.mod");
    let module = ModFormat.load(data).unwrap();
    // MOD always has 4 channels
    assert_eq!(module.patterns[0].data[0].len(), 64); // MAX_CHANNELS, only first 4 used
}

#[test]
fn mod_note_decode() {
    // Period 428 = C-3 (from period table)
    let note = period_to_note(428);
    assert_eq!(note, Note::On(36)); // C-3 = MIDI 36
}
```

### 3. Sequencer/Playback Tests

These test the sequencer logic without actual audio output.

```rust
#[test]
fn tick_advancement() {
    let mut state = SequencerState::new();
    state.bpm = 125;
    state.speed = 6;
    state.samples_per_tick = 960.0;

    // At 960 samples per tick, counter should trigger after 960 samples
    for _ in 0..959 {
        state.advance_sample();
    }
    assert_eq!(state.current_tick, 0); // Not yet

    state.advance_sample(); // 960th sample
    assert_eq!(state.current_tick, 1);
}

#[test]
fn row_advancement_after_speed_ticks() {
    let mut state = SequencerState::new();
    state.speed = 3;
    state.current_tick = 0;
    state.current_row = 0;

    // Tick 0, 1, 2, then advance
    for _ in 0..3 {
        state.advance_tick();
    }
    assert_eq!(state.current_row, 1);
    assert_eq!(state.current_tick, 0);
}

#[test]
fn effect_memory() {
    let mut ch = ChannelState::default();
    let mut effect = Effect::PortamentoUp { speed: 0 };
    ch.apply_effect_memory(&mut effect);
    assert_eq!(ch.last_portamento_up_speed, 0); // No memory yet

    let mut effect = Effect::PortamentoUp { speed: 5 };
    ch.apply_effect_memory(&mut effect);
    assert_eq!(ch.last_portamento_up_speed, 5);

    let mut effect = Effect::PortamentoUp { speed: 0 };
    ch.apply_effect_memory(&mut effect);
    assert_eq!(speed, 5); // Memory recalled
}

#[test]
fn volume_slide_up() {
    let mut ch = ChannelState::default();
    ch.channel_volume = 32;
    ch.last_volume_slide_up = 4;

    process_tick_volume_slide(&mut ch);
    assert_eq!(ch.channel_volume, 36);
}

#[test]
fn volume_slide_clamp() {
    let mut ch = ChannelState::default();
    ch.channel_volume = 62;
    ch.last_volume_slide_up = 4;

    process_tick_volume_slide(&mut ch);
    assert_eq!(ch.channel_volume, 64); // Clamped to max
}

#[test]
fn arpeggio_cycle() {
    // Arpeggio 0xy cycles through: base, base+x, base+y
    let base_key = 60; // C-4
    let (n1, n2) = (4, 7); // Major chord: C, E, G

    let tick0 = arpeggio_note(base_key, 0, n1, n2);
    let tick1 = arpeggio_note(base_key, 1, n1, n2);
    let tick2 = arpeggio_note(base_key, 2, n1, n2);

    assert_eq!(tick0, 60); // C-4
    assert_eq!(tick1, 64); // E-4
    assert_eq!(tick2, 67); // G-4
}

#[test]
fn position_jump() {
    let mut state = SequencerState::new();
    state.current_order = 3;
    state.current_row = 32;

    // Bxx: jump to order 0
    state.position_jump_order = Some(0);
    state.advance_row(&module);

    assert_eq!(state.current_order, 0);
    assert_eq!(state.current_row, 0);
}

#[test]
fn pattern_break() {
    let mut state = SequencerState::new();
    state.current_order = 0;
    state.current_row = 63;

    // Dxx: break to row 16 of next pattern
    state.pattern_break_row = Some(16);
    state.advance_row(&module);

    assert_eq!(state.current_order, 1);
    assert_eq!(state.current_row, 16);
}

#[test]
fn envelope_advancement() {
    let envelope = Envelope {
        points: vec![
            EnvelopePoint { tick: 0, value: 0 },
            EnvelopePoint { tick: 10, value: 64 },
            EnvelopePoint { tick: 20, value: 32 },
        ],
        sustain_point: Some(1),
        loop_start: None,
        loop_end: None,
        flags: EnvelopeFlags { enabled: true, sustain: true, ..Default::default() },
    };

    let mut state = EnvelopeState::new(&envelope);

    // At tick 5, should be halfway between point 0 and point 1
    state.advance();
    // ... (5 advances)
    let value = interpolate_envelope_value(&envelope.points, 0, 5.0);
    assert!((value - 0.5).abs() < 0.1); // ~32/64
}
```

### 4. Edit System Tests

```rust
#[test]
fn undo_redo_basic() {
    let mut module = Module::default();
    let mut undo = UndoManager::new(100);

    let cmd = SetCellCommand {
        order: 0, row: 0, channel: 0,
        old_cell: Cell::default(),
        new_cell: Cell { note: Note::On(60), ..Default::default() },
    };

    undo.execute(Box::new(cmd), &mut module).unwrap();
    assert_eq!(module.patterns[0].cell(0, 0).note, Note::On(60));

    undo.undo(&mut module).unwrap();
    assert_eq!(module.patterns[0].cell(0, 0).note, Note::None);

    undo.redo(&mut module).unwrap();
    assert_eq!(module.patterns[0].cell(0, 0).note, Note::On(60));
}

#[test]
fn undo_max_depth() {
    let mut module = Module::default();
    let mut undo = UndoManager::new(3);

    for i in 0..5 {
        let cmd = SetCellCommand { /* ... */ };
        undo.execute(Box::new(cmd), &mut module).unwrap();
    }

    assert_eq!(undo.undo_count(), 3); // Only last 3 are remembered
}

#[test]
fn insert_row_pushes_data_down() {
    let mut module = create_test_module(64);
    module.patterns[0].data[0][0].note = Note::On(60);
    module.patterns[0].data[1][0].note = Note::On(62);

    let cmd = InsertRowCommand { pattern_index: 0, row: 0, channel: None };
    cmd.execute(&mut module).unwrap();

    assert_eq!(module.patterns[0].data[0][0].note, Note::None); // Inserted empty
    assert_eq!(module.patterns[0].data[1][0].note, Note::On(60)); // Pushed down
    assert_eq!(module.patterns[0].data[2][0].note, Note::On(62)); // Pushed down
    // Last row should be lost (pushed off the end)
}
```

### 5. Integration Tests

Full end-to-end tests in `tests/` directory.

#### Playback Integration

```rust
#[test]
fn play_simple_module() {
    // Load a known module
    let data = include_bytes!("resources/simple.it");
    let module = ItFormat.load(data).unwrap();

    // Create audio engine with mock output
    let mut engine = AudioEngine::new(44100);
    engine.load_module(Arc::new(module));

    // Simulate playing through the entire song
    engine.play();

    let mut total_samples = 0;
    let mut output = vec![0.0f32; 512];
    let max_samples = 44100 * 60 * 5; // 5 minutes max

    while engine.is_playing() && total_samples < max_samples {
        engine.process_callback(&mut output);
        total_samples += 256;
    }

    // Verify the song completed
    assert!(!engine.is_playing());
    // Verify non-silent output was produced
    assert!(output.iter().any(|&s| s.abs() > 0.001));
}
```

#### Comparison Testing

Compare htrk's output against a reference implementation (e.g., libopenmpt):

```rust
#[test]
fn compare_with_reference() {
    let data = include_bytes!("resources/test.it");

    // Load with htrk
    let htrk_module = ItFormat.load(data).unwrap();

    // Render with htrk
    let htrk_output = render_to_buffer(&htrk_module, 44100);

    // Load reference rendering (pre-rendered WAV from libopenmpt)
    let reference = load_reference_wav("resources/test_reference.wav");

    // Compare RMS levels (exact sample comparison is too strict)
    let htrk_rms = compute_rms(&htrk_output);
    let ref_rms = compute_rms(&reference);

    assert!((htrk_rms - ref_rms).abs() / ref_rms < 0.1,
        "RMS difference too large: htrk={}, ref={}", htrk_rms, ref_rms);

    // Compare spectral content (FFT)
    let htrk_spectrum = compute_spectrum(&htrk_output);
    let ref_spectrum = compute_spectrum(&reference);
    let spectral_diff = compute_spectral_difference(&htrk_spectrum, &ref_spectrum);

    assert!(spectral_diff < 0.05,
        "Spectral difference too large: {}", spectral_diff);
}
```

## Test Resources

### Required Test Files

Place in `tests/resources/`:

| File | Purpose |
|------|---------|
| `simple.it` | Minimal IT: 1 pattern, 1 sample, no effects |
| `effects.it` | Tests all major effects (portamento, vibrato, tremolo, etc.) |
| `envelopes.it` | Volume, panning, and pitch envelopes with sustain/loop |
| `nna.it` | All 4 NNA types with overlapping notes |
| `compressed.it` | IT214 and IT215 compressed samples |
| `multisamp.it` | Multiple samples with instrument sample mapping |
| `long.it` | Multi-pattern song with order jumps and pattern breaks |
| `roundtrip.it` | Used for save/load round-trip testing |
| `test.mod` | Simple 4-channel MOD file |
| `test.s3m` | S3M file with stereo panning |
| `test.xm` | XM file with envelopes and key mapping |
| `*_reference.wav` | Reference renderings from libopenmpt for comparison |

### Test File Sources

- Use demo modules from the ModArchive (https://modarchive.org)
- Generate synthetic test files programmatically for edge cases
- Use files from the Schism Tracker test suite (GPL-licensed, compatible)
- Create minimal files manually with a hex editor for specific format features

## Test Execution

### Running Tests

```bash
# All tests
cargo test

# Unit tests only
cargo test --lib

# Integration tests
cargo test --test '*'

# Format tests only
cargo test formats::

# With output
cargo test -- --nocapture

# Specific test
cargo test it_header_parse
```

### Continuous Integration

```yaml
# .github/workflows/test.yml
name: Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [windows-latest, macos-latest, ubuntu-latest]
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
      - run: cargo test --test integration
```

## Coverage Targets

| Module | Target Coverage | Priority |
|--------|----------------|----------|
| Format parsers (IT, XM, S3M, MOD) | 90%+ | Critical |
| Resampler | 95%+ | Critical |
| Sequencer tick/row logic | 90%+ | Critical |
| Effect processing | 85%+ | High |
| Envelope processing | 90%+ | High |
| Voice management | 85%+ | High |
| Edit commands | 90%+ | High |
| UI widgets | 50%+ | Medium (visual testing) |
| Audio device | 30%+ | Low (hardware dependent) |

## Debugging Tools

### Audio Output Dump

```rust
// Debug: write audio output to WAV for analysis
#[cfg(debug_assertions)]
fn dump_audio_to_wav(output: &[f32], path: &str) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for sample in output {
        writer.write_sample(*sample).unwrap();
    }
}
```

### Sequencer State Trace

```rust
#[cfg(debug_assertions)]
fn trace_sequencer(state: &SequencerState) {
    eprintln!(
        "ord={:03} row={:03} tick={}/{} pat={:03} bpm={} spd={}",
        state.current_order,
        state.current_row,
        state.current_tick,
        state.speed,
        state.current_pattern,
        state.bpm,
        state.speed,
    );
}
```

### Pattern Visualizer

For debugging pattern parsing without audio:

```rust
fn print_pattern(pattern: &Pattern, channels: usize) {
    for row in 0..pattern.num_rows {
        print!("{:03} │ ", row);
        for ch in 0..channels {
            let cell = pattern.cell(row, ch);
            print!("{} ", cell.display());
        }
        println!();
    }
}
```
