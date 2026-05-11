use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::sequencer::instrument::Instrument;
use crate::sequencer::sample::{LoopType, Sample, VibratoWaveform};

const HTI_MAGIC: &[u8; 4] = b"HTIN";
const HTI_VERSION: u32 = 1;

fn float_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

fn i16_to_float(sample: i16) -> f32 {
    sample as f32 / 32768.0
}

pub fn load_instrument(data: &[u8]) -> FormatResult<(Instrument, Vec<Sample>)> {
    if data.len() < 12 {
        return Err(FormatError::TruncatedFile {
            expected_size: 12,
            actual_size: data.len(),
        });
    }

    if &data[0..4] != HTI_MAGIC {
        return Err(FormatError::InvalidHeader {
            expected: "HTIN magic".to_string(),
            found: [data[0], data[1], data[2], data[3]],
        });
    }

    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if version > HTI_VERSION {
        return Err(FormatError::InvalidHeader {
            expected: format!("HTI version <= {}", HTI_VERSION),
            found: version.to_le_bytes(),
        });
    }

    let _flags = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

    let mut pos = 12usize;

    let metadata_len = u32::from_le_bytes([
        data[pos], data[pos + 1], data[pos + 2], data[pos + 3],
    ]) as usize;
    pos += 4;

    if pos + metadata_len > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: pos + metadata_len,
            actual_size: data.len(),
        });
    }

    let instrument: Instrument = bincode::deserialize(&data[pos..pos + metadata_len])
        .map_err(|e| FormatError::ParseError(e.to_string()))?;
    pos += metadata_len;

    let num_samples = u32::from_le_bytes([
        data[pos], data[pos + 1], data[pos + 2], data[pos + 3],
    ]) as usize;
    pos += 4;

    let mut samples = Vec::with_capacity(num_samples);

    for _ in 0..num_samples {
        let pcm_data_size = u32::from_le_bytes([
            data[pos], data[pos + 1], data[pos + 2], data[pos + 3],
        ]) as usize;
        pos += 4;

        let metadata_size = u32::from_le_bytes([
            data[pos], data[pos + 1], data[pos + 2], data[pos + 3],
        ]) as usize;
        pos += 4;

        let sample_metadata: SampleMetadata = bincode::deserialize(&data[pos..pos + metadata_size])
            .map_err(|e| FormatError::ParseError(e.to_string()))?;
        pos += metadata_size;

        if pos + pcm_data_size > data.len() {
            return Err(FormatError::TruncatedFile {
                expected_size: pos + pcm_data_size,
                actual_size: data.len(),
            });
        }

        let pcm_data = &data[pos..pos + pcm_data_size];
        pos += pcm_data_size;

        let sample = decode_sample_from_metadata(sample_metadata, pcm_data);
        samples.push(sample);
    }

    Ok((instrument, samples))
}

