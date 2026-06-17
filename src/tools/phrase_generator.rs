use crate::sequencer::pattern::Cell;
use crate::sequencer::Note;
use super::scale::{self, Scale};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenMode {
    Melodic,
    Euclidean,
    Drum,
    Chord,
}

impl GenMode {
    pub fn name(&self) -> &'static str {
        match self {
            GenMode::Melodic => "Melodic",
            GenMode::Euclidean => "Euclidean",
            GenMode::Drum => "Drum",
            GenMode::Chord => "Chord",
        }
    }

    pub fn all() -> &'static [GenMode] {
        &[GenMode::Melodic, GenMode::Euclidean, GenMode::Drum, GenMode::Chord]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordType {
    Triad,
    Seventh,
    Sus2,
    Sus4,
}

impl ChordType {
    pub fn name(&self) -> &'static str {
        match self {
            ChordType::Triad => "Triad",
            ChordType::Seventh => "7th",
            ChordType::Sus2 => "Sus2",
            ChordType::Sus4 => "Sus4",
        }
    }

    pub fn all() -> &'static [ChordType] {
        &[ChordType::Triad, ChordType::Seventh, ChordType::Sus2, ChordType::Sus4]
    }

    pub fn intervals(&self) -> &'static [i8] {
        match self {
            ChordType::Triad => &[0, 4, 7],
            ChordType::Seventh => &[0, 4, 7, 11],
            ChordType::Sus2 => &[0, 2, 7],
            ChordType::Sus4 => &[0, 5, 7],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progression {
    OneFourFiveOne,
    OneFiveSixFour,
    OneSixFourFive,
    OneThreeFourFive,
    Circle,
}

impl Progression {
    pub fn name(&self) -> &'static str {
        match self {
            Progression::OneFourFiveOne => "I-IV-V-I",
            Progression::OneFiveSixFour => "I-V-vi-IV",
            Progression::OneSixFourFive => "I-vi-IV-V",
            Progression::OneThreeFourFive => "I-iii-IV-V",
            Progression::Circle => "Circle of 5ths",
        }
    }

    pub fn all() -> &'static [Progression] {
        &[
            Progression::OneFourFiveOne,
            Progression::OneFiveSixFour,
            Progression::OneSixFourFive,
            Progression::OneThreeFourFive,
            Progression::Circle,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct PhraseParams {
    pub mode: GenMode,
    pub scale: Scale,
    pub root: u8,
    pub octave_min: u8,
    pub octave_max: u8,
    pub density: f32,
    pub step_size: u8,
    pub seed: u64,
    pub instrument: Option<u8>,
    pub pulses: usize,
    pub rotation: usize,
    pub kick_ch: usize,
    pub snare_ch: usize,
    pub hat_ch: usize,
    pub chord_type: ChordType,
    pub progression: Progression,
    pub bars_per_chord: u8,
    pub chord_channels: [usize; 4],
}

impl Default for PhraseParams {
    fn default() -> Self {
        PhraseParams {
            mode: GenMode::Melodic,
            scale: Scale::Major,
            root: 0,
            octave_min: 3,
            octave_max: 5,
            density: 0.3,
            step_size: 3,
            seed: 0,
            instrument: None,
            pulses: 8,
            rotation: 0,
            kick_ch: 0,
            snare_ch: 1,
            hat_ch: 2,
            chord_type: ChordType::Triad,
            progression: Progression::OneFourFiveOne,
            bars_per_chord: 4,
            chord_channels: [0, 1, 2, 3],
        }
    }
}

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed.wrapping_add(1) | 1)
    }

    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }

    fn f32(&mut self) -> f32 {
        (self.next() >> 8) as f32 * 1.0 / 16777216.0
    }
}

pub fn generate_phrase(
    params: &PhraseParams,
    start_row: usize,
    end_row: usize,
    num_channels: usize,
) -> Vec<(usize, usize, Cell)> {
    match params.mode {
        GenMode::Melodic => generate_melodic(params, start_row, end_row),
        GenMode::Euclidean => generate_euclidean(params, start_row, end_row),
        GenMode::Drum => generate_drum(params, start_row, end_row, num_channels),
        GenMode::Chord => generate_chord(params, start_row, end_row, num_channels),
    }
}

fn note_cell(note_key: u8, instrument: Option<u8>) -> Cell {
    Cell {
        note: Note::On(note_key),
        instrument,
        volume: None,
        volume_effect: None,
        effect: Default::default(),
    }
}

