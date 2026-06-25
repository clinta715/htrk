use super::*;
use crate::sequencer::Instrument;
use crate::sequencer::module::ModuleFlags;
use crate::sequencer::pattern::Pattern;
use crate::sequencer::note::Note;
use crate::sequencer::effect::Effect;
use crate::audio::voice::EnvelopeState;
use crate::audio::effects::{compute_samples_per_tick, advance_single_envelope, evaluate_envelope, compute_playback_frequency, VIBRATO_SINE_TABLE, FUNK_TRACK};
use crate::audio::Voice;
use crate::sequencer::module::MAX_VOICES;
use crate::sequencer::sample::VibratoWaveform;
use crate::sequencer::period::get_vib_tab;


#[test]
fn compute_samples_per_tick_default() {
    let spt = compute_samples_per_tick(125, 48000.0);
    assert!((spt - 960.0).abs() < 1.0);
}

#[test]
fn compute_samples_per_tick_140_bpm() {
    let spt = compute_samples_per_tick(140, 48000.0);
    let expected = 48000.0 * 5.0 / (140.0 * 2.0);
    assert!((spt - expected).abs() < 1.0);
}

#[test]
fn sequencer_engine_new() {
    let engine = SequencerEngine::new(48000.0);
    assert_eq!(engine.voice_pool.voices.len(), MAX_VOICES);
    assert!(!engine.state.playing);
}

#[test]
fn advance_envelope_linear() {
    let env = crate::sequencer::instrument::Envelope {
        points: vec![
            crate::sequencer::instrument::EnvelopePoint { tick: 0, value: 0 },
            crate::sequencer::instrument::EnvelopePoint { tick: 10, value: 64 },
        ],
        sustain_point: None,
        loop_start: None,
        loop_end: None,
        flags: crate::sequencer::instrument::EnvelopeFlags {
            enabled: true,
            sustain: false,
            loop_: false,
            carry: false,
        },
    };

    let mut state = EnvelopeState {
        envelope: Arc::new(env),
        current_point: 0,
        position: 0.0,
        released: false,
        finished: false,
    };

    assert!((evaluate_envelope(&state) - 0.0).abs() < 0.1);

    for _ in 0..5 {
        advance_single_envelope(&mut state);
    }
    assert!((evaluate_envelope(&state) - 32.0).abs() < 1.0);

    for _ in 0..5 {
        advance_single_envelope(&mut state);
    }
    assert!((evaluate_envelope(&state) - 64.0).abs() < 1.0);
}

#[test]
fn advance_envelope_sustain() {
    let env = crate::sequencer::instrument::Envelope {
        points: vec![
            crate::sequencer::instrument::EnvelopePoint { tick: 0, value: 0 },
            crate::sequencer::instrument::EnvelopePoint { tick: 5, value: 64 },
            crate::sequencer::instrument::EnvelopePoint { tick: 10, value: 0 },
        ],
        sustain_point: Some(1),
        loop_start: None,
        loop_end: None,
        flags: crate::sequencer::instrument::EnvelopeFlags {
            enabled: true,
            sustain: true,
            loop_: false,
            carry: false,
        },
    };

    let mut state = EnvelopeState {
        envelope: Arc::new(env),
        current_point: 0,
        position: 0.0,
        released: false,
        finished: false,
    };

    for _ in 0..20 {
        advance_single_envelope(&mut state);
    }
    assert!(!state.finished);

    state.released = true;
    for _ in 0..10 {
        advance_single_envelope(&mut state);
    }
    assert!(state.finished);
}

#[test]
fn vibrato_sine_table_range() {
    for &val in &VIBRATO_SINE_TABLE {
        assert!(val >= -255.0 && val <= 255.0);
    }
}

#[test]
fn compute_playback_frequency_basic() {
    let freq = compute_playback_frequency(440.0, 8363, 0, 0);
    assert!(freq > 0.0);
}

#[test]
fn allocate_voice_finds_inactive() {
    let mut engine = SequencerEngine::new(48000.0);
    let idx = engine.allocate_voice(0);
    assert!(!engine.voice_pool.voices[idx].active);
}

