use htrk::audio::renderer::WavRenderer;
use htrk::sequencer::{
    Effect, Instrument, LoopType, Module, ModuleFormat, NewNoteAction, Note, Pattern, Sample,
};
use htrk::ui::wav_export_window::{AudioFormat, BitDepth, ChannelMode, WavExportSettings};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

const SAMPLE_RATE: u32 = 44100;

fn test_sample_data() -> Vec<f32> {
    (0..512)
        .map(|i| {
            // Pure arithmetic ramp: avoids libm so the golden hash cannot
            // drift with the platform's `sin` implementation.
            let phase = (i % 64) as f32 / 64.0;
            (phase * 2.0 - 1.0) * 0.6
        })
        .collect()
}

fn base_module() -> Module {
    let mut module = Module::default();
    module.samples.clear();
    module.instruments.clear();
    module.patterns.clear();
    module.name = "Golden".to_string();

    let mut pattern = Pattern::new(64);

    pattern.data[0][0].note = Note::On(60);
    pattern.data[0][0].instrument = Some(1);
    pattern.data[0][0].volume = Some(64);

    pattern.data[0][1].note = Note::On(67);
    pattern.data[0][1].instrument = Some(1);
    pattern.data[0][1].volume = Some(48);

    pattern.data[8][0].effect = Effect::VolumeSlide { up: 0, down: 2 };
    pattern.data[16][0].effect = Effect::Arpeggio { note1: 4, note2: 7 };
    pattern.data[24][0].effect = Effect::Vibrato { speed: 4, depth: 8 };
    pattern.data[32][0].effect = Effect::PortamentoUp { speed: 2 };

    pattern.data[24][1].note = Note::On(72);
    pattern.data[24][1].effect = Effect::TonePortamento { speed: 4 };
    pattern.data[40][1].effect = Effect::SetPanning { pan: 200 };
    pattern.data[48][1].effect = Effect::SetVolume { volume: 40 };

    pattern.data[56][0].effect = Effect::NoteCutAfter { ticks: 3 };

    module.patterns.push(pattern);
    module.order_list = vec![0];
    module.channel_panning = vec![128, 128];
    module.channel_volume = vec![64, 64];

    module.samples.push(Sample::default());

    module.samples.push(Sample {
        name: "Golden Sample".to_string(),
        data: Arc::new(test_sample_data()),
        sample_rate: 8363,
        default_volume: 64,
        loop_type: LoopType::Forward,
        loop_start: 0,
        loop_end: 512,
        ..Sample::default()
    });

    module.instruments.push(Instrument::default());

    let mut inst = Instrument {
        name: "Golden Instrument".to_string(),
        ..Instrument::default()
    };
    for key in [60u8, 67, 72] {
        inst.sample_map[key as usize] = 1;
    }
    module.instruments.push(inst);

    module
}

fn case_legacy_htk() -> Module {
    base_module()
}

fn case_xm_linear() -> Module {
    let mut module = base_module();
    module.format = ModuleFormat::XM;
    module.flags.xm_period_model = true;
    module.flags.linear_slides = true;
    module
}

fn case_xm_amiga() -> Module {
    let mut module = base_module();
    module.format = ModuleFormat::XM;
    module.flags.xm_period_model = true;
    module.flags.linear_slides = false;
    module
}

fn case_it_nna() -> Module {
    let mut module = base_module();
    module.format = ModuleFormat::IT;
    module.flags.use_instruments = true;
    module.instruments[1].nna = NewNoteAction::NoteFade;
    module.instruments[1].fade_out = 300;

    // Retrigger channel 0 twice within a few rows so the previous note has to
    // go through the new-note-action path while its voice is still active.
    // The retrigger keys MUST be mapped to a sample: unmapped keys resolve
    // to sample 0, which returns from `trigger_note` before `handle_nna`
    // runs, leaving the old voice ringing (and the hash identical to
    // legacy_htk — the test would prove nothing).
    for key in [62u8, 64] {
        module.instruments[1].sample_map[key as usize] = 1;
    }
    for (row, key) in [(2usize, 62u8), (3, 64)] {
        module.patterns[0].data[row][0].note = Note::On(key);
        module.patterns[0].data[row][0].instrument = Some(1);
        module.patterns[0].data[row][0].volume = Some(64);
    }
    module
}

