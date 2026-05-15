use crate::sequencer::effect::Effect;
use crate::sequencer::note::Note;
use serde::de::Deserializer;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};

pub const MAX_CHANNELS: usize = 64;
#[allow(dead_code)]
pub const DEFAULT_ROWS: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cell {
    pub note: Note,
    pub instrument: Option<u8>,
    pub volume: Option<u8>,
    pub volume_effect: Option<Effect>,
    pub effect: Effect,
}

impl Cell {
    pub fn is_empty(&self) -> bool {
        self.note == Note::None
            && self.instrument.is_none()
            && self.volume.is_none()
            && self.volume_effect.is_none()
            && self.effect == Effect::None
    }
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub num_rows: usize,
    pub data: Vec<[Cell; MAX_CHANNELS]>,
}

impl Serialize for Pattern {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("Pattern", 2)?;
        s.serialize_field("num_rows", &self.num_rows)?;
        let rows: Vec<Vec<Cell>> = self.data.iter().map(|row| row.to_vec()).collect();
        s.serialize_field("data", &rows)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct PatternData {
            num_rows: usize,
            data: Vec<Vec<Cell>>,
        }
        let pd = PatternData::deserialize(deserializer)?;
        let data = pd.data.into_iter().map(|row| {
            let mut arr = [Cell::default(); MAX_CHANNELS];
            for (i, cell) in row.into_iter().enumerate().take(MAX_CHANNELS) {
                arr[i] = cell;
            }
            arr
        }).collect();
        Ok(Pattern { num_rows: pd.num_rows, data })
    }
}

impl Pattern {
    pub fn new(num_rows: usize) -> Self {
        Pattern {
            num_rows,
            data: vec![[Cell::default(); MAX_CHANNELS]; num_rows],
        }
    }

    pub fn cell(&self, row: usize, channel: usize) -> &Cell {
        debug_assert!(row < self.num_rows, "cell: row {} out of bounds (max {})", row, self.num_rows);
        debug_assert!(channel < MAX_CHANNELS, "cell: channel {} out of bounds", channel);
        &self.data[row][channel]
    }

    #[allow(dead_code)]
    pub fn cell_mut(&mut self, row: usize, channel: usize) -> &mut Cell {
        debug_assert!(row < self.num_rows, "cell_mut: row {} out of bounds (max {})", row, self.num_rows);
        debug_assert!(channel < MAX_CHANNELS, "cell_mut: channel {} out of bounds", channel);
        &mut self.data[row][channel]
    }

    #[allow(dead_code)]
    pub fn resize_rows(&mut self, new_rows: usize) {
        self.data.resize(new_rows, [Cell::default(); MAX_CHANNELS]);
        self.num_rows = new_rows;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cell_is_empty() {
        assert!(Cell::default().is_empty());
    }

    #[test]
    fn pattern_new_has_correct_rows() {
        let p = Pattern::new(64);
        assert_eq!(p.num_rows, 64);
        assert_eq!(p.data.len(), 64);
        assert!(p.cell(0, 0).is_empty());
    }

    #[test]
    fn pattern_cell_mut() {
        let mut p = Pattern::new(64);
        p.cell_mut(0, 0).note = Note::On(60);
        assert_eq!(p.cell(0, 0).note, Note::On(60));
    }

    #[test]
    fn pattern_resize() {
        let mut p = Pattern::new(64);
        p.resize_rows(128);
        assert_eq!(p.num_rows, 128);
        assert_eq!(p.data.len(), 128);
    }
}
