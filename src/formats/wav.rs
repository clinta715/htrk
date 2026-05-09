use std::io::Cursor;
use std::sync::Arc;

use crate::errors::{FormatError, FormatResult};
use crate::sequencer::sample::{LoopType, Sample, SampleFlags, VibratoWaveform};

pub fn import_wav(data: &[u8]) -> FormatResult<Sample> {
    let cursor = Cursor::new(data);
    let mut reader = hound::WavReader::new(cursor)
        .map_err(|e| FormatError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())))?;

    let spec = reader.spec();
    let bits = spec.bits_per_sample;
    let sample_rate = spec.sample_rate;
    let channels = spec.channels;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            if bits == 8 {
                let raw: Vec<i8> = reader
                    .samples()
                    .filter_map(|s: Result<i8, _>| s.ok())
                    .collect();
                raw.chunks(channels as usize)
                    .map(|ch| {
                        let avg = ch.iter().map(|&s| f32::from(s) / 128.0).sum::<f32>()
                            / ch.len() as f32;
                        avg
                    })
                    .collect()
            } else if bits == 16 {
                let raw: Vec<i16> = reader
                    .samples()
                    .filter_map(|s: Result<i16, _>| s.ok())
                    .collect();
                raw.chunks(channels as usize)
                    .map(|ch| {
                        let avg = ch.iter().map(|&s| f32::from(s) / 32768.0).sum::<f32>()
                            / ch.len() as f32;
                        avg
                    })
                    .collect()
            } else if bits == 24 {
                let raw: Vec<i32> = reader
                    .samples()
                    .filter_map(|s: Result<i32, _>| s.ok())
                    .collect();
                raw.chunks(channels as usize)
                    .map(|ch| {
                        let avg = ch.iter().map(|&s| s as f32 / 8388608.0).sum::<f32>()
                            / ch.len() as f32;
                        avg
                    })
                    .collect()
            } else if bits == 32 {
                let raw: Vec<i32> = reader
                    .samples()
                    .filter_map(|s: Result<i32, _>| s.ok())
                    .collect();
                raw.chunks(channels as usize)
                    .map(|ch| {
                        let avg = ch.iter().map(|&s| s as f32 / 2147483648.0).sum::<f32>()
                            / ch.len() as f32;
                        avg
                    })
                    .collect()
            } else {
                return Err(FormatError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Unsupported integer bit depth: {}", bits),
                )));
            }
        }
        hound::SampleFormat::Float => {
            let raw: Vec<f32> = reader
                .samples()
                .filter_map(|s: Result<f32, _>| s.ok())
                .collect();
            raw.chunks(channels as usize)
                .map(|ch| {
                    let avg = ch.iter().sum::<f32>() / ch.len() as f32;
                    avg
                })
                .collect()
        }
    };

    Ok(Sample {
        name: String::new(),
        data: Arc::new(samples),
        sample_rate,
        bits_per_sample: bits as u8,
        loop_type: LoopType::None,
        loop_start: 0,
        loop_end: 0,
        default_volume: 64,
        default_panning: 32,
        global_volume: 64,
        relative_note: 0,
        fine_tune: 0,
        vibrato_speed: 0,
        vibrato_depth: 0,
        vibrato_rate: 0,
        vibrato_waveform: VibratoWaveform::default(),
        _flags: SampleFlags::default(),
    })
}

#[allow(dead_code)]
pub fn export_wav(sample: &Sample) -> Vec<u8> {
    let rate = if sample.sample_rate == 0 {
        44100
    } else {
        sample.sample_rate
    };

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buf = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut buf, spec).unwrap();
        for &s in sample.data.iter() {
            let clamped = s.clamp(-1.0, 1.0);
            let val = (clamped * 32768.0) as i16;
            writer.write_sample(val).unwrap();
        }
        writer.finalize().unwrap();
    }

    buf.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_sine_wave() {
        let rate = 44100u32;
        let num_samples = 256usize;
        let data: Vec<f32> = (0..num_samples)
            .map(|i| (i as f32 / num_samples as f32 * 2.0 * std::f32::consts::PI).sin())
            .collect();

        let sample = Sample {
            name: String::from("TestSine"),
            data: Arc::new(data.clone()),
            sample_rate: rate,
            bits_per_sample: 16,
            loop_type: LoopType::None,
            loop_start: 0,
            loop_end: 0,
            default_volume: 64,
            default_panning: 32,
            global_volume: 64,
            relative_note: 0,
            fine_tune: 0,
            vibrato_speed: 0,
            vibrato_depth: 0,
            vibrato_rate: 0,
            vibrato_waveform: VibratoWaveform::default(),
            _flags: SampleFlags::default(),
        };

        let wav_bytes = export_wav(&sample);
        assert!(!wav_bytes.is_empty());
        assert_eq!(&wav_bytes[0..4], b"RIFF");

        let imported = import_wav(&wav_bytes).unwrap();
        assert_eq!(imported.sample_rate, rate);
        assert_eq!(imported.data.len(), num_samples);
        assert_eq!(imported.name, "");

        for i in 0..num_samples {
            let original_i16 = (data[i].clamp(-1.0, 1.0) * 32767.0) as i16;
            let roundtrip_f32 = f32::from(original_i16) / 32768.0;
            let diff = (imported.data[i] - roundtrip_f32).abs();
            assert!(diff < 0.001, "Mismatch at sample {}: {} vs {}", i, imported.data[i], roundtrip_f32);
        }
    }

    #[test]
    fn round_trip_silence() {
        let data = vec![0.0f32; 100];
        let sample = Sample {
            name: String::from("Silence"),
            data: Arc::new(data),
            sample_rate: 22050,
            bits_per_sample: 16,
            ..Sample::default()
        };

        let wav_bytes = export_wav(&sample);
        let imported = import_wav(&wav_bytes).unwrap();
        assert_eq!(imported.sample_rate, 22050);
        assert_eq!(imported.data.len(), 100);
        for &s in imported.data.iter() {
            assert!(s.abs() < 0.001);
        }
    }

    #[test]
    fn export_default_sample_rate() {
        let sample = Sample {
            data: Arc::new(vec![0.0; 10]),
            sample_rate: 0,
            ..Sample::default()
        };
        let wav_bytes = export_wav(&sample);
        let imported = import_wav(&wav_bytes).unwrap();
        assert_eq!(imported.sample_rate, 44100);
    }

    #[test]
    fn round_trip_clipping() {
        let data = vec![-2.0f32, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0];
        let sample = Sample {
            data: Arc::new(data),
            sample_rate: 44100,
            bits_per_sample: 16,
            ..Sample::default()
        };
        let wav_bytes = export_wav(&sample);
        let imported = import_wav(&wav_bytes).unwrap();
        assert!(imported.data[0] >= -1.0);
        assert!(imported.data[imported.data.len() - 1] <= 1.0);
        assert!(imported.data[3].abs() < 0.001);
    }
}
