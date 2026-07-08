//! Shared helpers for MCP mutation handlers: typed-param extraction macros
//! and the note-name parser.

use crate::sequencer::note::Note;

// ── Note name parser ──

/// Parse an IT/XM note name (`C-5`, `D#4`), a sentinel (`---`, `===`, `^^^`,
/// `~~~`), or a bare MIDI key number (`60`) into a [`Note`].
pub(super) fn parse_note(s: &str) -> Result<Note, String> {
    match s {
        "..." | "---" => return Ok(Note::None),
        "===" | "^^^" if s == "===" => return Ok(Note::Off),
        "^^^" if s == "^^^" => return Ok(Note::Cut),
        "~~~" => return Ok(Note::Fade),
        _ => {}
    }
    let tone_names = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];
    let s_upper = s.to_uppercase();
    if s_upper.len() < 3 {
        if let Ok(k) = s_upper.parse::<u8>() {
            if k <= 119 {
                return Ok(Note::On(k));
            }
        }
        return Err(format!("Invalid note: '{s}'"));
    }
    let tone_str = &s_upper[..2];
    let octave_str = &s_upper[2..];
    let tone = tone_names.iter().position(|&t| t == tone_str)
        .ok_or_else(|| format!("Unknown note name: '{s}'"))?;
    let octave = octave_str.parse::<u8>().map_err(|_| format!("Invalid octave in '{s}'"))?;
    let key = octave * 12 + tone as u8;
    if key > 119 {
        return Err(format!("Note '{s}' out of range (max G-9)"));
    }
    Ok(Note::On(key))
}

// ── Helper to get typed params ──
//
// Each macro is `pub(crate) use`-exported so the per-domain submodules can pull
// them in with a plain `use super::common::{get_str, ...};` and invoke them at
// the call site unchanged.

macro_rules! get_str {
    ($p:expr, $k:expr) => {
        $p.get($k).and_then(|v| v.as_str()).map(|s| s.to_string())
    };
}
macro_rules! get_i64 {
    ($p:expr, $k:expr) => {
        $p.get($k).and_then(|v| v.as_i64())
    };
}
macro_rules! get_f64 {
    ($p:expr, $k:expr) => {
        $p.get($k).and_then(|v| v.as_f64())
    };
}
macro_rules! get_bool {
    ($p:expr, $k:expr) => {
        $p.get($k).and_then(|v| v.as_bool())
    };
}

pub(crate) use get_bool;
pub(crate) use get_f64;
pub(crate) use get_i64;
pub(crate) use get_str;
