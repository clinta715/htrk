use htrk::sequencer::{Module, ModuleFormat, Pattern, Instrument, Sample, Note, Effect, Cell};
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
