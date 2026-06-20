#[derive(Clone, Debug)]
pub struct SliceRegion {
    pub start_sample: usize,
    pub end_sample: usize,
}

pub fn slices_by_time(
    sample_len: usize,
    sample_rate: u32,
    bpm: f32,
    division: u8,
) -> Vec<SliceRegion> {
    if sample_len == 0 || sample_rate == 0 || bpm <= 0.0 || division == 0 {
        return vec![SliceRegion { start_sample: 0, end_sample: sample_len }];
    }
    let samples_per_beat = (60.0 / bpm) * sample_rate as f32;
    let beat_divisor = 4.0 / division as f32;
    let slice_len = (samples_per_beat / beat_divisor).round() as usize;

    if slice_len == 0 {
        return vec![SliceRegion { start_sample: 0, end_sample: sample_len }];
    }

    let mut regions = Vec::new();
    let mut pos = 0;
    while pos < sample_len {
        let end = (pos + slice_len).min(sample_len);
        regions.push(SliceRegion { start_sample: pos, end_sample: end });
        pos = end;
    }
    regions
}

pub fn slices_by_onset(
    data: &[f32],
    sample_rate: u32,
    sensitivity: f32,
    min_spacing_ms: f32,
) -> Vec<SliceRegion> {
    if data.is_empty() || sample_rate == 0 {
        return vec![SliceRegion { start_sample: 0, end_sample: data.len() }];
    }

    let sensitivity = sensitivity.clamp(0.01, 1.0);
    let min_spacing = (min_spacing_ms.max(1.0) / 1000.0 * sample_rate as f32).round() as usize;

    // Frame energy detection
    let frame_size = 512usize;
    let hop = frame_size / 2;
    let mut frame_energies: Vec<f32> = Vec::new();

    let mut frame_start = 0;
    while frame_start < data.len() {
        let frame_end = (frame_start + frame_size).min(data.len());
        let mut sum_sq = 0.0f32;
        let count = frame_end - frame_start;
        let mut i = frame_start;
        while i < frame_end {
            let s = data[i];
            sum_sq += s * s;
            i += 1;
        }
        frame_energies.push((sum_sq / count as f32).sqrt());
        frame_start += hop;
    }

    if frame_energies.is_empty() {
        return vec![SliceRegion { start_sample: 0, end_sample: data.len() }];
    }

    // Compute threshold: local average * sensitivity
    let window = 8usize;
    let mut onsets: Vec<usize> = Vec::new();

    for i in 0..frame_energies.len() {
        if i == 0 {
            continue;
        }
        let diff = frame_energies[i] - frame_energies[i - 1];
        if diff <= 0.0 {
            continue;
        }

        // Local average of the previous `window` frames (excluding current)
        let start = if i >= window { i - window } else { 0 };
        let mut avg = 0.0f32;
        let mut count = 0usize;
        let mut j = start;
        while j < i {
            avg += frame_energies[j];
            count += 1;
            j += 1;
        }
        avg /= count.max(1) as f32;

        let threshold = avg * (1.0 + sensitivity * 5.0);
        if diff > threshold {
            onsets.push(i);
        }
    }

    // Filter by min spacing (in frames, not samples)
    let min_frame_spacing = min_spacing / hop;
    let mut filtered: Vec<usize> = Vec::new();
    for &idx in &onsets {
        if filtered.is_empty() || idx - filtered.last().unwrap() >= min_frame_spacing {
            filtered.push(idx);
        }
    }

    if filtered.is_empty() {
        return vec![SliceRegion { start_sample: 0, end_sample: data.len() }];
    }

    // Convert frame indices to sample positions, build regions
    let mut regions: Vec<SliceRegion> = Vec::with_capacity(filtered.len());
    for &frame_idx in &filtered {
        let sample_pos = frame_idx * hop;
        regions.push(SliceRegion {
            start_sample: sample_pos,
            end_sample: sample_pos,
        });
    }

    // Fill end positions: each onset starts the next slice, last goes to data end
    for i in 0..regions.len() {
        if i + 1 < regions.len() {
            regions[i].end_sample = regions[i + 1].start_sample;
        } else {
            regions[i].end_sample = data.len();
        }
    }

    // Extend first region start to 0 if the first onset isn't at 0
    if regions[0].start_sample > 0 {
        regions.insert(0, SliceRegion {
            start_sample: 0,
            end_sample: regions[0].start_sample,
        });
    }

    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slices_by_time_basic() {
        let regions = slices_by_time(44100, 44100, 120.0, 4);
        assert!(regions.len() >= 2);
        assert_eq!(regions[0].start_sample, 0);
        assert_eq!(regions.last().unwrap().end_sample, 44100);
    }

    #[test]
    fn test_slices_by_time_edge_cases() {
        let empty = slices_by_time(0, 44100, 120.0, 4);
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].end_sample, 0);

        let no_bpm = slices_by_time(44100, 44100, 0.0, 4);
        assert_eq!(no_bpm.len(), 1);
    }

    #[test]
    fn test_slices_by_onset_simple() {
        let mut data = vec![0.0f32; 44100];
        // Add a transient at sample 10000
        for i in 0..500 {
            data[10000 + i] = 0.5;
        }
        let regions = slices_by_onset(&data, 44100, 0.5, 50.0);
        assert!(regions.len() >= 2);
        assert_eq!(regions[0].start_sample, 0);
        assert_eq!(regions.last().unwrap().end_sample, data.len());
    }

    #[test]
    fn test_slices_by_onset_empty_data() {
        let regions = slices_by_onset(&[], 44100, 0.5, 50.0);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].end_sample, 0);
    }

    #[test]
    fn test_all_regions_cover_full_sample() {
        let length = 48000;
        let data = vec![0.0f32; length];
        let regions = slices_by_onset(&data, 44100, 0.5, 50.0);
        assert_eq!(regions[0].start_sample, 0);
        assert_eq!(regions.last().unwrap().end_sample, length);
        for w in regions.windows(2) {
            assert_eq!(w[0].end_sample, w[1].start_sample);
        }
    }
}
