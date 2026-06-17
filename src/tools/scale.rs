pub const ROOT_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Chromatic,
    Major,
    NaturalMinor,
    HarmonicMinor,
    PentatonicMinor,
    PentatonicMajor,
    Blues,
    Dorian,
    Phrygian,
}

impl Scale {
    pub fn intervals(&self) -> &'static [i8] {
        match self {
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::PentatonicMinor => &[0, 3, 5, 7, 10],
            Scale::PentatonicMajor => &[0, 2, 4, 7, 9],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Scale::Chromatic => "Chromatic",
            Scale::Major => "Major",
            Scale::NaturalMinor => "Natural Minor",
            Scale::HarmonicMinor => "Harmonic Minor",
            Scale::PentatonicMinor => "Pentatonic Minor",
            Scale::PentatonicMajor => "Pentatonic Major",
            Scale::Blues => "Blues",
            Scale::Dorian => "Dorian",
            Scale::Phrygian => "Phrygian",
        }
    }

    pub fn all() -> &'static [Scale] {
        &[
            Scale::Major,
            Scale::NaturalMinor,
            Scale::HarmonicMinor,
            Scale::PentatonicMinor,
            Scale::PentatonicMajor,
            Scale::Blues,
            Scale::Dorian,
            Scale::Phrygian,
            Scale::Chromatic,
        ]
    }
}

pub fn scale_note(root: u8, scale: &Scale, degree: i32, octave: u8) -> u8 {
    let intervals = scale.intervals();
    let len = intervals.len() as i32;
    let oct = octave as i32 + degree.div_euclid(len);
    let idx = degree.rem_euclid(len) as usize;
    (root as i32 + oct.saturating_mul(12) + intervals[idx] as i32).clamp(0, 119) as u8
}

pub fn euclidean(steps: usize, pulses: usize, rotation: usize) -> Vec<bool> {
    if pulses == 0 || steps == 0 {
        return vec![false; steps.max(1)];
    }
    let pulses = pulses.min(steps);
    let mut pattern = vec![false; steps];
    let mut bucket = steps - pulses;
    let mut count = 0usize;
    for i in 0..steps {
        bucket += pulses;
        if bucket >= steps {
            bucket -= steps;
            pattern[i] = true;
            count += 1;
            if count >= pulses {
                break;
            }
        }
    }
    if rotation > 0 {
        let rot = rotation % steps;
        pattern.rotate_left(rot);
    }
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_note_c_major() {
        assert_eq!(scale_note(48, &Scale::Major, 0, 0), 48);
        assert_eq!(scale_note(48, &Scale::Major, 1, 0), 50);
        assert_eq!(scale_note(48, &Scale::Major, 2, 0), 52);
        assert_eq!(scale_note(48, &Scale::Major, 6, 0), 59);
        assert_eq!(scale_note(48, &Scale::Major, 7, 0), 60);
    }

    #[test]
    fn test_euclidean_four_on_floor() {
        let pat = euclidean(16, 4, 0);
        assert_eq!(pat.iter().filter(|&&x| x).count(), 4);
        assert!(pat[0]);
        assert!(pat[4]);
        assert!(pat[8]);
        assert!(pat[12]);
    }

    #[test]
    fn test_euclidean_empty() {
        assert_eq!(euclidean(8, 0, 0), vec![false; 8]);
    }
}
