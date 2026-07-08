//! Pattern & cell mutation handlers: ensure / set / set_batch / fill / clear /
//! transpose / interpolate, plus the cell-builder and effect-hex parsers shared
//! between them.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::edit::{BulkSetCellsCommand, InterpolateCommand, SetCellCommand, TransposeCommand};
use crate::mcp::protocol::CmdResult;
use crate::sequencer::note::Note;
use crate::sequencer::pattern::{Cell, MAX_CHANNELS};

use super::common::{get_i64, get_str, parse_note};

pub(super) fn cmd_pattern_ensure(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let num_rows = get_i64!(params, "num_rows").unwrap_or(64) as usize;
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let rows = num_rows.max(1).min(crate::sequencer::module::MAX_PATTERN_ROWS);
            if idx >= arc_module.patterns.len() {
                arc_module.patterns.resize_with(idx + 1, || crate::sequencer::pattern::Pattern::new(rows));
            } else if arc_module.patterns[idx].num_rows != rows {
                arc_module.patterns[idx].num_rows = rows;
                arc_module.patterns[idx].data.resize(rows, [Cell::default(); MAX_CHANNELS]);
            }
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "index": idx, "num_rows": rows}));
        }
    }
    Err("No module loaded".into())
}

/// Build a [`Cell`] from JSON params, starting from `defaults` and overriding
/// any field present in `params`.
pub(super) fn build_cell_from_params(params: &serde_json::Value, defaults: &Cell) -> Cell {
    let mut cell = *defaults;
    if let Some(note_str) = get_str!(params, "note") {
        if let Ok(note) = parse_note(&note_str) {
            cell.note = note;
        }
    }
    if let Some(inst) = get_i64!(params, "instrument") {
        cell.instrument = if inst < 0 { None } else { Some(inst as u8) };
    }
    if let Some(vol) = get_i64!(params, "volume") {
        cell.volume = Some((vol as u8).min(64));
    }
    if let Some(ve) = params.get("volume_effect") {
        if ve.is_null() {
            cell.volume_effect = None;
        } else if let Some(obj) = ve.as_object() {
            cell.volume_effect = parse_effect_json(obj);
        }
    }
    if let Some(ef) = params.get("effect") {
        if ef.is_null() {
            cell.effect = crate::sequencer::effect::Effect::None;
        } else if let Some(obj) = ef.as_object() {
            cell.effect = parse_effect_json(obj).unwrap_or(crate::sequencer::effect::Effect::None);
        }
    }
    cell
}

fn parse_effect_json(obj: &serde_json::Map<String, serde_json::Value>) -> Option<crate::sequencer::effect::Effect> {
    if let Some(hex) = obj.get("hex").and_then(|v| v.as_str()) {
        return parse_hex_effect(hex);
    }
    None
}

fn parse_hex_effect(hex: &str) -> Option<crate::sequencer::effect::Effect> {
    if hex.len() < 3 { return None; }
    let chars: Vec<char> = hex.chars().collect();
    let effect_char = chars[0];
    let param_hi = chars.get(1).and_then(|c| c.to_digit(16)).unwrap_or(0) as u8;
    let param_lo = chars.get(2).and_then(|c| c.to_digit(16)).unwrap_or(0) as u8;

    use crate::sequencer::effect::Effect;
    Some(match effect_char {
        '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
            // Volume column hex — treat as volume_effect
            return None;
        }
        'A' => Effect::Arpeggio { note1: param_hi, note2: param_lo },
        'B' => Effect::PositionJump { order: param_hi as u16 * 16 + param_lo as u16 },
        'C' => Effect::SetVolume { volume: (param_hi << 4) | param_lo },
        'D' => Effect::PatternBreak { row: param_hi as u16 * 16 + param_lo as u16 },
        'E' => Effect::ExtendedEffect { param: (param_hi << 4) | param_lo },
        'F' => {
            let param = (param_hi << 4) | param_lo;
            if param < 32 {
                Effect::SetSpeed { speed: param }
            } else {
                Effect::SetTempo { bpm: param }
            }
        }
        'G' => Effect::GlobalVolumeSlide { up: param_hi as i8, down: param_lo as i8 },
        'H' => Effect::Vibrato { speed: param_hi, depth: param_lo },
        'I' => Effect::Tremor { ontime: param_hi, offtime: param_lo },
        'J' => Effect::TonePortamento { speed: (param_hi << 4) | param_lo },
        'K' => Effect::TonePortamentoVolumeSlide { up: param_hi as i8 },
        'L' => Effect::VibratoVolumeSlide { up: param_hi as i8 },
        'M' => Effect::SetPanning { pan: (param_hi << 4) | param_lo },
        'N' => Effect::SetSampleOffset { offset: param_hi as u16 * 16 + param_lo as u16 },
        'O' => Effect::SetEnvelopePosition { tick: param_hi as u16 * 16 + param_lo as u16 },
        'P' => Effect::SetPanning { pan: (param_hi << 4) | param_lo },
        'Q' => Effect::ExtraFinePortamentoUp { speed: param_lo },
        'R' => Effect::Retrigger { interval: (param_hi << 4) | param_lo },
        'S' => Effect::SetFilterCutoff { cutoff: ((param_hi << 4) | param_lo) as u16 * 8 },
        'T' => Effect::SetTempo { bpm: (param_hi << 4) | param_lo },
        'U' => Effect::FineVolumeSlideUp { amount: param_lo },
        'V' => Effect::SetGlobalVolume { volume: (param_hi << 4) | param_lo },
        'W' => Effect::GlobalVolumeSlide { up: param_hi as i8, down: param_lo as i8 },
        'X' => Effect::ExtraFinePortamentoDown { speed: param_lo },
        'Y' => Effect::Panbrello { speed: param_hi, depth: param_lo },
        'Z' => Effect::SetFilterResonance { resonance: (param_hi << 4) | param_lo },
        _ => return None,
    })
}