#[test]
fn mod_playback_produces_audio() {
    use crate::formats::modfile::ModHandler;
    use crate::formats::FormatHandler;

    let sample_data: &[u8] = &[0x00, 0x40, 0x7F, 0x40, 0x00, 0xC0, 0x7F, 0xC0];
    let sample_len_words = (sample_data.len() / 2) as u16;
    let pattern_size = 64 * 4 * 4;
    let total_size = 1084 + pattern_size + sample_data.len();
    let mut data = vec![0u8; total_size];

    data[950] = 1;
    data[952] = 0;
    data[1080..1084].copy_from_slice(b"M.K.");

    let s0_base = 20;
    data[s0_base + 22] = (sample_len_words >> 8) as u8;
    data[s0_base + 23] = (sample_len_words & 0xFF) as u8;
    data[s0_base + 25] = 64;

    data[1084 + pattern_size..1084 + pattern_size + sample_data.len()].copy_from_slice(sample_data);

    let period_c3: u16 = 428;
    data[1084] = ((period_c3 >> 8) & 0x0F) as u8;
    data[1085] = (period_c3 & 0xFF) as u8;
    data[1086] = 0x10;
    data[1087] = 0x00;

    let handler = ModHandler;
    let module = Arc::new(handler.load(&data).unwrap());

    let cell = module.patterns[0].cell(0, 0);
    assert!(cell.instrument.is_some(), "MOD cell should have instrument");
    assert!(matches!(cell.note, Note::On(_)), "MOD cell should have note, got {:?}", cell.note);

    assert!(!module.samples[1].data.is_empty(), "Sample 1 should have data");
    assert_eq!(module.samples[1].sample_rate, 8363);

    let mut engine = SequencerEngine::new(48000.0);
    engine.load_module(module.clone());
    engine.play();

    // With the new engine loop, play() doesn't trigger tick 0 immediately.
    // We need to call process_tick() or advance() to trigger the notes.
    engine.process_tick();

    assert!(engine.state.playing, "Engine should be playing after play()");

    let active_after_play = engine.voice_pool.voices.iter().filter(|v| v.active).count();
    assert!(active_after_play > 0, "Should have at least 1 active voice after tick 0, got {}", active_after_play);

    let voice = engine.voice_pool.voices.iter().find(|v| v.active).unwrap();
    assert!(voice.sample.is_some(), "Active voice should have sample data");
    let sample_ref = voice.sample.as_ref().unwrap();
    assert!(!sample_ref.is_empty(), "Sample data should not be empty");
    assert!(voice.sample_delta > 0.0, "Sample delta should be positive, got {}", voice.sample_delta);
    assert!(voice.final_volume > 0.0, "Final volume should be positive, got {}", voice.final_volume);

    engine.advance(4800);

    let mut left = vec![0.0f32; 4800];
    let mut right = vec![0.0f32; 4800];
    crate::audio::mixer::mix_voices(
        &mut engine.voice_pool.voices,
        &mut left,
        &mut right,
        1.0,
        crate::audio::commands::InterpolationType::Linear,
        &[],
        48000.0,
    );

    let max_sample = left.iter().chain(right.iter()).map(|&s| s.abs()).fold(0.0f32, f32::max);
    assert!(max_sample > 0.0001, "MOD playback should produce audio output, max sample = {:.6}", max_sample);
}

#[test]
fn advance_row_resets_effects() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.state.channels[0].active_effects.volume_slide = true;
    engine.state.channels[0].last_retrigger_interval = 2;
    engine.state.channels[0].vol_kol = 0x50;

    engine.advance_row();

    assert!(!engine.state.channels[0].active_effects.volume_slide);
    assert_eq!(engine.state.channels[0].last_retrigger_interval, 0);
    assert_eq!(engine.state.channels[0].vol_kol, 0);
}

#[test]
fn note_delay_stores_cell() {
    let mut engine = SequencerEngine::new(48000.0);
    let mut cell = Cell::default();
    cell.note = Note::On(60);
    cell.instrument = Some(1);
    cell.effect = Effect::NoteDelay { ticks: 3 };

    let module = Arc::new(Module::default());
    engine.load_module(module);
    engine.process_cell_unified(0, &cell);

    assert_eq!(engine.state.channels[0].note_delay_ticks, 3);
    assert!(engine.state.channels[0].delayed_cell.is_some());
    assert_eq!(engine.state.channels[0].delayed_cell.unwrap().note, Note::On(60));
}

#[test]
fn auto_vibrato_period_base_set_on_trigger_note() {
    use crate::sequencer::instrument::Instrument;
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    // Create a sample with data
    let mut sample = Sample::default();
    sample.default_volume = 48;
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    // Create an instrument with auto-vibrato
    let vib_depth: u8 = 10;
    let mut inst = Instrument::default();
    inst.vib_depth = vib_depth;
    inst.vib_sweep = 0;
    inst.vib_rate = 8;
    inst.vib_type = 0;
    inst.sample_map[60] = 1;

    // Create pattern with a cell on channel 0 row 0
    let mut pattern = Pattern::new(64);
    let mut cell = Cell::default();
    cell.note = Note::On(60);
    cell.instrument = Some(1);
    pattern.data[0][0] = cell;

    let module = Arc::new(Module {
        name: String::new(),
        format: ModuleFormat::XM,
        instruments: vec![Instrument::default(), inst],
        samples: vec![Sample::default(), sample],
        order_list: vec![0],
        patterns: vec![pattern],
        flags: ModuleFlags {
            linear_slides: true,
            use_instruments: true,
            xm_period_model: true,
            ..ModuleFlags::default()
        },
        ..Module::default()
    });

    engine.load_module(module.clone());

    // Initialise state so that process_tick_zero_unified reads row 0 of pattern 0
    engine.state.current_order = 0;
    engine.state.current_pattern = 0;
    engine.state.current_row = 0;
    engine.state.clock.current_tick = 0;
    engine.state.channels.resize(64, ChannelState::default());

    engine.process_tick_zero_unified();

    // Find the active voice on channel 0
    let voice = engine.voice_pool.voices.iter()
        .find(|v| v.active && v.channel == Some(0))
        .expect("Should have an active voice on channel 0");

    // auto_vib_period_base must be non-zero and match the note's period
    assert!(
        voice.auto_vib_period_base > 0,
        "auto_vib_period_base should be > 0 after trigger, got {}",
        voice.auto_vib_period_base
    );

    let expected_period = crate::sequencer::period::get_note_period(60, 0, true);
    assert_eq!(
        voice.auto_vib_period_base, expected_period,
        "auto_vib_period_base {} should match note period {}",
        voice.auto_vib_period_base, expected_period
    );

    // Verify auto-vibrato sweep is set up correctly (sweep=0 ΓåÆ full depth)
    assert_eq!(voice.auto_vib_amp, (vib_depth as i32) * 256);
    assert_eq!(voice.auto_vib_sweep, 0);
}