fn generate_melodic(params: &PhraseParams, start_row: usize, end_row: usize) -> Vec<(usize, usize, Cell)> {
    let mut rng = Lcg::new(params.seed);
    let num_rows = end_row.saturating_sub(start_row) + 1;
    let mut result = Vec::new();

    let intervals = params.scale.intervals();
    let num_intervals = intervals.len() as i32;

    let mid_octave = (params.octave_min + params.octave_max) / 2;
    let mut current_degree: i32 = rng.next() as i32 % num_intervals;

    for row in start_row..=end_row {
        let pct = row.saturating_sub(start_row) as f32 / num_rows as f32;
        let density = params.density * (1.0 - pct * 0.3);

        if rng.f32() < density {
            if rng.f32() < 0.05 {
                current_degree = rng.next() as i32 % num_intervals;
            } else {
                let step = (rng.next() as i32 % (params.step_size as i32 * 2 + 1)) - params.step_size as i32;
                current_degree += step;
            }
            current_degree = current_degree.clamp(-num_intervals * 4, num_intervals * 4);

            let raw_oct = if current_degree < 0 {
                mid_octave.saturating_sub(((-current_degree) as u32 / num_intervals as u32).min(3) as u8)
            } else {
                mid_octave + ((current_degree as u32) / num_intervals as u32).min(3) as u8
            };
            let octave = raw_oct.clamp(params.octave_min, params.octave_max);
            let degree_idx = current_degree.rem_euclid(num_intervals) as usize;
            let note_key = (params.root as i32 + (octave as i32) * 12 + intervals[degree_idx] as i32).clamp(0, 119) as u8;

            result.push((row, 0, note_cell(note_key, params.instrument)));
        }
    }

    result
}

fn generate_euclidean(params: &PhraseParams, start_row: usize, end_row: usize) -> Vec<(usize, usize, Cell)> {
    let num_rows = end_row.saturating_sub(start_row) + 1;
    let mut rng = Lcg::new(params.seed);
    let mut result = Vec::new();

    let pattern = scale::euclidean(num_rows, params.pulses.min(num_rows), params.rotation);
    let intervals = params.scale.intervals();
    let num_intervals = intervals.len();
    let mut degree: usize = 0;

    for (i, &active) in pattern.iter().enumerate() {
        if active {
            let row = start_row + i;
            let octave = params.octave_min + (rng.next() % (params.octave_max - params.octave_min + 1) as u32) as u8;
            let note_key = (params.root as i32 + (octave as i32) * 12 + intervals[degree % num_intervals] as i32).clamp(0, 119) as u8;
            result.push((row, 0, note_cell(note_key, params.instrument)));
            degree += 1 + (rng.next() % 3) as usize;
        }
    }

    result
}

fn generate_drum(
    params: &PhraseParams,
    start_row: usize,
    end_row: usize,
    num_channels: usize,
) -> Vec<(usize, usize, Cell)> {
    let num_rows = end_row.saturating_sub(start_row) + 1;
    let mut result = Vec::new();

    let kick_pat = scale::euclidean(num_rows, (num_rows / 4).max(1), 0);
    let snare_pat = scale::euclidean(num_rows, (num_rows / 4).max(1), (num_rows / 8).max(1));
    let hat_pat = scale::euclidean(num_rows, (num_rows / 2).max(1), 0);

    let kick_ch = if params.kick_ch < num_channels { Some(params.kick_ch) } else { None };
    let snare_ch = if params.snare_ch < num_channels { Some(params.snare_ch) } else { None };
    let hat_ch = if params.hat_ch < num_channels { Some(params.hat_ch) } else { None };

    for (i, &active) in kick_pat.iter().enumerate() {
        if active {
            if let Some(ch) = kick_ch {
                result.push((start_row + i, ch, note_cell(36, params.instrument)));
            }
        }
    }
    for (i, &active) in snare_pat.iter().enumerate() {
        if active {
            if let Some(ch) = snare_ch {
                result.push((start_row + i, ch, note_cell(38, params.instrument)));
            }
        }
    }
    for (i, &active) in hat_pat.iter().enumerate() {
        if active {
            if let Some(ch) = hat_ch {
                result.push((start_row + i, ch, note_cell(42, params.instrument)));
            }
        }
    }

    result
}