pub(super) fn cmd_cell_set(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let order = get_i64!(params, "order").ok_or("Missing 'order'")? as usize;
    let row = get_i64!(params, "row").ok_or("Missing 'row'")? as usize;
    let channel = get_i64!(params, "channel").ok_or("Missing 'channel'")? as usize;
    let new_cell = build_cell_from_params(params, &Cell::default());

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let pat_idx = *arc_module.order_list.get(order).ok_or("Order index out of range")? as usize;
            if pat_idx >= arc_module.patterns.len() {
                return Err(format!("Pattern {pat_idx} does not exist"));
            }
            if row >= arc_module.patterns[pat_idx].num_rows {
                return Err(format!("Row {row} out of range (max {})", arc_module.patterns[pat_idx].num_rows));
            }
            if channel >= MAX_CHANNELS {
                return Err(format!("Channel {channel} out of range (max {})", MAX_CHANNELS - 1));
            }
            let old_cell = arc_module.patterns[pat_idx].data[row][channel];
            let cmd = Box::new(SetCellCommand { order, row, channel, old_cell, new_cell });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_cell_set_batch(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let entries = params.get("entries").and_then(|v| v.as_array())
        .ok_or("Missing 'entries'")?;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let mut old_cells = Vec::new();
            let mut new_cells = Vec::new();

            for entry in entries {
                let order = entry.get("order").and_then(|v| v.as_i64()).ok_or("Each entry needs 'order'")? as usize;
                let row = entry.get("row").and_then(|v| v.as_i64()).ok_or("Each entry needs 'row'")? as usize;
                let channel = entry.get("channel").and_then(|v| v.as_i64()).ok_or("Each entry needs 'channel'")? as usize;

                let pat_idx = *arc_module.order_list.get(order).ok_or("Order index out of range")? as usize;
                if pat_idx >= arc_module.patterns.len() {
                    return Err(format!("Pattern {pat_idx} does not exist"));
                }
                if row >= arc_module.patterns[pat_idx].num_rows {
                    return Err(format!("Row {row} out of range"));
                }
                if channel >= MAX_CHANNELS {
                    return Err(format!("Channel {channel} out of range"));
                }

                let new_cell = build_cell_from_params(entry, &Cell::default());
                let old_cell = arc_module.patterns[pat_idx].data[row][channel];
                old_cells.push((row, channel, old_cell));
                new_cells.push((row, channel, new_cell));
            }

            let cmd = Box::new(BulkSetCellsCommand { order: 0, old_cells, new_cells });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "count": entries.len()}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_pattern_fill(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let row_start = get_i64!(params, "row_start").ok_or("Missing 'row_start'")? as usize;
    let row_end = get_i64!(params, "row_end").ok_or("Missing 'row_end'")? as usize;
    let ch_start = get_i64!(params, "channel_start").unwrap_or(0) as usize;
    let ch_end = get_i64!(params, "channel_end")
        .map(|v| v as usize)
        .unwrap_or(ch_start);

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.patterns.len() {
                return Err(format!("Pattern {idx} does not exist"));
            }
            let num_rows = arc_module.patterns[idx].num_rows;
            let max_ch = arc_module.channel_panning.len().min(MAX_CHANNELS);
            if row_start >= num_rows || row_end >= num_rows || row_start > row_end {
                return Err("Invalid row range".into());
            }
            if ch_start >= max_ch || ch_end >= max_ch || ch_start > ch_end {
                return Err("Invalid channel range".into());
            }

            let mut old_cells = Vec::new();
            let mut new_cells = Vec::new();
            for r in row_start..=row_end {
                for c in ch_start..=ch_end {
                    old_cells.push((r, c, arc_module.patterns[idx].data[r][c]));
                    let new_cell = build_cell_from_params(params, &arc_module.patterns[idx].data[r][c]);
                    new_cells.push((r, c, new_cell));
                }
            }

            let cmd = Box::new(BulkSetCellsCommand { order: 0, old_cells, new_cells });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "cells_affected": (row_end - row_start + 1) * (ch_end - ch_start + 1)}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_pattern_clear(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let row_start = get_i64!(params, "row_start").map(|v| v as usize);
    let row_end = get_i64!(params, "row_end").map(|v| v as usize);
    let ch_start = get_i64!(params, "channel_start").map(|v| v as usize);
    let ch_end = get_i64!(params, "channel_end").map(|v| v as usize);

    let empty_cell = Cell::default();
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.patterns.len() {
                return Err(format!("Pattern {idx} does not exist"));
            }
            let num_rows = arc_module.patterns[idx].num_rows;
            let max_ch = arc_module.channel_panning.len().min(MAX_CHANNELS);

            let (rs, re, cs, ce) = match (row_start, row_end, ch_start, ch_end) {
                (Some(rs), Some(re), Some(cs), Some(ce)) => (rs, re, cs, ce),
                _ => (0, num_rows - 1, 0, max_ch - 1),
            };

            let mut old_cells = Vec::new();
            let mut new_cells = Vec::new();
            for r in rs..=re.min(num_rows - 1) {
                for c in cs..=ce.min(max_ch - 1) {
                    old_cells.push((r, c, arc_module.patterns[idx].data[r][c]));
                    new_cells.push((r, c, empty_cell));
                }
            }

            let cmd = Box::new(BulkSetCellsCommand { order: 0, old_cells, new_cells });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_pattern_transpose(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let semitones = get_i64!(params, "semitones").ok_or("Missing 'semitones'")? as i8;
    let channel = get_i64!(params, "channel").map(|v| v as usize);
    let row_start = get_i64!(params, "row_start").map(|v| v as usize);
    let row_end = get_i64!(params, "row_end").map(|v| v as usize);

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.patterns.len() {
                return Err(format!("Pattern {idx} does not exist"));
            }
            let num_rows = arc_module.patterns[idx].num_rows;
            let max_ch = arc_module.channel_panning.len().min(MAX_CHANNELS);
            let rs = row_start.unwrap_or(0);
            let re = row_end.unwrap_or(num_rows - 1).min(num_rows - 1);
            let chs = channel.unwrap_or(0);
            let che = channel.unwrap_or(max_ch - 1).min(max_ch - 1);

            let mut old_notes = Vec::new();
            for r in rs..=re {
                for c in chs..=che {
                    if c < max_ch {
                        let cell = &arc_module.patterns[idx].data[r][c];
                        if let Note::On(_) = cell.note {
                            old_notes.push((r, c, cell.note));
                        }
                    }
                }
            }
            let notes_affected = old_notes.len();

            let cmd = Box::new(TransposeCommand { order: 0, delta: semitones, old_notes });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "notes_affected": notes_affected}));
        }
    }
    Err("No module loaded".into())
}