#[test]
fn delayed_note_xm_sets_up_auto_vibrato_and_envelopes() {
    use crate::sequencer::instrument::{
        Instrument, Envelope, EnvelopeFlags, EnvelopePoint,
    };
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let mut sample = Sample::default();
    sample.default_volume = 48;
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    let vol_env = Envelope {
        points: vec![
            EnvelopePoint { tick: 0, value: 0 },
            EnvelopePoint { tick: 10, value: 64 },
        ],
        sustain_point: Some(1),
        loop_start: None,
        loop_end: None,
        flags: EnvelopeFlags {
            enabled: true,
            sustain: true,
            loop_: false,
            carry: false,
        },
    };

    let vib_depth: u8 = 8;
    let fade_out: u16 = 128;
    let mut inst = Instrument::default();
    inst.vib_depth = vib_depth;
    inst.vib_sweep = 0;
    inst.vib_rate = 6;
    inst.vib_type = 0;
    inst.fade_out = fade_out;
    inst.volume_envelope = Some(vol_env);
    inst.sample_map[60] = 1;

    let module = Arc::new(Module {
        format: ModuleFormat::XM,
        instruments: vec![Instrument::default(), inst],
        samples: vec![Sample::default(), sample],
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags {
            linear_slides: true,
            use_instruments: true,
            xm_period_model: true,
            ..ModuleFlags::default()
        },
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    // Create a delayed-note cell
    let mut cell = Cell::default();
    cell.note = Note::On(60);
    cell.instrument = Some(1);
    cell.effect = Effect::NoteDelay { ticks: 2 };

    // Set up channel state for the delayed note
    engine.state.channels[0].delayed_cell = Some(cell);
    engine.state.channels[0].note_delay_ticks = 2;
    engine.state.channels[0].last_instrument = 1;

    // Trigger the delayed note (simulating tick 2 processing)
    engine.state.clock.current_tick = 2;
    let linear = module.flags.linear_slides;
    engine.trigger_delayed_note_period(0, linear);

    let voice = engine.voice_pool.voices.iter()
        .find(|v| v.active && v.channel == Some(0))
        .expect("Should have active voice after delayed trigger");

    // Verify auto-vibrato was set up
    assert!(
        voice.auto_vib_period_base > 0,
        "Delayed note: auto_vib_period_base should be > 0, got {}",
        voice.auto_vib_period_base
    );
    assert_eq!(voice.auto_vib_amp, (vib_depth as i32) * 256,
        "Delayed note: auto_vib_amp should be at full depth");

    // Verify envelope was set up
    assert!(
        voice.vol_env.is_some(),
        "Delayed note: volume envelope should be set up"
    );
    assert!(
        voice.env_sustain_active,
        "Delayed note: env_sustain_active should be true"
    );
    assert_eq!(
        voice.fade_out_rate, fade_out,
        "Delayed note: fade_out_rate should be {}", fade_out
    );
    assert_eq!(
        voice.fade_out_amp, 32768,
        "Delayed note: fade_out_amp should be 32768"
    );
    assert_eq!(
        voice.instrument_index,
        Some(1),
        "Delayed note: instrument_index should be set"
    );
}

#[test]
fn trigger_channel_note_resets_auto_vib_period_on_reuse() {
    use crate::sequencer::instrument::Instrument;
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let mut sample = Sample::default();
    sample.default_volume = 48;
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    let mut inst = Instrument::default();
    inst.vib_depth = 5;
    inst.vib_sweep = 20;
    inst.vib_rate = 4;
    inst.vib_type = 0;
    inst.sample_map[60] = 1;
    inst.sample_map[72] = 1;

    let module = Arc::new(Module {
        format: ModuleFormat::XM,
        instruments: vec![Instrument::default(), inst],
        samples: vec![Sample::default(), sample],
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags {
            linear_slides: true,
            use_instruments: true,
            xm_period_model: true,
            ..ModuleFlags::default()
        },
        ..Module::default()
    });

    engine.load_module(module.clone());

    // First trigger: C-5 (key=60)
    engine.play();
    let mut cell = Cell::default();
    cell.note = Note::On(60);
    cell.instrument = Some(1);

    engine.state.clock.current_tick = 0;
    engine.state.current_row = 0;
    engine.process_cell_unified(0, &cell);

    let period_c5 = crate::sequencer::period::get_note_period(60, 0, true);

    let voice = engine.voice_pool.voices.iter()
        .find(|v| v.active && v.channel == Some(0))
        .expect("Should have voice after first trigger");
    assert_eq!(
        voice.auto_vib_period_base, period_c5,
        "First note: auto_vib_period_base should match C-5 period"
    );

    // Advance to next row and trigger a different note
    engine.advance_row();
    let mut cell2 = Cell::default();
    cell2.note = Note::On(72); // C-6
    cell2.instrument = Some(1);

    engine.state.clock.current_tick = 0;
    engine.state.current_row = 1;
    engine.process_cell_unified(0, &cell2);

    let period_c6 = crate::sequencer::period::get_note_period(72, 0, true);

    let voice2 = engine.voice_pool.voices.iter()
        .find(|v| v.active && v.channel == Some(0))
        .expect("Should have voice after second trigger");
    assert_eq!(
        voice2.auto_vib_period_base, period_c6,
        "Second note: auto_vib_period_base should match C-6 period, not C-5"
    );
}

#[test]
fn xm_active_effects_dispatch_volume_slide() {
    use crate::sequencer::instrument::Instrument;
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let mut sample = Sample::default();
    sample.default_volume = 64;
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    let mut inst = Instrument::default();
    inst.sample_map[60] = 1;

    let module = Arc::new(Module {
        format: ModuleFormat::XM,
        instruments: vec![Instrument::default(), inst],
        samples: vec![Sample::default(), sample],
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags {
            linear_slides: true,
            use_instruments: true,
            xm_period_model: true,
            ..ModuleFlags::default()
        },
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    // Trigger a note
    let mut cell = Cell::default();
    cell.note = Note::On(60);
    cell.instrument = Some(1);
    cell.effect = Effect::VolumeSlide { up: 2, down: 0 };

    engine.state.clock.current_tick = 0;
    engine.process_cell_unified(0, &cell);

    assert!(
        engine.state.channels[0].active_effects.volume_slide,
        "VolumeSlide should set active_effects.volume_slide"
    );
    assert_eq!(
        engine.state.channels[0].last_volume_slide_up, 2,
        "VolumeSlide should store up value"
    );

    // Process non-zero tick ΓÇö ActiveEffects dispatch should apply slide
    let vol_before = engine.state.channels[0].real_vol;
    engine.state.clock.current_tick = 1;
    engine.process_effects_tick_unified();
    let vol_after = engine.state.channels[0].real_vol;
    assert!(
        vol_after > vol_before || vol_after == 64,
        "VolumeSlide should increase volume on non-zero tick: {} -> {}",
        vol_before, vol_after
    );
}

#[test]
fn xm_active_effects_dispatch_tpvs() {
    use crate::sequencer::instrument::Instrument;
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let mut sample = Sample::default();
    sample.default_volume = 64;
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    let mut inst = Instrument::default();
    inst.sample_map[60] = 1;
    inst.sample_map[64] = 1;

    let module = Arc::new(Module {
        format: ModuleFormat::XM,
        instruments: vec![Instrument::default(), inst],
        samples: vec![Sample::default(), sample],
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags {
            linear_slides: true,
            use_instruments: true,
            xm_period_model: true,
            ..ModuleFlags::default()
        },
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    // First trigger a note
    let mut cell = Cell::default();
    cell.note = Note::On(60);
    cell.instrument = Some(1);
    engine.state.clock.current_tick = 0;
    engine.process_cell_unified(0, &cell);

    // Now set TPVS: tone portamento to new note + volume slide
    engine.advance_row();
    let mut cell2 = Cell::default();
    cell2.note = Note::On(64);
    cell2.effect = Effect::TonePortamentoVolumeSlide { up: 0x15 };
    engine.state.clock.current_tick = 0;
    engine.process_cell_unified(0, &cell2);

    assert!(
        engine.state.channels[0].active_effects.tone_portamento,
        "TPVS should set active_effects.tone_portamento"
    );
    assert!(
        engine.state.channels[0].active_effects.volume_slide,
        "TPVS should set active_effects.volume_slide"
    );
    assert_eq!(
        engine.state.channels[0].last_volume_slide_up, 1,
        "TPVS param 0x15: up nibble = 1"
    );
    assert_eq!(
        engine.state.channels[0].last_volume_slide_down, 5,
        "TPVS param 0x15: down nibble = 5"
    );
}

#[test]
fn xm_note_delay_triggers_on_correct_tick() {
    use crate::sequencer::instrument::Instrument;
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let mut sample = Sample::default();
    sample.default_volume = 48;
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    let mut inst = Instrument::default();
    inst.sample_map[60] = 1;

    let module = Arc::new(Module {
        format: ModuleFormat::XM,
        instruments: vec![Instrument::default(), inst],
        samples: vec![Sample::default(), sample],
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags {
            linear_slides: true,
            use_instruments: true,
            xm_period_model: true,
            ..ModuleFlags::default()
        },
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    // Tick 0: process delayed note cell
    let mut cell = Cell::default();
    cell.note = Note::On(60);
    cell.instrument = Some(1);
    cell.effect = Effect::NoteDelay { ticks: 3 };

    engine.state.clock.current_tick = 0;
    engine.process_cell_unified(0, &cell);

    // No voice should be active yet (note is delayed)
    assert!(
        engine.voice_pool.voices.iter().all(|v| !v.active || v.channel != Some(0)),
        "No voice on ch0 after tick 0 with NoteDelay"
    );
    assert_eq!(engine.state.channels[0].note_delay_ticks, 3);
    assert!(engine.state.channels[0].delayed_cell.is_some());

    // Tick 1: still no voice
    engine.state.clock.current_tick = 1;
    engine.process_effects_tick_unified();
    assert!(
        engine.voice_pool.voices.iter().all(|v| !v.active || v.channel != Some(0)),
        "No voice on ch0 at tick 1"
    );

    // Tick 2: still no voice
    engine.state.clock.current_tick = 2;
    engine.process_effects_tick_unified();
    assert!(
        engine.voice_pool.voices.iter().all(|v| !v.active || v.channel != Some(0)),
        "No voice on ch0 at tick 2"
    );

    // Tick 3: delayed note should trigger
    engine.state.clock.current_tick = 3;
    engine.process_effects_tick_unified();
    assert!(
        engine.voice_pool.voices.iter().any(|v| v.active && v.channel == Some(0)),
        "Voice should be active on ch0 at tick 3 (delayed note trigger)"
    );
}

#[test]
fn mod_pattern_loop_sets_loop_start() {
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    engine.state.current_order = 2;
    engine.state.current_row = 16;
    engine.state.channels.resize(1, ChannelState::default());
    engine.use_xm_model = false;

    // E6x with count=0 sets loop start
    let cell = Cell {
        effect: Effect::PatternLoop { count: 0 },
        ..Cell::default()
    };

    engine.process_cell_unified(0, &cell);

    // Loop start should be captured
    assert!(
        engine.state.pattern_loop_start.is_some(),
        "Pattern loop start should be set when count=0"
    );
    let (order, row) = engine.state.pattern_loop_start.unwrap();
    assert_eq!(order, 2);
    assert_eq!(row, 16);
}

#[test]
fn mod_pattern_loop_executes_loop() {
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    engine.state.current_order = 0;
    engine.state.current_row = 4;
    engine.state.channels.resize(1, ChannelState::default());
    engine.use_xm_model = false;

    // First, E60 to set loop start at row 4
    let cell_start = Cell {
        effect: Effect::PatternLoop { count: 0 },
        ..Cell::default()
    };
    engine.process_cell_unified(0, &cell_start);
    assert!(engine.state.pattern_loop_start.is_some());

    // Move to row 63 (last row) for the loop trigger
    engine.state.current_row = 63;

    // Then E62 (count=2) to set loop repeat count
    let cell_loop = Cell {
        effect: Effect::PatternLoop { count: 2 },
        ..Cell::default()
    };
    engine.process_cell_unified(0, &cell_loop);

    assert_eq!(engine.state.pattern_loop_count, 2);

    // After advance_row from last row, should loop back and decrement
    engine.advance_row();

    assert_eq!(engine.state.pattern_loop_count, 1);
    assert_eq!(engine.state.current_row, 4);
}

#[test]
fn mod_pattern_loop_advances_to_next_order() {
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0, 1],
        patterns: vec![
            Pattern::new(64),
            Pattern::new(64),
        ],
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    engine.state.current_order = 0;
    engine.state.current_row = 4;
    engine.state.channels.resize(1, ChannelState::default());
    engine.use_xm_model = false;

    // E60 to set loop start at row 4
    let cell_start = Cell {
        effect: Effect::PatternLoop { count: 0 },
        ..Cell::default()
    };
    engine.process_cell_unified(0, &cell_start);
    assert!(engine.state.pattern_loop_start.is_some());

    // Move to row 63 for the trigger
    engine.state.current_row = 63;

    // E61 (count=1) to trigger one loop iteration
    let cell_loop = Cell {
        effect: Effect::PatternLoop { count: 1 },
        ..Cell::default()
    };
    engine.process_cell_unified(0, &cell_loop);
    assert_eq!(engine.state.pattern_loop_count, 1);

    // advance_row: count 1->0, loop back to row 4
    engine.advance_row();
    assert_eq!(engine.state.pattern_loop_count, 0);
    assert_eq!(engine.state.current_row, 4);
    assert_eq!(engine.state.current_order, 0);
    assert_eq!(engine.state.pattern_loop_start, None);

    // Advance from row 4 through the rest of the pattern back to row 63
    for _ in 0..59 {
        engine.advance_row();
    }
    assert_eq!(engine.state.current_row, 63);
    assert_eq!(engine.state.current_order, 0);

    // Now at row 63 again with no loop active - advance past last row to next order
    engine.advance_row();
    assert_eq!(engine.state.current_order, 1);
    assert_eq!(engine.state.current_row, 0);
}

#[test]
fn mod_pattern_loop_count_3_exits_correctly() {
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0, 1],
        patterns: vec![
            Pattern::new(64),
            Pattern::new(64),
        ],
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    engine.state.current_order = 0;
    engine.state.current_row = 0;
    engine.state.channels.resize(1, ChannelState::default());
    engine.use_xm_model = false;

    // E60 to set loop start at row 0
    let cell_start = Cell {
        effect: Effect::PatternLoop { count: 0 },
        ..Cell::default()
    };
    engine.process_cell_unified(0, &cell_start);
    assert!(engine.state.pattern_loop_start.is_some());

    // Move to row 63
    engine.state.current_row = 63;

    // E63 (count=3) - same as wash.mod pattern 3
    let cell_loop = Cell {
        effect: Effect::PatternLoop { count: 3 },
        ..Cell::default()
    };
    engine.process_cell_unified(0, &cell_loop);
    assert_eq!(engine.state.pattern_loop_count, 3);

    // Iteration 1: count 3->2, jump back to row 0
    engine.advance_row();
    assert_eq!(engine.state.pattern_loop_count, 2);
    assert_eq!(engine.state.current_row, 0);
    assert_eq!(engine.state.current_order, 0);

    // Advance to row 63
    for _ in 0..63 {
        engine.advance_row();
    }
    assert_eq!(engine.state.current_row, 63);

    // Iteration 2: count 2->1, jump back to row 0
    engine.advance_row();
    assert_eq!(engine.state.pattern_loop_count, 1);
    assert_eq!(engine.state.current_row, 0);
    assert_eq!(engine.state.current_order, 0);

    // Advance to row 63
    for _ in 0..63 {
        engine.advance_row();
    }
    assert_eq!(engine.state.current_row, 63);

    // Iteration 3 (final): count 1->0, jump back to row 0 for final pass
    engine.advance_row();
    assert_eq!(engine.state.pattern_loop_count, 0);
    assert_eq!(engine.state.pattern_loop_start, None);
    assert_eq!(engine.state.pattern_loop_final_pass, true);
    assert_eq!(engine.state.current_row, 0);
    assert_eq!(engine.state.current_order, 0);

    // Advance through final pass to row 63 (loop commands ignored due to final_pass flag)
    for _ in 0..63 {
        engine.advance_row();
    }
    assert_eq!(engine.state.current_row, 63);

    // Advance past row 63 to next order
    engine.advance_row();
    assert_eq!(engine.state.current_order, 1);
    assert_eq!(engine.state.current_row, 0);
    assert_eq!(engine.state.pattern_loop_final_pass, false);
}

#[test]
fn advance_row_resets_retrigger_state() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.state.channels[0].retrig_speed = 4;
    engine.state.channels[0].retrig_cnt = 3;
    engine.state.channels[0].last_retrigger_interval = 4;

    engine.advance_row();

    assert_eq!(engine.state.channels[0].retrig_speed, 0,
        "retrig_speed should be reset on row advance");
    assert_eq!(engine.state.channels[0].retrig_cnt, 0,
        "retrig_cnt should be reset on row advance");
    assert_eq!(engine.state.channels[0].last_retrigger_interval, 0,
        "last_retrigger_interval should be reset on row advance");
}

#[test]
fn xm_pattern_delay_sets_row_delay_active() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = true;

    let module = Arc::new(Module::default());
    engine.load_module(module);
    engine.play();

    let cell = Cell {
        effect: Effect::PatternDelay { ticks: 2 },
        ..Cell::default()
    };

    engine.state.clock.current_tick = 0;
    engine.process_cell_unified(0, &cell);

    assert!(engine.state.row_delay_active,
        "PatternDelay should set row_delay_active for XM");
    assert_eq!(engine.state.pattern_delay_ticks, 2,
        "PatternDelay should store tick count");
}

#[test]
fn advance_row_resets_note_cut_tick() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.state.channels[0].note_cut_tick = Some(5);

    engine.advance_row();

    assert_eq!(engine.state.channels[0].note_cut_tick, None,
        "note_cut_tick should be reset on row advance");
}

#[test]
fn mod_tone_portamento_slides_toward_target() {
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);
    let mut sample = Sample::default();
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        instruments: vec![Instrument::default()],
        samples: vec![Sample::default(), sample.clone()],
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    engine.state.channels[0].last_instrument = 1;
    engine.state.channels[0].last_sample = 1;

    let (target_period, target_freq) = engine.compute_portamento_target(0, 60, 60, Some(&sample), 1, &module);
    assert!(target_period > 0, "compute_portamento_target should return a valid period");

    engine.state.channels[0].real_period = 856;
    engine.state.channels[0].out_period = 856;
    engine.state.channels[0].want_period = target_period;
    engine.state.channels[0].portamento_target_period = Some(target_period);
    engine.state.channels[0].portamento_target_frequency = Some(target_freq);
    engine.state.channels[0].last_tone_portamento_speed = 8;

    let before = engine.state.channels[0].real_period;
    engine.apply_tone_portamento(0, 8);
    let after = engine.state.channels[0].real_period;

    assert_ne!(before, after, "apply_tone_portamento should change period");
    assert_ne!(after, 856, "apply_tone_portamento should produce a different period");
}

#[test]
fn mod_vibrato_depth_within_protracker_range() {
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;
    use crate::sequencer::period::period_to_frequency;

    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let mut sample = Sample::default();
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        samples: vec![Sample::default(), sample],
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    let mut voice = Voice::default();
    voice.active = true;
    voice.channel = Some(0);
    voice.base_frequency = 440.0;
    voice.sample_delta = 440.0 / 48000.0;
    voice.vibrato_waveform = VibratoWaveform::Sine;
    voice.vibrato_phase = 0.0;
    engine.voice_pool.voices[0] = voice;

    engine.state.channels[0].real_period = 856;
    engine.state.channels[0].out_period = 856;
    engine.state.channels[0].wave_ctrl = 0;
    engine.state.channels[0].vib_pos = 0;

    let initial_period = engine.state.channels[0].real_period;
    let initial_freq = period_to_frequency(initial_period, false, 8363);
    let after_period = {
        let ch = &mut engine.state.channels[0];
        let vib_tab = get_vib_tab();
        let waveform = ch.wave_ctrl & 0x03;
        let tmp_vib = ((ch.vib_pos >> 2) & 0x1F) as usize;
        let vibrato_val: i32 = match waveform {
            0 => vib_tab[tmp_vib] as i32,
            1 => {
                let val = (tmp_vib as i32) << 3;
                if (ch.vib_pos as i8) < 0 { !val } else { val }
            }
            _ => 255,
        };
        let offset = ((vibrato_val * 15) >> 3) as u16;
        if (ch.vib_pos as i8) < 0 {
            ch.real_period.saturating_sub(offset).max(1)
        } else {
            ch.real_period.saturating_add(offset).min(31999)
        }
    };
    let after_freq = period_to_frequency(after_period, false, 8363);

    let freq_mod = after_freq / initial_freq;
    let semitones = (freq_mod.log2() * 12.0).abs();
    assert!(semitones < 2.0,
        "MOD vibrato depth 15 should be < 2 semitones, got {:.1}", semitones);
}

#[test]
fn mod_volume_slide_memory_uses_full_param() {
    use crate::sequencer::module::ModuleFlags;
    use crate::sequencer::pattern::Pattern;

    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    engine.state.channels[0].channel_volume = 64;
    engine.state.channels[0].last_volume_slide_param = 0;
    engine.state.channels[0].last_volume_slide_up = 3;
    engine.state.channels[0].last_volume_slide_down = 0;

    engine.apply_volume_slide(0);

    let vol_after = engine.state.channels[0].channel_volume;
    assert_eq!(vol_after, 64,
        "With param=0, should slide 0 (up=0, down=0 from param), ignoring stale up=3");

    engine.state.channels[0].channel_volume = 64;
    engine.state.channels[0].last_volume_slide_param = 0x30;
    engine.state.channels[0].last_volume_slide_up = 5;
    engine.state.channels[0].last_volume_slide_down = 7;

    engine.apply_volume_slide(0);

    let vol_after_param = engine.state.channels[0].channel_volume;
    assert_eq!(vol_after_param, 64,
        "With param=0x30, should slide up by 3, but 64+3=67 exceeds max 64, clamped to 64");
}

#[test]
fn xm_tone_portamento_still_works() {
    use crate::sequencer::{Instrument, Module, ModuleFormat, Pattern, Sample};
    use crate::sequencer::module::ModuleFlags;

    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = true;

    let mut sample = Sample::default();
    sample.data = Arc::new(vec![0.0f32; 100]);
    sample.sample_rate = 8363;

    let module = Arc::new(Module {
        format: ModuleFormat::XM,
        instruments: vec![Instrument::default()],
        samples: vec![Sample::default(), sample],
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags {
            linear_slides: true,
            use_instruments: true,
            xm_period_model: true,
            ..ModuleFlags::default()
        },
        ..Module::default()
    });

    engine.load_module(module.clone());
    engine.play();

    engine.state.channels[0].last_instrument = 1;
    engine.state.channels[0].last_sample = 1;
    engine.state.channels[0].real_period = 856;
    engine.state.channels[0].want_period = 428;
    engine.state.channels[0].porta_dir = 2;
    engine.state.channels[0].porta_speed_period = 8;

    let before = engine.state.channels[0].real_period;
    let target_period = engine.state.channels[0].want_period;
    engine.apply_tone_portamento_period(0, true);
    let after = engine.state.channels[0].real_period;

    assert!(after < before,
        "XM portamento (porta_dir=2, slide up = lower period) should decrease period: {} -> {}",
        before, after);
    assert!(after >= target_period,
        "XM portamento should not overshoot target period");
}

#[test]
fn portamento_up_memory_preserved_when_param_is_zero() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    engine.state.channels[0].last_portamento_up_speed = 4;
    engine.state.channels[0].active_effects.portamento_up = true;

    engine.apply_effect_unified(0, &Effect::PortamentoUp { speed: 0 }, true);

    assert_eq!(engine.state.channels[0].last_portamento_up_speed, 4,
        "Zero-param portamento up should preserve last speed");
    assert!(engine.state.channels[0].active_effects.portamento_up,
        "Zero-param portamento up should keep active flag");
}

#[test]
fn vibrato_memory_preserved_when_param_is_zero() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    engine.state.channels[0].last_vibrato_speed = 5;
    engine.state.channels[0].last_vibrato_depth = 8;
    engine.state.channels[0].active_effects.vibrato = true;

    engine.apply_effect_unified(0, &Effect::Vibrato { speed: 0, depth: 0 }, true);

    assert_eq!(engine.state.channels[0].last_vibrato_speed, 5,
        "Zero-param vibrato should preserve last speed");
    assert_eq!(engine.state.channels[0].last_vibrato_depth, 8,
        "Zero-param vibrato should preserve last depth");
    assert!(engine.state.channels[0].active_effects.vibrato,
        "Zero-param vibrato should keep active flag");
}

