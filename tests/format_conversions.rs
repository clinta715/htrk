use htrk::sequencer::{Module, ModuleFormat, Pattern, Instrument, Sample, Note, Effect, Cell};
use htrk::formats;
use std::sync::Arc;

fn create_test_module() -> Module {
    let mut module = Module::default();
    module.name = "Test Module".to_string();
    
    // Add a pattern
    let mut pat = Pattern::new(64);
    pat.data[0][0].note = Note::On(60); // C-5
    pat.data[0][0].instrument = Some(1);
    pat.data[0][0].volume = Some(64);
    pat.data[0][0].effect = Effect::SetSpeed { speed: 3 };
    
    pat.data[1][1].note = Note::On(64); // E-5
    pat.data[1][1].effect = Effect::VolumeSlide { up: 1, down: 0 };
    
    module.patterns.push(pat);
    module.order_list = vec![0];
    
    // Add a sample
    let mut sample = Sample::default();
    sample.name = "Test Sample".to_string();
    sample.data = Arc::new(vec![0.0; 1000]);
    sample.loop_type = htrk::sequencer::LoopType::Forward;
    sample.loop_start = 100;
    sample.loop_end = 900;
    module.samples.push(sample);
    
    // Add an instrument
    let mut inst = Instrument::default();
    inst.name = "Test Instrument".to_string();
    inst.sample_map[60] = 1;
    module.instruments.push(inst);
    
    module
}

#[test]
fn test_it_roundtrip() {
    let module = create_test_module();
    let data = formats::save_module(&module, ModuleFormat::IT);
    let loaded = formats::load_module(&data).unwrap();
    
    assert_eq!(loaded.name, module.name);
    assert_eq!(loaded.patterns.len(), module.patterns.len());
    assert_eq!(loaded.samples.len(), module.samples.len());
    assert_eq!(loaded.instruments.len(), module.instruments.len());
    
    // Check pattern data
    assert_eq!(loaded.patterns[0].data[0][0].note, module.patterns[0].data[0][0].note);
    assert_eq!(loaded.patterns[0].data[1][1].effect, module.patterns[0].data[1][1].effect);
}

#[test]
fn test_xm_roundtrip() {
    let module = create_test_module();
    let data = formats::save_module(&module, ModuleFormat::XM);
    let loaded = formats::load_module(&data).unwrap();
    
    // XM might truncate name to 20 chars
    assert!(module.name.starts_with(&loaded.name));
    assert_eq!(loaded.patterns.len(), module.patterns.len());
    
    // Check pattern data
    assert_eq!(loaded.patterns[0].data[0][0].note, module.patterns[0].data[0][0].note);
}

#[test]
fn test_s3m_roundtrip() {
    let mut module = create_test_module();
    module.format = ModuleFormat::S3M;
    let data = formats::save_module(&module, ModuleFormat::S3M);
    let loaded = formats::load_module(&data).unwrap();
    
    assert!(module.name.starts_with(&loaded.name));
    assert_eq!(loaded.patterns.len(), module.patterns.len());
}

#[test]
fn test_it_to_xm_conversion() {
    let module = create_test_module();
    let xm_data = formats::save_module(&module, ModuleFormat::XM);
    let xm_loaded = formats::load_module(&xm_data).unwrap();
    
    assert_eq!(xm_loaded.format, ModuleFormat::XM);
    
    let it_data = formats::save_module(&xm_loaded, ModuleFormat::IT);
    let it_loaded = formats::load_module(&it_data).unwrap();
    
    // Verify essential data survived IT -> XM -> IT
    assert_eq!(it_loaded.patterns[0].data[0][0].note, Note::On(60));
}
