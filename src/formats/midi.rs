//! Standard MIDI File (.mid / .midi) import.
//!
//! Converts an SMF into a set of tracker [`Pattern`]s ready to merge into the
//! current song. Each MIDI **track** becomes one tracker **channel** (track 0
//! → channel 0, …, capped at `MAX_CHANNELS`). Timing is quantized onto a fixed
//! `rows_per_beat` grid: `row = round(midi_tick / ticks_per_row)`.
//!
//! ### Limitations (v1)
//! - Only the first tempo meta-event is honored; later tempo changes are
//!   ignored (the patterns use a single `initial_speed`).
//! - Pitch bend, CC, aftertouch, and program changes are dropped (no
//!   meaningful tracker representation without per-instrument mapping).
//! - MIDI channel 10 (drums) lands on its track's assigned channel like any
//!   other; no GM drum-kit sample assignment.
//! - Sub-tick timing is quantized away (no Note-Delay effects).
//!
//! See `AGENTS.md` §26 for the full architecture.

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

use crate::errors::{FormatError, FormatResult};
use crate::sequencer::module::MAX_PATTERN_ROWS;
use crate::sequencer::note::Note;
use crate::sequencer::pattern::{Pattern, MAX_CHANNELS};

/// Default number of rows per pattern chunk (matches `DEFAULT_ROWS`).
const ROWS_PER_PATTERN: usize = 64;

/// Result of importing a MIDI file: ready to merge into a `Module`.
pub struct MidiImport {
    /// One `Pattern` per 64-row (configurable) chunk of the song, across all
    /// tracks. Patterns are blank where no notes fall.
    pub patterns: Vec<Pattern>,
    /// Order list referencing the patterns sequentially: `[base, base+1, …]`.
    /// Stored as the *offsets* from the first new pattern (caller adds the
    /// real base index). Length == `patterns.len()`.
    pub order_offsets: Vec<u8>,
    /// BPM from the first tempo meta-event, or 0 if none found (caller keeps
    /// the current tempo when this is 0).
    pub bpm: u16,
    /// Track / channel names, indexed by channel. Empty string when unnamed.
    pub track_names: Vec<String>,
    /// Highest tracker channel index that received at least one note + 1.
    pub channels_used: usize,
    /// Rows per pattern that was used (for reporting).
    pub rows_per_pattern: usize,
    /// How many MIDI tracks were skipped because they'd exceed `MAX_CHANNELS`.
    pub tracks_skipped: usize,
}