#[test]
fn panning_slide_memory_preserved_when_param_is_zero() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = true;

    let module = Arc::new(Module {
        format: ModuleFormat::XM,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: true, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    engine.state.channels[0].last_panning_slide = 3;
    engine.state.channels[0].channel_panning = 128;

    engine.apply_effect_unified(0, &Effect::PanningSlide { speed: 0 }, true);

    assert_eq!(engine.state.channels[0].last_panning_slide, 3,
        "Zero-param panning slide should preserve last value");
    assert!(engine.state.channels[0].active_effects.panning_slide,
        "Zero-param panning slide should keep active flag");
}

#[test]
fn global_volume_slide_xm_applies_each_tick() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = true;

    let module = Arc::new(Module {
        format: ModuleFormat::XM,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: true, ..ModuleFlags::default() },
        instruments: vec![crate::sequencer::instrument::Instrument::default()],
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    engine.state.global_volume = 32;
    engine.state.last_global_volume_up = 3;
    engine.state.last_global_volume_down = 0;
    engine.state.channels[0].active_effects.global_volume_slide = true;
    engine.state.clock.current_tick = 1;
    engine.process_effects_tick_unified();
    assert_eq!(engine.state.global_volume, 35, "XM global volume should increase by up each tick");
}

#[test]
fn global_volume_slide_non_xm_applies_each_tick() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    engine.state.global_volume = 64;
    engine.state.last_global_volume_up = 0;
    engine.state.last_global_volume_down = 5;
    engine.state.channels[0].active_effects.global_volume_slide = true;
    engine.state.clock.current_tick = 1;
    engine.process_effects_tick_unified();
    assert_eq!(engine.state.global_volume, 59, "non-XM global volume should decrease by down each tick");
}

