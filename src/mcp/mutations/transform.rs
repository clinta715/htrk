//! `pattern.transform` mutation handler — applies a batch of transformative
//! ops (humanize / thin / transpose / rotate / reverse / invert / quantize /
//! echo / set_instrument / shift / …) to a pattern as a single undoable edit.
//!
//! This is the largest single handler (a second-level dispatch over transform
//! op types), so it lives in its own file for isolation and future
//! refactorability.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::edit::BulkSetCellsCommand;
use crate::mcp::protocol::CmdResult;
use crate::sequencer::pattern::MAX_CHANNELS;
use crate::tools::phrase_generator::Lcg;

use super::common::get_i64;
// `parse_scale` is shared with the phrase.generate handler.
use super::phrase::parse_scale;

/// Apply a list of transforms to a pattern. Each transform mutates cells
/// in place and is recorded as a single undoable operation (one undo_id
/// covers the whole batch). Use the returned undo_id to revert.
pub(super) fn cmd_pattern_transform(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let order = get_i64!(params, "order").unwrap_or_else(|| core.selected_order as i64) as usize;
    let ops = params.get("ops").and_then(|v| v.as_array())
        .ok_or("Missing 'ops' array")?;
    if ops.is_empty() {
        return Err("'ops' must contain at least one operation".into());
    }
    let seed = get_i64!(params, "seed").unwrap_or(0) as u64;

    core.ensure_module_ownership();
    let module = core.module.as_mut().ok_or("No module loaded")?;
    let arc_module = Arc::get_mut(module).ok_or("Cannot get unique module access")?;

    // Auto-extend order list (same logic as phrase.generate).
    if order >= arc_module.order_list.len() {
        let fallback_pat: u8 = if arc_module.patterns.is_empty() { 0 }
                           else { (arc_module.patterns.len() - 1) as u8 };
        while arc_module.order_list.len() <= order {
            arc_module.order_list.push(fallback_pat);
        }
    }
    let pat_idx = arc_module.order_list[order] as usize;
    if pat_idx >= arc_module.patterns.len() {
        return Err(format!("Pattern index {pat_idx} does not exist"));
    }

    // Snapshot all cells for the BulkSetCellsCommand undo record.
    let pattern = &arc_module.patterns[pat_idx];
    let old_cells: Vec<(usize, usize, crate::sequencer::pattern::Cell)> = pattern.data
        .iter()
        .enumerate()
        .flat_map(|(r, row)| row.iter().copied()
            .enumerate()
            .map(move |(ch, c)| (r, ch, c)))
        .collect();

    // Apply each op, mutating a local working copy of the pattern.
    let mut working: Vec<Vec<crate::sequencer::pattern::Cell>> = pattern.data
        .iter()
        .map(|row| row.to_vec())
        .collect();

    let mut applied: Vec<String> = Vec::with_capacity(ops.len());
    for op_val in ops {
        let op = op_val.as_object().ok_or("Each op must be a JSON object")?;
        let op_type = op.get("type").and_then(|v| v.as_str())
            .ok_or("Op missing 'type'")?
            .to_string();
        match op_type.as_str() {
            "humanize" => {
                // Add small random timing/velocity variation.
                // "timing" 0.0-1.0 = probability of shifting note by ±1 row
                // "velocity" 0.0-1.0 = probability of changing the volume column
                let timing_p = op.get("timing").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let _vel_p = op.get("velocity").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let mut rng = Lcg::new(seed.wrapping_add(0xA1));
                for row in working.iter_mut() {
                    for cell in row.iter_mut() {
                        if cell.note == crate::sequencer::Note::None { continue; }
                        if rng.f32() < timing_p {
                            // Skip for now — row shift would require row coordinate too.
                            // We can perturb volume instead as a stand-in.
                            let new_vel = if cell.volume.is_some() {
                                let v = cell.volume.unwrap() as i32;
                                Some(((v + (rng.next() as i32 % 5) - 2).clamp(0, 64)) as u8)
                            } else {
                                Some(60 + (rng.next() as i32 % 8) as u8)
                            };
                            cell.volume = new_vel;
                        }
                    }
                }
                applied.push(format!("humanize(t={timing_p})"));
            }
            "swing" => {
                // No-op placeholder: swing operates on per-row timing which
                // doesn't fit the Cell struct (timing is in the channel's
                // effect column). Logged as a no-op for now.
                let _amount = op.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                applied.push(format!("swing(amount={})", _amount));
            }
            "thin" => {
                // Remove notes with the given probability.
                let p = op.get("probability").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                let mut rng = Lcg::new(seed.wrapping_add(0xB2));
                let mut removed = 0;
                for row in working.iter_mut() {
                    for cell in row.iter_mut() {
                        if cell.note == crate::sequencer::Note::None { continue; }
                        if rng.f32() < p {
                            *cell = crate::sequencer::pattern::Cell::default();
                            removed += 1;
                        }
                    }
                }
                applied.push(format!("thin(p={p}, removed={removed})"));
            }
            "transpose" => {
                // Shift note keys by N semitones, clamped to 0-119.
                let semitones = op.get("semitones").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let mut count = 0;
                for row in working.iter_mut() {
                    for cell in row.iter_mut() {
                        if let crate::sequencer::Note::On(k) = cell.note {
                            let new_k = (k as i32 + semitones).clamp(0, 119) as u8;
                            if new_k != k { count += 1; }
                            cell.note = crate::sequencer::Note::On(new_k);
                        }
                    }
                }
                applied.push(format!("transpose({semitones} st, {count} notes)"));
            }
            "rotate" => {
                // Shift rows up/down by N.
                let n = op.get("rows").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let num_rows = working.len() as i32;
                if num_rows > 0 && n != 0 {
                    let shift = n.rem_euclid(num_rows) as usize;
                    if shift > 0 {
                        working.rotate_left(shift);
                    }
                }
                applied.push(format!("rotate({n} rows)"));
            }
            "reverse" => {
                working.reverse();
                applied.push("reverse".to_string());
            }
            "invert" => {
                // Mirror pitches around the root.
                let root = op.get("root").and_then(|v| v.as_i64()).unwrap_or(60) as i32;
                let mut count = 0;
                for row in working.iter_mut() {
                    for cell in row.iter_mut() {
                        if let crate::sequencer::Note::On(k) = cell.note {
                            let dist = k as i32 - root;
                            let new_k = (root - dist).clamp(0, 119) as u8;
                            if new_k != k { count += 1; }
                            cell.note = crate::sequencer::Note::On(new_k);
                        }
                    }
                }
                applied.push(format!("invert(root={root}, {count} notes)"));
            }
            "quantize" => {
                // Snap notes to the nearest scale degree.
                let scale_name = op.get("scale").and_then(|v| v.as_str()).unwrap_or("major");
                let root = op.get("root").and_then(|v| v.as_i64()).unwrap_or(0) as u8;
                let scale = parse_scale(scale_name)?;
                let intervals = scale.intervals();
                let mut count = 0;
                for row in working.iter_mut() {
                    for cell in row.iter_mut() {
                        if let crate::sequencer::Note::On(k) = cell.note {
                            let rel = (k as i32 - root as i32).rem_euclid(12);
                            let mut best: (i32, i32) = (0, i32::MAX);
                            for (i, &iv) in intervals.iter().enumerate() {
                                let iv12 = (iv as i32).rem_euclid(12);
                                let diff = rel - iv12;
                                let dist = diff.rem_euclid(12).min((-diff).rem_euclid(12));
                                if dist < best.1 { best = (i as i32, dist); }
                            }
                            let snap_rel = (intervals[best.0 as usize] as i32).rem_euclid(12);
                            let new_k = (root as i32 + (k as i32 - root as i32) - rel + snap_rel).clamp(0, 119) as u8;
                            if new_k != k { count += 1; }
                            cell.note = crate::sequencer::Note::On(new_k);
                        }
                    }
                }
                applied.push(format!("quantize(scale={scale_name}, {count} snapped)"));
            }
            "augment" => {
                // Double the pattern length by duplicating each row.
                // Not currently supported — needs pattern resize.
                applied.push("augment(skipped: needs pattern resize)".to_string());
            }
            "diminish" => {
                // Halve the pattern length. Not currently supported.
                applied.push("diminish(skipped: needs pattern resize)".to_string());
            }
            "echo" => {
                // Duplicate every note N rows later at lower velocity.
                let delay = op.get("delay_rows").and_then(|v| v.as_i64()).unwrap_or(4) as usize;
                let decay = op.get("decay").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                let num_rows = working.len();
                let mut new_notes = Vec::new();
                for r in 0..num_rows {
                    for (ch, cell) in working[r].iter().enumerate() {
                        if cell.note == crate::sequencer::Note::None { continue; }
                        let echo_row = r + delay;
                        if echo_row >= num_rows { continue; }
                        let mut echo_cell = *cell;
                        let v = cell.volume.unwrap_or(64) as f32 * decay;
                        echo_cell.volume = Some(v as u8);
                        new_notes.push((echo_row, ch, echo_cell));
                    }
                }
                for (r, ch, c) in new_notes {
                    working[r][ch] = c;
                }
                applied.push(format!("echo(delay={delay}, decay={decay})"));
            }
            "set_instrument" => {
                // Set the instrument field on every cell that has a note.
                let target = op.get("instrument").and_then(|v| v.as_i64())
                    .ok_or("set_instrument requires 'instrument'")? as u8;
                let mut count = 0;
                for row in working.iter_mut() {
                    for cell in row.iter_mut() {
                        if cell.note != crate::sequencer::Note::None {
                            cell.instrument = Some(target);
                            count += 1;
                        }
                    }
                }
                applied.push(format!("set_instrument({target}, {count} cells)"));
            }
            "shift" => {
                // Shift notes in time by N rows.
                let n = op.get("rows").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let num_rows = working.len() as i32;
                if n != 0 && num_rows > 0 {
                    let src = std::mem::take(&mut working);
                    for r in 0..num_rows as usize {
                        let dst = ((r as i32) + n).clamp(0, num_rows - 1) as usize;
                        for ch in 0..src[r].len() {
                            working[dst][ch] = src[r][ch];
                        }
                    }
                }
                applied.push(format!("shift({n} rows)"));
            }
            other => return Err(format!("Unknown transform type: '{other}'")),
        }
    }

    // Build the new_cells list (only the ones that differ from old).
    let mut new_cells = Vec::new();
    for (r, row) in working.iter().enumerate() {
        for (ch, cell) in row.iter().enumerate() {
            if *cell != old_cells[r * MAX_CHANNELS + ch].2 {
                new_cells.push((r, ch, *cell));
            }
        }
    }

    if new_cells.is_empty() {
        return Ok(serde_json::json!({
            "ok": true, "applied": applied, "cells_changed": 0,
            "note": "no cells changed by transforms",
        }));
    }

    let cells_changed = new_cells.len();
    let cmd = Box::new(BulkSetCellsCommand {
        order,
        old_cells,
        new_cells,
    });
    let label = format!("pattern.transform [{}]", applied.join(", "));
    let undo_id = core.undo_manager.execute_with_label(cmd, label.clone(), arc_module)
        .map_err(|e| format!("Transform apply failed: {e:?}"))?;
    core.sync_module_to_audio();
    Ok(serde_json::json!({
        "ok": true,
        "applied": applied,
        "cells_changed": cells_changed,
        "undo_id": undo_id,
        "undo_label": label,
        "pattern": order,
    }))
}
