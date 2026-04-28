use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Note {
    On(u8),
    Off,
    Cut,
    Fade,
    None,
}

impl Note {
    #[allow(dead_code)]
    pub fn from_tone_octave(tone: u8, octave: u8) -> Note {
        Note::On(octave * 12 + tone)
    }

    #[allow(dead_code)]
    pub fn tone(self) -> Option<u8> {
        match self {
            Note::On(key) => Some(key % 12),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn octave(self) -> Option<u8> {
        match self {
            Note::On(key) => Some(key / 12),
            _ => None,
        }
    }

    pub fn frequency(self) -> Option<f64> {
        match self {
            Note::On(key) => Some(440.0 * 2.0_f64.powf((key as f64 - 69.0) / 12.0)),
            _ => None,
        }
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Note::On(key) => {
                let tone = key % 12;
                let octave = key / 12;
                write!(f, "{}{}", TONE_NAMES[tone as usize], octave)
            }
            Note::Off => write!(f, "==="),
            Note::Cut => write!(f, "^^^"),
            Note::Fade => write!(f, "~~~"),
            Note::None => write!(f, "---"),
        }
    }
}

impl Default for Note {
    fn default() -> Self {
        Note::None
    }
}

pub const TONE_NAMES: [&str; 12] = [
    "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
];

pub const PERIOD_TABLE: [u16; 108] = [
    1712, 1616, 1524, 1440, 1358, 1280, 1208, 1140, 1076, 1016, 960, 906,
    856, 808, 762, 720, 678, 640, 604, 570, 538, 508, 480, 453,
    428, 404, 381, 360, 339, 320, 302, 285, 269, 254, 240, 226,
    214, 202, 190, 180, 170, 160, 151, 143, 135, 127, 120, 113,
    107, 101, 95, 90, 85, 80, 75, 71, 67, 63, 60, 56,
    53, 50, 47, 45, 42, 40, 37, 35, 33, 31, 30, 28,
    27, 25, 24, 22, 21, 20, 19, 18, 17, 16, 15, 14,
    13, 13, 12, 12, 11, 11, 10, 10, 9, 9, 8, 8,
    7, 7, 6, 6, 6, 6, 5, 5, 5, 5, 4, 4,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_from_tone_octave() {
        let note = Note::from_tone_octave(0, 4);
        assert_eq!(note, Note::On(48));
        assert_eq!(note.tone(), Some(0));
        assert_eq!(note.octave(), Some(4));
    }

    #[test]
    fn note_frequency_middle_a() {
        let a4 = Note::On(69);
        let freq = a4.frequency().unwrap();
        assert!((freq - 440.0).abs() < 0.01);
    }

    #[test]
    fn note_display() {
        assert_eq!(format!("{}", Note::On(48)), "C-4");
        assert_eq!(format!("{}", Note::On(49)), "C#4");
        assert_eq!(format!("{}", Note::None), "---");
        assert_eq!(format!("{}", Note::Off), "===");
        assert_eq!(format!("{}", Note::Cut), "^^^");
        assert_eq!(format!("{}", Note::Fade), "~~~");
    }
}