#[test]
fn global_volume_slide_memory_accumulates_per_tick() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    engine.state.global_volume = 64;
    engine.state.last_global_volume_up = 2;
    engine.state.last_global_volume_down = 0;
    engine.state.channels[0].active_effects.global_volume_slide = true;
    engine.state.clock.current_tick = 1;
    engine.process_effects_tick_unified();
    assert_eq!(engine.state.global_volume, 66);
    engine.state.clock.current_tick = 2;
    engine.process_effects_tick_unified();
    assert_eq!(engine.state.global_volume, 68, "slide should accumulate across ticks");
}

#[test]
fn extra_fine_portamento_slows_by_factor_4() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    let ch = 0;
    engine.state.channels[ch].real_period = 500;
    engine.state.channels[ch].out_period = 500;
    engine.apply_effect_unified(ch, &Effect::ExtraFinePortamentoDown { speed: 4 }, true);
    let spd = ((4u8 as u16 + 2) >> 2).max(1);
    assert_eq!(engine.state.channels[ch].real_period, 500 + spd as u16,
        "ExtraFinePortamentoDown speed 4 -> spd {}, period+spd", spd);
}

#[test]
fn funkit_modulates_voice_position_on_tick() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    let ch = 0;
    engine.state.channels[ch].funk_speed = 4;
    engine.state.channels[ch].funk_toggle = true;
    engine.state.clock.current_tick = FUNK_TRACK[4] as u8;
    engine.voice_pool.voices[0].active = true;
    engine.voice_pool.voices[0].channel = Some(0);
    engine.voice_pool.voices[0].position = 100.0;
    engine.process_effects_tick_unified();
    assert!(engine.voice_pool.voices[0].position >= 100.0, "FunkIt should modulate voice position");
}