pub fn save_instrument(instrument: &Instrument, samples: &[Sample]) -> FormatResult<Vec<u8>> {
    let mut data = Vec::new();

    data.extend_from_slice(HTI_MAGIC);
    data.extend_from_slice(&HTI_VERSION.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let metadata = bincode::serialize(instrument)
        .map_err(|e| FormatError::SerializeError(e.to_string()))?;
    data.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    data.extend_from_slice(&metadata);

    data.extend_from_slice(&(samples.len() as u32).to_le_bytes());

    for sample in samples {
        let (pcm_data, metadata) = encode_sample_to_metadata(sample);

        let sample_metadata = bincode::serialize(&metadata)
            .map_err(|e| FormatError::SerializeError(e.to_string()))?;

        data.extend_from_slice(&(pcm_data.len() as u32).to_le_bytes());
        data.extend_from_slice(&(sample_metadata.len() as u32).to_le_bytes());
        data.extend_from_slice(&sample_metadata);
        data.extend_from_slice(&pcm_data);
    }

    Ok(data)
}

pub fn detect_hti(data: &[u8]) -> bool {
    data.len() >= 4 && &data[0..4] == HTI_MAGIC
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SampleMetadata {
    name: String,
    sample_rate: u32,
    bits_per_sample: u8,
    loop_type: LoopType,
    loop_start: u32,
    loop_end: u32,
    default_volume: u8,
    default_panning: u8,
    global_volume: u8,
    relative_note: i8,
    fine_tune: i8,
    vibrato_speed: u8,
    vibrato_depth: u8,
    vibrato_rate: u8,
    vibrato_waveform: VibratoWaveform,
}

fn decode_sample_from_metadata(meta: SampleMetadata, pcm_data: &[u8]) -> Sample {
    let num_samples = pcm_data.len() / 2;
    let mut float_data = Vec::with_capacity(num_samples);

    for i in 0..num_samples {
        let low = pcm_data[i * 2] as i16;
        let high = pcm_data[i * 2 + 1] as i16;
        let sample = i16::from_le_bytes([low as u8, high as u8]);
        float_data.push(i16_to_float(sample));
    }

    Sample {
        name: meta.name,
        data: Arc::new(float_data),
        sample_rate: meta.sample_rate,
        bits_per_sample: meta.bits_per_sample,
        loop_type: meta.loop_type,
        loop_start: meta.loop_start as usize,
        loop_end: meta.loop_end as usize,
        default_volume: meta.default_volume,
        default_panning: meta.default_panning,
        global_volume: meta.global_volume,
        relative_note: meta.relative_note,
        fine_tune: meta.fine_tune,
        vibrato_speed: meta.vibrato_speed,
        vibrato_depth: meta.vibrato_depth,
        vibrato_rate: meta.vibrato_rate,
        vibrato_waveform: meta.vibrato_waveform,
        _flags: crate::sequencer::sample::SampleFlags::default(),
    }
}

fn encode_sample_to_metadata(sample: &Sample) -> (Vec<u8>, SampleMetadata) {
    let mut pcm_data = Vec::with_capacity(sample.data.len() * 2);

    for &float_sample in sample.data.iter() {
        let i16_val = float_to_i16(float_sample);
        pcm_data.extend_from_slice(&i16_val.to_le_bytes());
    }

    let metadata = SampleMetadata {
        name: sample.name.clone(),
        sample_rate: sample.sample_rate,
        bits_per_sample: sample.bits_per_sample,
        loop_type: sample.loop_type,
        loop_start: sample.loop_start as u32,
        loop_end: sample.loop_end as u32,
        default_volume: sample.default_volume,
        default_panning: sample.default_panning,
        global_volume: sample.global_volume,
        relative_note: sample.relative_note,
        fine_tune: sample.fine_tune,
        vibrato_speed: sample.vibrato_speed,
        vibrato_depth: sample.vibrato_depth,
        vibrato_rate: sample.vibrato_rate,
        vibrato_waveform: sample.vibrato_waveform,
    };

    (pcm_data, metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::instrument::Envelope;
    use crate::sequencer::instrument::EnvelopeFlags;
    use crate::sequencer::instrument::EnvelopePoint;

    #[test]
    fn detect_hti_magic() {
        let valid = [b'H', b'T', b'I', b'N', 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(detect_hti(&valid));

        let invalid = [b'X', b'M', b'I', b'S', 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(!detect_hti(&invalid));
    }

    #[test]
    fn roundtrip_basic_instrument() {
        let mut inst = Instrument::default();
        inst.name = "Test Instrument".to_string();
        inst.fade_out = 128;
        inst.global_volume = 100;

        let samples: Vec<Sample> = vec![];

        let data = save_instrument(&inst, &samples).unwrap();
        let (loaded_inst, loaded_samples) = load_instrument(&data).unwrap();

        assert_eq!(loaded_inst.name, inst.name);
        assert_eq!(loaded_inst.fade_out, inst.fade_out);
        assert_eq!(loaded_inst.global_volume, inst.global_volume);
        assert!(loaded_samples.is_empty());
    }

    #[test]
    fn roundtrip_instrument_with_sample() {
        let inst = Instrument::default();

        let mut sample = Sample::default();
        sample.name = "Kick".to_string();
        sample.data = Arc::new(vec![0.0, 0.5, 1.0, 0.5, 0.0]);
        sample.sample_rate = 44100;
        sample.default_volume = 64;
        sample.loop_type = LoopType::None;

        let data = save_instrument(&inst, &[sample.clone()]).unwrap();
        let (loaded_inst, loaded_samples) = load_instrument(&data).unwrap();

        assert_eq!(loaded_inst.name, inst.name);
        assert_eq!(loaded_samples.len(), 1);
        assert_eq!(loaded_samples[0].name, sample.name);
        assert_eq!(loaded_samples[0].sample_rate, sample.sample_rate);
        assert_eq!(loaded_samples[0].data.len(), sample.data.len());
        for (a, b) in loaded_samples[0].data.iter().zip(sample.data.iter()) {
            assert!((a - b).abs() < 0.001);
        }
    }

    #[test]
    fn roundtrip_instrument_with_envelopes() {
        let mut inst = Instrument::default();
        inst.name = "Synth Lead".to_string();
        inst.volume_envelope = Some(Envelope {
            points: vec![
                EnvelopePoint { tick: 0, value: 0 },
                EnvelopePoint { tick: 10, value: 64 },
                EnvelopePoint { tick: 50, value: 32 },
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
        });
        inst.filter_envelope = Some(Envelope {
            points: vec![
                EnvelopePoint { tick: 0, value: 64 },
                EnvelopePoint { tick: 30, value: 32 },
            ],
            sustain_point: None,
            loop_start: None,
            loop_end: None,
            flags: EnvelopeFlags {
                enabled: true,
                sustain: false,
                loop_: false,
                carry: false,
            },
        });

        let samples: Vec<Sample> = vec![];

        let data = save_instrument(&inst, &samples).unwrap();
        let (loaded_inst, _) = load_instrument(&data).unwrap();

        assert_eq!(loaded_inst.name, inst.name);

        let loaded_vol_env = loaded_inst.volume_envelope.unwrap();
        assert_eq!(loaded_vol_env.points.len(), 3);
        assert_eq!(loaded_vol_env.sustain_point, Some(1));
        assert!(loaded_vol_env.flags.enabled);
        assert!(loaded_vol_env.flags.sustain);

        let loaded_filter_env = loaded_inst.filter_envelope.unwrap();
        assert_eq!(loaded_filter_env.points.len(), 2);
        assert!(loaded_filter_env.flags.enabled);
    }

    #[test]
    fn roundtrip_multiple_samples() {
        let inst = Instrument::default();

        let mut sample1 = Sample::default();
        sample1.name = "Bass".to_string();
        sample1.data = Arc::new(vec![0.5; 100]);
        sample1.sample_rate = 44100;
        sample1.default_volume = 48;

        let mut sample2 = Sample::default();
        sample2.name = "Lead".to_string();
        sample2.data = Arc::new(vec![1.0; 200]);
        sample2.sample_rate = 44100;
        sample2.default_volume = 64;
        sample2.loop_type = LoopType::Forward;
        sample2.loop_start = 0;
        sample2.loop_end = 200;

        let samples = vec![sample1.clone(), sample2.clone()];

        let data = save_instrument(&inst, &samples).unwrap();
        let (loaded_inst, loaded_samples) = load_instrument(&data).unwrap();

        assert_eq!(loaded_inst.name, inst.name);
        assert_eq!(loaded_samples.len(), 2);

        assert_eq!(loaded_samples[0].name, sample1.name);
        assert_eq!(loaded_samples[0].default_volume, sample1.default_volume);

        assert_eq!(loaded_samples[1].name, sample2.name);
        assert_eq!(loaded_samples[1].loop_type, LoopType::Forward);
        assert_eq!(loaded_samples[1].loop_end, sample2.loop_end);
    }
}