/// Import a Standard MIDI File.
///
/// `rows_per_beat` controls the quantization grid (e.g. 4 = 16th notes, 8 =
/// 32nd notes). It is clamped to `1..=64`.
pub fn import_midi(data: &[u8], rows_per_beat: u32) -> FormatResult<MidiImport> {
    let smf = Smf::parse(data).map_err(|e| FormatError::ParseError(format!("MIDI parse: {e}")))?;

    // PPQ (pulses per quarter note) from the header. SMPTE timecode timing is
    // not supported — fall back to 480.
    let ppq = match smf.header.timing {
        Timing::Metrical(ppq) => ppq.as_int() as u32,
        Timing::Timecode(_, _) => {
            return Err(FormatError::ParseError(
                "SMPTE timecode MIDI timing is not supported (use metrical/PPQ)".into(),
            ))
        }
    };
    let rows_per_beat = rows_per_beat.clamp(1, 64);
    // ticks_per_row = ticks-per-beat / rows_per_beat = ppq / rows_per_beat
    // (ppq is per *quarter*; one beat = one quarter note in 4/4, which is the
    // assumption here). Guard against zero.
    let ticks_per_row = ((ppq + rows_per_beat / 2) / rows_per_beat).max(1);

    // ── Pass 1: walk every track, accumulate absolute-tick note events. ──
    // A note-on with velocity 0 is treated as a note-off (common SMF convention).
    #[derive(Clone, Copy)]
    struct NoteOn { row: usize, key: u8, vel: u8 }
    // channel -> key -> (row, vel) of the still-sounding note-on
    let mut sounding: Vec<std::collections::HashMap<u8, NoteOn>> = vec![];
    // (row, channel, key, vel) — final placed note-ons with their cut row
    struct PlacedNote { on_row: usize, off_row: usize, channel: usize, key: u8, vel: u8 }
    let mut placed: Vec<PlacedNote> = Vec::new();
    let mut track_names: Vec<String> = Vec::new();
    let mut bpm: u16 = 0;
    let mut max_row: usize = 0;
    let mut tracks_skipped: usize = 0;
    let mut channels_used: usize = 0;

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        if track_idx >= MAX_CHANNELS {
            tracks_skipped = smf.tracks.len() - track_idx;
            break;
        }
        let channel = track_idx;
        // Ensure sounding map is large enough for this channel.
        if sounding.len() <= channel {
            sounding.resize_with(channel + 1, std::collections::HashMap::new);
        }
        let mut abs_tick: u32 = 0;
        for event in track.iter() {
            abs_tick = abs_tick.saturating_add(event.delta.as_int());
            match event.kind {
                TrackEventKind::Midi { message, .. } => match message {
                    MidiMessage::NoteOn { key, vel } => {
                        let key = key.as_int();
                        let vel = vel.as_int();
                        let row = ((abs_tick as u64 + (ticks_per_row / 2) as u64)
                            / ticks_per_row as u64) as usize;
                        if vel == 0 {
                            // velocity-0 => note-off
                            if let Some(on) = sounding[channel].remove(&key) {
                                placed.push(PlacedNote {
                                    on_row: on.row,
                                    off_row: row,
                                    channel,
                                    key: on.key,
                                    vel: on.vel,
                                });
                                if row > max_row { max_row = row; }
                            }
                        } else {
                            // Close any prior sounding note of the same key first.
                            if let Some(on) = sounding[channel].remove(&key) {
                                placed.push(PlacedNote {
                                    on_row: on.row,
                                    off_row: row,
                                    channel,
                                    key: on.key,
                                    vel: on.vel,
                                });
                            }
                            sounding[channel].insert(key, NoteOn { row, key, vel });
                            if row > max_row { max_row = row; }
                        }
                    }
                    MidiMessage::NoteOff { key, .. } => {
                        let key = key.as_int();
                        if let Some(on) = sounding[channel].remove(&key) {
                            let row = ((abs_tick as u64 + (ticks_per_row / 2) as u64)
                                / ticks_per_row as u64) as usize;
                            placed.push(PlacedNote {
                                on_row: on.row,
                                off_row: row,
                                channel,
                                key: on.key,
                                vel: on.vel,
                            });
                            if row > max_row { max_row = row; }
                        }
                    }
                    // Other channel-voice messages (CC, pitch bend, program change,
                    // aftertouch) are intentionally ignored — see module docs.
                    _ => {}
                },
                TrackEventKind::Meta(meta) => match meta {
                    MetaMessage::TrackName(name) => {
                        let s = std::str::from_utf8(name).unwrap_or("").trim().to_string();
                        if !s.is_empty() {
                            while track_names.len() <= channel { track_names.push(String::new()); }
                            track_names[channel] = s;
                        }
                    }
                    MetaMessage::Tempo(tempo) => {
                        if bpm == 0 {
                            // tempo = microseconds per quarter note
                            let mpqn = tempo.as_int() as u64;
                            if mpqn > 0 {
                                bpm = (60_000_000u64 / mpqn).min(u16::MAX as u64) as u16;
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        // Close any notes still sounding at end-of-track.
        let end_row = ((abs_tick as u64 + (ticks_per_row / 2) as u64)
            / ticks_per_row as u64) as usize;
        if end_row > max_row { max_row = end_row; }
        if !sounding[channel].is_empty() {
            for (_, on) in sounding[channel].drain() {
                placed.push(PlacedNote {
                    on_row: on.row,
                    off_row: end_row,
                    channel,
                    key: on.key,
                    vel: on.vel,
                });
            }
        }
        if !sounding[channel].is_empty() || placed.iter().any(|p| p.channel == channel) {
            channels_used = channels_used.max(channel + 1);
        }
    }

    // ── Pass 2: slice into fixed-size patterns and place notes. ──
    if max_row >= MAX_PATTERN_ROWS * 256 {
        return Err(FormatError::ParseError(format!(
            "MIDI file too long: {max_row} rows exceeds the 256-pattern limit"
        )));
    }
    let total_rows = max_row + 1;
    let num_patterns = ((total_rows + ROWS_PER_PATTERN - 1) / ROWS_PER_PATTERN).max(1);
    let mut patterns: Vec<Pattern> = (0..num_patterns)
        .map(|_| Pattern::new(ROWS_PER_PATTERN))
        .collect();

    // Place note-offs first, then note-ons — so a note-on landing on a row
    // that also carries a note-off (retrigger) overwrites the off.
    for n in &placed {
        if n.off_row <= n.on_row { continue; }
        let off_pat = n.off_row / ROWS_PER_PATTERN;
        let off_row_in_pat = n.off_row % ROWS_PER_PATTERN;
        if off_pat < patterns.len() {
            let off_cell = patterns[off_pat].cell_mut(off_row_in_pat, n.channel);
            if let Note::None = off_cell.note {
                off_cell.note = Note::Off;
            }
        }
    }
    for n in &placed {
        let pat = n.on_row / ROWS_PER_PATTERN;
        let row_in_pat = n.on_row % ROWS_PER_PATTERN;
        if pat >= patterns.len() { continue; }
        let cell = patterns[pat].cell_mut(row_in_pat, n.channel);
        // Note-on wins over any prior contents (incl. a note-off from the loop
        // above), but a note already placed by an earlier (row,channel) entry
        // keeps priority to avoid clobbering a deliberate first note.
        if let Note::None | Note::Off = cell.note {
            cell.note = Note::On(n.key);
            cell.instrument = Some((n.channel as u8).saturating_add(1));
            // Velocity 1..127 → volume 0..64.
            cell.volume = Some(((n.vel as u32 * 64 + 63) / 127).min(64) as u8);
        }
    }

    let order_offsets: Vec<u8> = (0..num_patterns)
        .map(|i| i as u8)
        .take(u8::MAX as usize)
        .collect();

    Ok(MidiImport {
        patterns,
        order_offsets,
        bpm,
        track_names,
        channels_used: channels_used.max(1),
        rows_per_pattern: ROWS_PER_PATTERN,
        tracks_skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequencer::note::Note;

    /// Build a minimal valid SMF byte stream with one track and the given
    /// (delta, status, key, vel) tuples.
    fn build_smf(tracks: &[&[(u32, u8, u8, u8)]], ppq: u16) -> Vec<u8> {
        let mut out = Vec::new();
        // MThd chunk: "MThd" + length(6) + format(2) + ntracks(2) + division(2)
        let format: u16 = if tracks.len() <= 1 { 0 } else { 1 };
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&6u32.to_be_bytes());
        out.extend_from_slice(&format.to_be_bytes());
        out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
        out.extend_from_slice(&ppq.to_be_bytes());

        for track in tracks {
            let mut body = Vec::new();
            for &(delta, status, key, vel) in *track {
                write_varlen(&mut body, delta);
                body.push(status);
                body.push(key);
                body.push(vel);
            }
            // End of track meta event: FF 2F 00, delta 0
            write_varlen(&mut body, 0);
            body.push(0xFF);
            body.push(0x2F);
            body.push(0x00);

            out.extend_from_slice(b"MTrk");
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(&body);
        }
        out
    }

    fn write_varlen(out: &mut Vec<u8>, mut value: u32) {
        let mut buffer = [0u8; 5];
        let mut idx = 4;
        buffer[idx] = (value & 0x7F) as u8;
        value >>= 7;
        while value > 0 {
            idx -= 1;
            buffer[idx] = (((value & 0x7F) as u8)) | 0x80;
            value >>= 7;
        }
        out.extend_from_slice(&buffer[idx..]);
    }

    #[test]
    fn test_single_note_on_off_one_row() {
        // ppq=4, rows_per_beat=4 => ticks_per_row=1. Note on at tick 0,
        // off at tick 10 => on row 0, off row 10. Channel 0 (note-on 0x90).
        let track: &[(u32, u8, u8, u8)] = &[
            (0, 0x90, 60, 100), // C4 on, vel 100
            (10, 0x80, 60, 0),  // C4 off at tick 10
        ];
        let data = build_smf(&[track], 4);
        let imp = import_midi(&data, 4).unwrap();
        assert_eq!(imp.patterns.len(), 1);
        assert_eq!(imp.channels_used, 1);
        let cell = imp.patterns[0].cell(0, 0);
        assert_eq!(cell.note, Note::On(60));
        // vel 100 -> (100*64+63)/127 = 50
        assert_eq!(cell.volume, Some(50));
        assert_eq!(cell.instrument, Some(1));
        // off at row 10
        assert_eq!(imp.patterns[0].cell(10, 0).note, Note::Off);
    }

    #[test]
    fn test_velocity_zero_is_note_off() {
        let track: &[(u32, u8, u8, u8)] = &[
            (0, 0x90, 64, 80),
            (5, 0x90, 64, 0), // velocity 0 => note off at tick 5
        ];
        let data = build_smf(&[track], 4);
        let imp = import_midi(&data, 4).unwrap();
        assert_eq!(imp.patterns[0].cell(0, 0).note, Note::On(64));
        assert_eq!(imp.patterns[0].cell(5, 0).note, Note::Off);
    }

    #[test]
    fn test_two_tracks_two_channels() {
        // Track 0: note on ch 0. Track 1: note on ch 1.
        let t0: &[(u32, u8, u8, u8)] = &[(0, 0x90, 60, 100), (4, 0x80, 60, 0)];
        let t1: &[(u32, u8, u8, u8)] = &[(0, 0x90, 64, 110), (4, 0x80, 64, 0)];
        let data = build_smf(&[t0, t1], 4);
        let imp = import_midi(&data, 4).unwrap();
        assert_eq!(imp.channels_used, 2);
        assert_eq!(imp.patterns[0].cell(0, 0).note, Note::On(60));
        assert_eq!(imp.patterns[0].cell(0, 1).note, Note::On(64));
        // vel 110 -> (110*64+63)/127 = 55
        assert_eq!(imp.patterns[0].cell(0, 1).volume, Some(55));
        assert_eq!(imp.patterns[0].cell(0, 1).instrument, Some(2)); // track 1 -> inst 2
    }

    #[test]
    fn test_pattern_split_at_64_rows() {
        // A note at row 70 should land in pattern 1, row 6.
        let track: &[(u32, u8, u8, u8)] = &[
            (70, 0x90, 72, 100),
            (74, 0x80, 72, 0),
        ];
        let data = build_smf(&[track], 1); // ppq=1, rpb=4 => ticks_per_row=1 (ppq/rpb rounded: (1+2)/4=0 -> max(1)=1)
        let imp = import_midi(&data, 4).unwrap();
        assert!(imp.patterns.len() >= 2, "expected split, got {}", imp.patterns.len());
        assert_eq!(imp.patterns[1].cell(6, 0).note, Note::On(72));
    }

    #[test]
    fn test_retrigger_overwrites_note_off() {
        // If a new note-on lands on the same absolute row as a note-off
        // (here both at abs tick 5: off after delta 5, then on with delta 0),
        // the note-on wins and the off is not placed.
        let track: &[(u32, u8, u8, u8)] = &[
            (0, 0x90, 60, 100),  // on  at abs 0
            (5, 0x80, 60, 0),    // off at abs 5
            (0, 0x90, 62, 100),  // on  at abs 5 (delta 0)
            (9, 0x80, 62, 0),    // off at abs 14
        ];
        let data = build_smf(&[track], 4);
        let imp = import_midi(&data, 4).unwrap();
        // Row 5 channel 0 should be the new Note::On(62), not Note::Off.
        assert_eq!(imp.patterns[0].cell(5, 0).note, Note::On(62));
        assert_eq!(imp.patterns[0].cell(14, 0).note, Note::Off);
    }

    #[test]
    fn test_smpte_timing_rejected() {
        // Timecode timing: format 0, but timing = (25, 40) -> SMPTE. midly
        // encodes this; easiest is to craft raw bytes. Build a header with
        // SMPTE: a negative-ish first division byte (0xE7 etc).
        let mut out = Vec::new();
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&6u32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes()); // format 0
        out.extend_from_slice(&1u16.to_be_bytes()); // 1 track
        // SMPTE: -24 fps => 0xE8, 40 subframes
        out.push(0xE8);
        out.push(40u8);
        // empty track
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&0u32.to_be_bytes());
        let res = import_midi(&out, 4);
        assert!(res.is_err(), "expected SMPTE to be rejected");
    }
}