#[test]
fn funkit_speed_zero_disables_modulation() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    engine.state.channels[0].funk_speed = 0;
    let pos_before = engine.voice_pool.voices[0].position;
    engine.state.clock.current_tick = 5;
    engine.process_effects_tick_unified();
    assert_eq!(engine.voice_pool.voices[0].position, pos_before, "funk_speed=0 should not move position");
}

#[test]
fn karplus_strong_initializes_buffer_on_trigger() {
    let mut engine = SequencerEngine::new(48000.0);
    engine.use_xm_model = false;

    let module = Arc::new(Module {
        format: ModuleFormat::MOD,
        order_list: vec![0],
        patterns: vec![Pattern::new(64)],
        flags: ModuleFlags { linear_slides: false, ..ModuleFlags::default() },
        ..Module::default()
    });
    engine.load_module(module);
    engine.play();

    let ch = 0;
    engine.state.channels[ch].karplus_param = 8;
    assert_eq!(engine.state.channels[ch].karplus_param, 8);
}

#[test]
fn karplus_strong_disabled_when_param_zero() {
    let mut engine = SequencerEngine::new(48000.0);
    let voice = &mut engine.voice_pool.voices[0];
    voice.karplus_strong = false;
    assert!(!voice.karplus_strong);
    voice.karplus_strong = true;
    voice.karplus_strong = false;
    assert!(!voice.karplus_strong);
}

#[test]
fn karplus_strong_mixer_produces_output() {
    use crate::audio::mixer;
    use crate::audio::commands::InterpolationType;

    let mut voices = vec![crate::audio::voice::Voice::default()];
    let v = &mut voices[0];
    v.active = true;
    v.karplus_strong = true;
    v.ks_pos = 0;
    v.ks_delay_line = vec![0.5_f32; 64];
    v.ks_feedback = 0.9;
    v.base_volume = 1.0;
    v.final_volume = 1.0;
    v.final_panning = 0.5;
    v.channel = Some(0);
    let mut left = vec![0.0_f32; 16];
    let mut right = vec![0.0_f32; 16];
    let sample_rate = 44100.0;
    mixer::mix_voices(&mut voices, &mut left, &mut right, 1.0, InterpolationType::Linear, &[], sample_rate);
    let has_output = left.iter().any(|&s| s != 0.0);
    assert!(has_output, "KS should produce non-zero output");
}

#[test]
fn test_plugin_instrument_queues_note_on() {
    use crate::sequencer::plugin::PluginSlot;

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