fn case_s3m_extrafine() -> Module {
    let mut module = base_module();
    module.format = ModuleFormat::S3M;
    module.flags.xm_period_model = false;

    module.patterns[0].data[20][0].note = Note::On(60);
    module.patterns[0].data[20][0].instrument = Some(1);
    module.patterns[0].data[20][0].effect = Effect::ExtraFinePortamentoUp { speed: 3 };
    module.patterns[0].data[24][0].effect = Effect::ExtraFinePortamentoDown { speed: 3 };
    module.patterns[0].data[28][0].effect = Effect::GlobalVolumeSlide { up: 0, down: 1 };
    module
}

fn render_module(module: Module) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut cursor, spec).expect("wav writer");

    let settings = WavExportSettings {
        file_path: None,
        bit_depth: BitDepth::Bits16,
        sample_rate: SAMPLE_RATE,
        channel_mode: ChannelMode::Stereo,
        format: AudioFormat::Wav,
        normalize: false,
        dither: false,
    };

    {
        let mut renderer = WavRenderer::new(Arc::new(module), SAMPLE_RATE);
        renderer
            .render_with_settings(&mut writer, &settings, |_| true)
            .expect("render");
    }

    writer.finalize().expect("finalize");
    cursor.into_inner()
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Pinned hashes of the full offline render (sequencer + mixer + limiter +
/// WAV encoding) for each format-conditional case.
///
/// A value of `0` means "not yet recorded": the test prints the computed hash
/// and fails until it is filled in. Run with `--nocapture` after an
/// intentional audio change to see the new hashes.
const GOLDEN: &[(&str, u64)] = &[
    ("legacy_htk", 15004428839947752327),
    ("xm_linear", 15461450664475370306),
    ("xm_amiga", 4317173246003218421),
    ("it_nna", 1353589655225807274),
    ("s3m_extrafine", 12239231019632939853),
];

#[test]
fn golden_render_cases_are_stable() {
    let cases: Vec<(&str, Module)> = vec![
        ("legacy_htk", case_legacy_htk()),
        ("xm_linear", case_xm_linear()),
        ("xm_amiga", case_xm_amiga()),
        ("it_nna", case_it_nna()),
        ("s3m_extrafine", case_s3m_extrafine()),
    ];

    let mut hashes: HashMap<&str, u64> = HashMap::new();

    for (name, module) in cases {
        let bytes = render_module(module);
        assert!(bytes.len() > 44, "{name}: render produced no WAV data");

        let mut reader = hound::WavReader::new(Cursor::new(&bytes)).expect("wav reader");
        let non_silent = reader
            .samples::<i16>()
            .filter(|s| matches!(s, Ok(v) if *v != 0))
            .count();
        assert!(
            non_silent > 1000,
            "{name}: expected audible content, got {non_silent} non-zero samples"
        );

        let hash = fnv1a_64(&bytes);
        eprintln!("golden {name}: {hash} (bytes={})", bytes.len());
        hashes.insert(name, hash);
    }

    // The linear and Amiga period models must genuinely diverge; if they ever
    // render identically the format-conditional branch is dead.
    assert_ne!(
        hashes["xm_linear"], hashes["xm_amiga"],
        "XM linear and Amiga period models rendered identically"
    );

    let mut unset = Vec::new();
    for (name, expected) in GOLDEN {
        if *expected == 0 {
            unset.push(*name);
            continue;
        }
        assert_eq!(
            hashes[*name], *expected,
            "{name}: offline render output changed; if intended, update GOLDEN"
        );
    }

    assert!(unset.is_empty(), "unset golden hashes for: {unset:?}");
}
