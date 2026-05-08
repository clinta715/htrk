use crate::sequencer::pattern::Cell;
use crate::sequencer::note::Note;

pub fn interpolate_u8(start: u8, end: u8, step: usize, total: usize) -> u8 {
    if total <= 1 {
        return start;
    }
    let t = step as f32 / (total - 1) as f32;
    let v = start as f32 * (1.0 - t) + end as f32 * t;
    v.round().clamp(0.0, 255.0) as u8
}

pub fn interpolate_i8(start: i8, end: i8, step: usize, total: usize) -> i8 {
    if total <= 1 {
        return start;
    }
    let t = step as f32 / (total - 1) as f32;
    let v = start as f32 * (1.0 - t) + end as f32 * t;
    v.round().clamp(-128.0, 127.0) as i8
}

pub fn interpolate_note(start: Note, end: Note, step: usize, total: usize) -> Note {
    match (start, end) {
        (Note::On(s), Note::On(e)) => {
            let v = interpolate_u8(s, e, step, total);
            Note::On(v.min(119))
        }
        _ => start,
    }
}

pub fn random_u8(min: u8, max: u8) -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = seed;
    state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let rand_val = ((state >> 33) as u32) as u8;
    let range = (max - min) as u16;
    if range == 0 {
        return min;
    }
    min + (rand_val as u16 % (range + 1)) as u8
}

pub fn fill_instrument_cells(
    cells: &mut [Cell],
    instrument: u8,
) -> Vec<Cell> {
    let old: Vec<Cell> = cells.to_vec();
    for cell in cells.iter_mut() {
        if cell.note != Note::None {
            cell.instrument = Some(instrument);
        }
    }
    old
}