pub(super) fn cmd_pattern_interpolate(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let channel = get_i64!(params, "channel").ok_or("Missing 'channel'")? as usize;
    let row_start = get_i64!(params, "row_start").ok_or("Missing 'row_start'")? as usize;
    let row_end = get_i64!(params, "row_end").ok_or("Missing 'row_end'")? as usize;
    let target = get_str!(params, "target").ok_or("Missing 'target'")?;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.patterns.len() {
                return Err(format!("Pattern {idx} does not exist"));
            }
            if channel >= MAX_CHANNELS || channel >= arc_module.channel_panning.len() {
                return Err("Channel out of range".into());
            }
            let num_rows = arc_module.patterns[idx].num_rows;
            if row_start >= num_rows || row_end >= num_rows || row_start > row_end {
                return Err("Invalid row range".into());
            }
            if row_start == row_end {
                return Err("Row range must span at least 2 rows".into());
            }

            let get_val = |cell: &Cell| -> Option<u8> {
                match target.as_str() {
                    "volume" => cell.volume,
                    _ => None,
                }
            };

            let start_val = get_val(&arc_module.patterns[idx].data[row_start][channel]);
            let end_val = get_val(&arc_module.patterns[idx].data[row_end][channel]);

            match (start_val, end_val) {
                (Some(sv), Some(ev)) => {
                    let mut old_cells = Vec::new();
                    let mut new_cells = Vec::new();
                    let len = (row_end - row_start) as f32;
                    for r in row_start..=row_end {
                        let t = (r - row_start) as f32 / len;
                        let val = (sv as f32 * (1.0 - t) + ev as f32 * t).round() as u8;
                        let mut new_cell = arc_module.patterns[idx].data[r][channel];
                        new_cell.volume = Some(val.min(64));
                        old_cells.push((r, channel, arc_module.patterns[idx].data[r][channel]));
                        new_cells.push((r, channel, new_cell));
                    }
                    let cmd = Box::new(InterpolateCommand { order: 0, old_cells, new_cells });
                    let _ = core.undo_manager.execute(cmd, arc_module);
                    core.sync_module_to_audio();
                    Ok(serde_json::json!({"ok": true}))
                }
                _ => Err("Start or end row has no volume value".into())
            }
        } else {
            Err("Failed to get exclusive module access".into())
        }
    } else {
        Err("No module loaded".into())
    }
}
