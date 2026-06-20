use std::sync::Arc;
use crate::sequencer::module::Module;
use crate::sequencer::Sample;

#[derive(Clone, Debug, PartialEq)]
pub enum SliceMode {
    TimeDivisions,
    Onsets,
}

#[derive(Clone, Debug)]
pub struct SliceConfig {
    pub mode: SliceMode,
    pub bpm: f32,
    pub division: u8,
    pub sensitivity: f32,
    pub min_spacing_ms: f32,
    pub base_note: u8,
    pub target_instrument: Option<usize>,
    pub source_sample: usize,
}

impl Default for SliceConfig {
    fn default() -> Self {
        SliceConfig {
            mode: SliceMode::TimeDivisions,
            bpm: 120.0,
            division: 16,
            sensitivity: 0.5,
            min_spacing_ms: 30.0,
            base_note: 48,
            target_instrument: None,
            source_sample: 0,
        }
    }
}

pub struct SliceResult {
    pub slice_count: usize,
    pub sample_indices: Vec<usize>,
    pub target_instrument: usize,
}

/// Compute slice samples from a source sample without modifying the module.
/// Returns (slice_samples, slice_result) where slice_samples can be stored in
/// an undoable command.
pub fn compute_slices(
    module: &Module,
    config: &SliceConfig,
) -> Result<(Vec<Sample>, SliceResult), String> {
    let src = module.samples.get(config.source_sample)
        .ok_or_else(|| format!("Sample index {} out of range", config.source_sample))?;
    if src.data.is_empty() {
        return Err("Source sample has no data".into());
    }

    let regions = match config.mode {
        SliceMode::TimeDivisions => {
            crate::sequencer::slice_detector::slices_by_time(
                src.data.len(),
                src.sample_rate,
                config.bpm,
                config.division,
            )
        }
        SliceMode::Onsets => {
            crate::sequencer::slice_detector::slices_by_onset(
                &src.data,
                src.sample_rate,
                config.sensitivity,
                config.min_spacing_ms,
            )
        }
    };

    if regions.is_empty() {
        return Err("No slice regions generated".into());
    }

    let mut slice_samples: Vec<Sample> = Vec::with_capacity(regions.len());

    for region in &regions {
        let slice_data: Vec<f32> = if region.start_sample < region.end_sample {
            src.data[region.start_sample..region.end_sample].to_vec()
        } else {
            Vec::new()
        };

        if slice_data.is_empty() {
            continue;
        }

        let slice_sample = Sample {
            name: format!("{}_slice{}", src.name, slice_samples.len()),
            data: Arc::new(slice_data),
            sample_rate: src.sample_rate,
            bits_per_sample: src.bits_per_sample,
            loop_type: crate::sequencer::sample::LoopType::None,
            loop_start: 0,
            loop_end: 0,
            default_volume: src.default_volume,
            default_panning: src.default_panning,
            global_volume: src.global_volume,
            relative_note: src.relative_note,
            fine_tune: src.fine_tune,
            vibrato_speed: src.vibrato_speed,
            vibrato_depth: src.vibrato_depth,
            vibrato_rate: src.vibrato_rate,
            vibrato_waveform: src.vibrato_waveform.clone(),
            _flags: src._flags.clone(),
        };

        slice_samples.push(slice_sample);
    }

    if slice_samples.is_empty() {
        return Err("All slice regions were empty".into());
    }

    let target_instrument = config.target_instrument.unwrap_or_else(|| {
        let mut free = 1usize;
        while free < module.instruments.len()
            && !module.instruments[free].name.is_empty()
            && module.instruments[free].sample_map.iter().any(|&s| s != 0)
        {
            free += 1;
        }
        free.min(module.instruments.len().saturating_sub(1))
    });

    let slice_count = slice_samples.len();
    let sample_indices: Vec<usize> = (0..slice_count)
        .map(|i| module.samples.len() + i)
        .collect();

    Ok((slice_samples, SliceResult {
        slice_count,
        sample_indices,
        target_instrument,
    }))
}