fn chord_progression_degrees(progression: Progression, scale: Scale) -> Vec<i32> {
    let is_major = matches!(scale, Scale::Major | Scale::PentatonicMajor | Scale::Dorian);
    match progression {
        Progression::OneFourFiveOne => vec![0, 3, 4, 0],
        Progression::OneFiveSixFour => {
            if is_major { vec![0, 4, 5, 3] } else { vec![0, 4, 3, 5] }
        }
        Progression::OneSixFourFive => {
            if is_major { vec![0, 5, 3, 4] } else { vec![0, 3, 5, 4] }
        }
        Progression::OneThreeFourFive => {
            if is_major { vec![0, 2, 3, 4] } else { vec![0, 2, 3, 4] }
        }
        Progression::Circle => vec![0, 1, 2, 3, 4, 5, 6],
    }
}

fn generate_chord(
    params: &PhraseParams,
    start_row: usize,
    end_row: usize,
    num_channels: usize,
) -> Vec<(usize, usize, Cell)> {
    let intervals = params.scale.intervals();
    let num_degrees = intervals.len() as i32;
    let chord_intervals = params.chord_type.intervals();
    let degrees = chord_progression_degrees(params.progression, params.scale);
    let rows_per_bar = 4;
    let rows_per_chord = params.bars_per_chord as usize * rows_per_bar;
    let mut result = Vec::new();

    for (chord_idx, &degree) in degrees.iter().enumerate() {
        let row_start = start_row + chord_idx * rows_per_chord;
        if row_start > end_row {
            break;
        }
        let row_end = (row_start + rows_per_chord).saturating_sub(1).min(end_row);
        let degree_offset = degree.rem_euclid(num_degrees) as usize;
        let root_note = (params.root as i32
            + params.octave_min as i32 * 12
            + intervals[degree_offset] as i32)
            .clamp(0, 119) as u8;

        for &interval in chord_intervals {
            let note = (root_note as i32 + interval as i32).clamp(0, 119) as u8;
            let ch_idx = chord_intervals.iter().position(|&v| v == interval).unwrap_or(0);
            let ch = params.chord_channels[ch_idx % params.chord_channels.len()];
            if ch >= num_channels {
                continue;
            }
            result.push((row_start, ch, note_cell(note, params.instrument)));

            if row_end > row_start {
                let retrigger_row = row_start + rows_per_chord / 2;
                if retrigger_row <= row_end {
                    result.push((retrigger_row, ch, note_cell(note, params.instrument)));
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_melodic_generates_some_notes() {
        let params = PhraseParams {
            density: 1.0,
            ..Default::default()
        };
        let notes = generate_melodic(&params, 0, 15);
        assert!(!notes.is_empty());
    }

    #[test]
    fn test_euclidean_generates_correct_count() {
        let params = PhraseParams {
            pulses: 4,
            ..Default::default()
        };
        let notes = generate_euclidean(&params, 0, 15);
        assert_eq!(notes.len(), 4);
    }

    #[test]
    fn test_drum_skips_out_of_range_channel() {
        let params = PhraseParams {
            kick_ch: 0,
            snare_ch: 1,
            hat_ch: 99,
            ..Default::default()
        };
        let notes = generate_drum(&params, 0, 15, 2);
        assert!(!notes.is_empty());
        assert!(notes.iter().all(|(_, ch, _)| *ch < 2));
    }

    #[test]
    fn test_chord_triad_generates_notes() {
        let params = PhraseParams {
            mode: GenMode::Chord,
            chord_type: ChordType::Triad,
            progression: Progression::OneFourFiveOne,
            bars_per_chord: 2,
            ..Default::default()
        };
        let notes = generate_chord(&params, 0, 31, 4);
        assert!(notes.len() >= 3 * 4);
    }

    #[test]
    fn test_chord_seventh_generates_four_notes() {
        let params = PhraseParams {
            mode: GenMode::Chord,
            chord_type: ChordType::Seventh,
            progression: Progression::OneFourFiveOne,
            bars_per_chord: 1,
            ..Default::default()
        };
        let notes = generate_chord(&params, 0, 15, 4);
        assert!(notes.len() >= 4);
    }

    #[test]
    fn test_chord_circle_of_fifths() {
        let params = PhraseParams {
            mode: GenMode::Chord,
            chord_type: ChordType::Triad,
            progression: Progression::Circle,
            bars_per_chord: 2,
            ..Default::default()
        };
        let notes = generate_chord(&params, 0, 55, 4);
        assert!(notes.len() >= 7 * 3);
    }
}
