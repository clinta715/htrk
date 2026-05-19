use htrk::sequencer::{Module, ModuleFormat, Pattern, Instrument, Sample, Note, Effect};
use htrk::sequencer::{AutomationTrack, AutomationTarget, AutomationPoint, InterpolationMode};
use htrk::formats;
use std::sync::Arc;

fn create_test_module() -> Module {
    let mut module = Module::default();
    module.samples.clear();
    module.instruments.clear();
    module.name = "Test Module".to_string();

    let mut pat = Pattern::new(64);
    pat.data[0][0].note = Note::On(60);
    pat.data[0][0].instrument = Some(1);
    pat.data[0][0].volume = Some(64);
    pat.data[0][0].effect = Effect::SetSpeed { speed: 3 };

    pat.data[1][1].note = Note::On(64);
    pat.data[1][1].effect = Effect::VolumeSlide { up: 1, down: 0 };

    module.patterns.push(pat);
    module.order_list = vec![0];

    let mut sample = Sample::default();
    sample.name = "Test Sample".to_string();
    sample.data = Arc::new(vec![0.0; 1000]);
    sample.loop_type = htrk::sequencer::LoopType::Forward;
    sample.loop_start = 100;
    sample.loop_end = 900;
    module.samples.push(sample);

    let mut inst = Instrument::default();
    inst.name = "Test Instrument".to_string();
    inst.sample_map[60] = 1;
    module.instruments.push(inst);

    module
}

#[test]
fn test_htk_roundtrip() {
    let module = create_test_module();
    let data = formats::save_module(&module);
    let loaded = formats::load_module(&data).unwrap();

    assert_eq!(loaded.name, module.name);
    assert_eq!(loaded.format, ModuleFormat::HTK);
    assert_eq!(loaded.patterns.len(), module.patterns.len());
    assert_eq!(loaded.samples.len(), module.samples.len());
    assert_eq!(loaded.instruments.len(), module.instruments.len());

    assert_eq!(loaded.patterns[0].data[0][0].note, module.patterns[0].data[0][0].note);
    assert_eq!(loaded.patterns[0].data[1][1].effect, module.patterns[0].data[1][1].effect);
    assert_eq!(loaded.samples[0].name, module.samples[0].name);
    assert_eq!(loaded.samples[0].loop_start, 100);
    assert_eq!(loaded.samples[0].loop_end, 900);
}

#[test]
fn test_htk_automation_roundtrip() {
    let mut module = create_test_module();

    let track = AutomationTrack {
        id: 1,
        target: AutomationTarget::ChannelVolume,
        channel: Some(0),
        points: vec![
            AutomationPoint {
                order: 0,
                row: 0,
                value: 0.5,
                interp_to_next: InterpolationMode::Linear,
            },
            AutomationPoint {
                order: 0,
                row: 32,
                value: 1.0,
                interp_to_next: InterpolationMode::Smooth,
            },
            AutomationPoint {
                order: 1,
                row: 0,
                value: 0.25,
                interp_to_next: InterpolationMode::Hold,
            },
        ],
        default_interp: InterpolationMode::Linear,
        enabled: true,
    };

    module.automation_tracks.push(track);
    module.next_automation_id = 2;

    let data = formats::save_module(&module);
    let loaded = formats::load_module(&data).unwrap();

    assert_eq!(loaded.automation_tracks.len(), 1);
    assert_eq!(loaded.next_automation_id, 2);

    let t = &loaded.automation_tracks[0];
    assert_eq!(t.id, 1);
    assert_eq!(t.target, AutomationTarget::ChannelVolume);
    assert_eq!(t.channel, Some(0));
    assert_eq!(t.points.len(), 3);
    assert_eq!(t.points[0].value, 0.5);
    assert_eq!(t.points[1].interp_to_next, InterpolationMode::Smooth);
    assert_eq!(t.points[2].interp_to_next, InterpolationMode::Hold);
    assert!(t.enabled);
}

#[test]
fn test_htk_old_version_loads_empty_automation() {
    let module = create_test_module();
    let data = formats::save_module(&module);
    let loaded = formats::load_module(&data).unwrap();

    assert!(loaded.automation_tracks.is_empty());
    assert_eq!(loaded.next_automation_id, 0);
}
