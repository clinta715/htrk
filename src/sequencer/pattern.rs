use crate::sequencer::effect::Effect;
use crate::sequencer::note::Note;

pub const MAX_CHANNELS: usize = 64;
#[allow(dead_code)]
pub const DEFAULT_ROWS: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub note: Note,
    pub instrument: Option<u8>,
    pub volume: Option<u8>,
    pub effect: Effect,
}

impl Cell {
    pub fn is_empty(&self) -> bool {
        self.note == Note::None
            && self.instrument.is_none()
            && self.volume.is_none()
            && self.effect == Effect::None
    }
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub num_rows: usize,
    pub data: Vec<[Cell; MAX_CHANNELS]>,
}

impl Pattern {
    pub fn new(num_rows: usize) -> Self {
        Pattern {
            num_rows,
            data: vec![[Cell::default(); MAX_CHANNELS]; num_rows],
        }
    }

    pub fn cell(&self, row: usize, channel: usize) -> &Cell {
        &self.data[row][channel]
    }

    #[allow(dead_code)]
    pub fn cell_mut(&mut self, row: usize, channel: usize) -> &mut Cell {
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
