use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(dead_code)]
pub enum LoopType {
    None,
    Forward,
    PingPong,
    Backward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct SampleFlags {
    pub is_stereo: bool,
    pub is_16bit: bool,
    pub is_compressed: bool,
    pub has_trailing_byte: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VibratoWaveform {
    Sine,
    Square,
    Ramp,
    Random,
}

impl Default for VibratoWaveform {
    fn default() -> Self {
        VibratoWaveform::Sine
    }
}

mod arc_vec_f32_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(value: &Arc<Vec<f32>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let v: &Vec<f32> = value.as_ref();
        v.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<Vec<f32>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: Vec<f32> = Vec::deserialize(deserializer)?;
        Ok(Arc::new(v))
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Sample {
    pub name: String,

    #[serde(with = "arc_vec_f32_serde")]
    pub data: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub bits_per_sample: u8,

    pub loop_type: LoopType,
    pub loop_start: usize,
    pub loop_end: usize,

    pub default_volume: u8,
    pub default_panning: u8,
    pub global_volume: u8,

    pub relative_note: i8,
    pub fine_tune: i8,

    pub vibrato_speed: u8,
    pub vibrato_depth: u8,
    pub vibrato_rate: u8,
    pub vibrato_waveform: VibratoWaveform,

    pub _flags: SampleFlags,
}

impl Default for Sample {
    fn default() -> Self {
        Sample {
            name: String::new(),
            data: Arc::new(Vec::new()),
            sample_rate: 0,
            bits_per_sample: 0,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_default_values() {
        let s = Sample::default();
        assert_eq!(s.default_volume, 64);
        assert_eq!(s.default_panning, 32);
        assert_eq!(s.loop_type, LoopType::None);
        assert_eq!(s.relative_note, 0);
        assert!(s.data.is_empty());
    }

    #[test]
    fn loop_type_variants() {
        assert_ne!(LoopType::None, LoopType::Forward);
        assert_ne!(LoopType::Forward, LoopType::PingPong);
        assert_ne!(LoopType::PingPong, LoopType::Backward);
    }

    #[test]
    fn sample_with_data() {
        let data = Arc::new(vec![0.0, 0.5, 1.0, 0.5, 0.0]);
        let mut s = Sample::default();
        s.data = data.clone();
        s.sample_rate = 44100;
        s.bits_per_sample = 16;
        s.loop_type = LoopType::Forward;
        s.loop_start = 0;
        s.loop_end = 5;
        assert_eq!(s.data.len(), 5);
        assert_eq!(s.sample_rate, 44100);
    }
}
