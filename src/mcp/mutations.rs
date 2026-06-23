use std::sync::Arc;

use crate::audio::commands::AudioCommand;
use crate::core::HtrkCore;
use crate::edit::{
    SetCellCommand, BulkSetCellsCommand, TransposeCommand, InterpolateCommand,
    SetSamplePropertyCommand, SetInstrumentPropertyCommand, MapNoteToSampleCommand,
    SetSampleMapCommand, AddEnvelopePointCommand, RemoveEnvelopePointCommand,
    SetEnvelopePointsCommand, SampleProperty, InstrumentProperty, EnvelopeType,
};
use crate::mcp::protocol::CmdResult;
use crate::sequencer::automation::{AutomationTrack, AutomationPoint, AutomationTarget, InterpolationMode};
use crate::sequencer::effect::{FilterType, SendEffectType, NUM_SEND_BUSES};
use crate::sequencer::instrument::{Instrument, EnvelopePoint};
use crate::sequencer::module::{Module, PANNING_CENTER, VOLUME_MAX};
use crate::sequencer::note::Note;
use crate::sequencer::pattern::{Cell, MAX_CHANNELS};
use crate::sequencer::sample::LoopType;

// ── Note name parser ──

fn parse_note(s: &str) -> Result<Note, String> {
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

// ── Entry point ──

pub fn execute_mutation(core: &mut HtrkCore, method: &str, params: &serde_json::Value) -> CmdResult {
    match method {
        "module.create" => cmd_module_create(core, params),
        "module.load"   => cmd_module_load(core, params),
        "module.save"   => cmd_module_save(core, params),
        "order.set"     => cmd_order_set(core, params),
        "order.append"  => cmd_order_append(core, params),
        "order.insert"  => cmd_order_insert(core, params),
        "order.remove"  => cmd_order_remove(core, params),
        "order.set_entry" => cmd_order_set_entry(core, params),
        "pattern.ensure" => cmd_pattern_ensure(core, params),
        "cell.set"      => cmd_cell_set(core, params),
        "cell.set_batch"=> cmd_cell_set_batch(core, params),
        "pattern.fill"  => cmd_pattern_fill(core, params),
        "pattern.clear" => cmd_pattern_clear(core, params),
        "pattern.transpose" => cmd_pattern_transpose(core, params),
        "pattern.interpolate" => cmd_pattern_interpolate(core, params),

        // ── Instrument tools ──
        "instrument.create"    => cmd_instrument_create(core, params),
        "instrument.remove"    => cmd_instrument_remove(core, params),
        "instrument.set_property" => cmd_instrument_set_property(core, params),
        "instrument.map_note"  => cmd_instrument_map_note(core, params),
        "instrument.map_range" => cmd_instrument_map_range(core, params),

        // ── Sample tools ──
        "sample.load"          => cmd_sample_load(core, params),
        "sample.remove"        => cmd_sample_remove(core, params),
        "sample.set_property"  => cmd_sample_set_property(core, params),

        // ── Envelope tools ──
        "envelope.set"         => cmd_envelope_set(core, params),
        "envelope.add_point"   => cmd_envelope_add_point(core, params),
        "envelope.remove_point" => cmd_envelope_remove_point(core, params),
        "envelope.generate"    => cmd_envelope_generate(core, params),

        // ── Automation tools ──
        "automation.create"    => cmd_automation_create(core, params),
        "automation.remove"    => cmd_automation_remove(core, params),
        "automation.add_point" => cmd_automation_add_point(core, params),
        "automation.clear"     => cmd_automation_clear(core, params),

        // ── Playback tools ──
        "playback.play"        => cmd_playback_play(core, params),
        "playback.stop"        => cmd_playback_stop(core, params),
        "playback.set_position" => cmd_playback_set_position(core, params),
        "playback.set_bpm"     => cmd_playback_set_bpm(core, params),
        "playback.set_speed"   => cmd_playback_set_speed(core, params),

        // ── Channel tools ──
        "channel.set_panning"  => cmd_channel_set_panning(core, params),
        "channel.set_volume"   => cmd_channel_set_volume(core, params),
        "channel.set_mute"     => cmd_channel_set_mute(core, params),
        "channel.set_solo"     => cmd_channel_set_solo(core, params),

        // ── Send FX tools ──
        "sendfx.set_bus"       => cmd_sendfx_set_bus(core, params),
        "sendfx.set_return_level" => cmd_sendfx_set_return_level(core, params),

        _ => Err(format!("Unknown mutation tool: '{method}'")),
    }
}

// ── Tool implementations ──

fn cmd_module_create(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let name = get_str!(params, "name").unwrap_or_else(|| "Untitled".into());
    let channels = get_i64!(params, "channels").unwrap_or(4) as usize;
    let bpm = get_i64!(params, "bpm").unwrap_or(125) as u16;
    let speed = get_i64!(params, "speed").unwrap_or(6) as u8;

    let mut module = Module::default();
    module.name = name.clone();
    module.channel_panning = vec![PANNING_CENTER; channels.max(1).min(64)];
    module.channel_volume = vec![VOLUME_MAX; channels.max(1).min(64)];
    module.initial_bpm = bpm;
    module.initial_speed = speed;
    module.order_list = vec![0];
    module.patterns.push(crate::sequencer::pattern::Pattern::new(64));

    core.load_module(module, name, None);
    Ok(serde_json::json!({"ok": true}))
}

fn cmd_module_load(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let path = get_str!(params, "path").ok_or("Missing 'path'")?;
    let data = std::fs::read(&path).map_err(|e| format!("Failed to read '{path}': {e}"))?;
    let module = crate::formats::load_module(&data)
        .map_err(|e| format!("Failed to load module: {e}"))?;
    let name = std::path::Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    core.load_module(module, name, Some(path));
    Ok(serde_json::json!({"ok": true}))
}

fn cmd_module_save(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let path = get_str!(params, "path")
        .or_else(|| core.file_path().map(|s| s.to_string()))
        .ok_or("No file path specified and no current file path")?;
    let ok = core.save_file(&path);
    if ok {
        Ok(serde_json::json!({"ok": true, "path": path}))
    } else {
        Err("Failed to save file".into())
    }
}

fn order_mutate<F>(core: &mut HtrkCore, f: F) -> CmdResult
where F: FnOnce(&mut Vec<u8>) -> Result<(), String>
{
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            f(&mut arc_module.order_list)?;
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_order_set(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let entries: Vec<u8> = params.get("entries").and_then(|v| v.as_array())
        .ok_or("Missing 'entries'")?
        .iter()
        .map(|v| v.as_i64().ok_or("entries must be integers").map(|n| n as u8))
        .collect::<Result<Vec<u8>, _>>()?;
    order_mutate(core, |order_list| {
        *order_list = entries;
        Ok(())
    })
}

fn cmd_order_append(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let pat = get_i64!(params, "pattern_index").ok_or("Missing 'pattern_index'")? as u8;
    order_mutate(core, |order_list| {
        order_list.push(pat);
        Ok(())
    })
}

fn cmd_order_insert(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let pos = get_i64!(params, "position").ok_or("Missing 'position'")? as usize;
    let pat = get_i64!(params, "pattern_index").ok_or("Missing 'pattern_index'")? as u8;
    order_mutate(core, |order_list| {
        let pos = pos.min(order_list.len());
        order_list.insert(pos, pat);
        Ok(())
    })
}

fn cmd_order_remove(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let pos = get_i64!(params, "position").ok_or("Missing 'position'")? as usize;
    order_mutate(core, |order_list| {
        if pos >= order_list.len() {
            return Err(format!("Position {pos} out of range (len={})", order_list.len()));
        }
        order_list.remove(pos);
        Ok(())
    })
}

fn cmd_order_set_entry(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let pos = get_i64!(params, "position").ok_or("Missing 'position'")? as usize;
    let pat = get_i64!(params, "pattern_index").ok_or("Missing 'pattern_index'")? as u8;
    order_mutate(core, |order_list| {
        if pos >= order_list.len() {
            return Err(format!("Position {pos} out of range (len={})", order_list.len()));
        }
        order_list[pos] = pat;
        Ok(())
    })
}

fn cmd_pattern_ensure(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

fn build_cell_from_params(params: &serde_json::Value, defaults: &Cell) -> Cell {
    let mut cell = *defaults;
    if let Some(note_str) = get_str!(params, "note") {
        if let Ok(note) = parse_note(&note_str) {
            cell.note = note;
        }
    }
    if let Some(inst) = get_i64!(params, "instrument") {
        cell.instrument = if inst == 0 { None } else { Some(inst as u8) };
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

fn cmd_cell_set(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

fn cmd_cell_set_batch(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

fn cmd_pattern_fill(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

fn cmd_pattern_clear(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

fn cmd_pattern_transpose(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

fn cmd_pattern_interpolate(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

// ── Instrument handlers ──

fn cmd_instrument_create(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let name = get_str!(params, "name").unwrap_or_default();
    let base_sample = get_i64!(params, "base_sample").map(|v| v as u8);

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if arc_module.instruments.len() >= 256 {
                return Err("Maximum instruments reached (256)".into());
            }
            let mut inst = Instrument::default();
            inst.name = name;
            if let Some(sample_idx) = base_sample {
                for i in 0..120 {
                    inst.sample_map[i] = sample_idx;
                }
            }
            let idx = arc_module.instruments.len();
            arc_module.instruments.push(inst);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "index": idx}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_instrument_remove(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {idx} out of range"));
            }
            if arc_module.instruments.len() <= 1 {
                return Err("Cannot remove the last instrument".into());
            }
            let name = arc_module.instruments[idx].name.clone();
            arc_module.instruments.remove(idx);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "removed_index": idx, "name": name}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_instrument_set_property(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let prop_name = get_str!(params, "property").ok_or("Missing 'property'")?;
    let value = params.get("value").ok_or("Missing 'value'")?;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {idx} out of range"));
            }
            let inst = &arc_module.instruments[idx];
            let (property, old_property): (InstrumentProperty, InstrumentProperty) = match prop_name.as_str() {
                "name" => {
                    let v = value.as_str().ok_or("'value' must be a string")?.to_string();
                    (InstrumentProperty::Name(v.clone()), InstrumentProperty::Name(inst.name.clone()))
                }
                "fade_out" | "fadeout" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u16;
                    (InstrumentProperty::Fadeout(v), InstrumentProperty::Fadeout(inst.fade_out))
                }
                "global_volume" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::GlobalVolume(v), InstrumentProperty::GlobalVolume(inst.global_volume))
                }
                "nna" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "cut" | "note_cut" => crate::sequencer::instrument::NewNoteAction::NoteCut,
                        "continue" => crate::sequencer::instrument::NewNoteAction::Continue,
                        "off" | "note_off" => crate::sequencer::instrument::NewNoteAction::NoteOff,
                        "fade" | "note_fade" => crate::sequencer::instrument::NewNoteAction::NoteFade,
                        _ => return Err(format!("Unknown NNA: '{s}'")),
                    };
                    (InstrumentProperty::Nna(v), InstrumentProperty::Nna(inst.nna))
                }
                "dct" | "duplicate_check_type" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "disabled" => crate::sequencer::instrument::DuplicateCheckType::Disabled,
                        "note" => crate::sequencer::instrument::DuplicateCheckType::Note,
                        "sample" => crate::sequencer::instrument::DuplicateCheckType::Sample,
                        "instrument" => crate::sequencer::instrument::DuplicateCheckType::Instrument,
                        _ => return Err(format!("Unknown DCT: '{s}'")),
                    };
                    (InstrumentProperty::DuplicateCheckType(v), InstrumentProperty::DuplicateCheckType(inst.duplicate_check_type))
                }
                "dca" | "duplicate_check_action" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "cut" | "note_cut" => crate::sequencer::instrument::DuplicateCheckAction::NoteCut,
                        "off" | "note_off" => crate::sequencer::instrument::DuplicateCheckAction::NoteOff,
                        "fade" | "note_fade" => crate::sequencer::instrument::DuplicateCheckAction::NoteFade,
                        _ => return Err(format!("Unknown DCA: '{s}'")),
                    };
                    (InstrumentProperty::DuplicateCheckAction(v), InstrumentProperty::DuplicateCheckAction(inst.duplicate_check_action))
                }
                "pitch_pan_separation" | "pitch_pan_sep" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as i8;
                    (InstrumentProperty::PitchPanSeparation(v), InstrumentProperty::PitchPanSeparation(inst.pitch_pan_separation))
                }
                "pitch_pan_center" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::PitchPanCenter(v), InstrumentProperty::PitchPanCenter(inst.pitch_pan_center))
                }
                "random_volume" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::RandomVolume(v), InstrumentProperty::RandomVolume(inst.random_volume))
                }
                "random_panning" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::RandomPanning(v), InstrumentProperty::RandomPanning(inst.random_panning))
                }
                "filter_cutoff" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u16;
                    (InstrumentProperty::FilterCutoff(v), InstrumentProperty::FilterCutoff(inst.filter_cutoff))
                }
                "filter_resonance" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::FilterResonance(v), InstrumentProperty::FilterResonance(inst.filter_resonance))
                }
                "filter_type" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "lowpass" | "lp" => FilterType::LowPass,
                        "highpass" | "hp" => FilterType::HighPass,
                        "bandpass" | "bp" => FilterType::BandPass,
                        "notch" => FilterType::Notch,
                        _ => return Err(format!("Unknown filter type: '{s}'")),
                    };
                    (InstrumentProperty::FilterType(v), InstrumentProperty::FilterType(inst.filter_type))
                }
                "filter_random_cutoff" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::FilterRandomCutoff(v), InstrumentProperty::FilterRandomCutoff(inst.filter_random_cutoff))
                }
                "vib_type" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::VibType(v), InstrumentProperty::VibType(inst.vib_type))
                }
                "vib_sweep" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::VibSweep(v), InstrumentProperty::VibSweep(inst.vib_sweep))
                }
                "vib_depth" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::VibDepth(v), InstrumentProperty::VibDepth(inst.vib_depth))
                }
                "vib_rate" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (InstrumentProperty::VibRate(v), InstrumentProperty::VibRate(inst.vib_rate))
                }
                _ => return Err(format!("Unknown instrument property: '{prop_name}'")),
            };
            let cmd = Box::new(SetInstrumentPropertyCommand { instrument_index: idx, property, old_property });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

fn note_name_to_midi_key(s: &str) -> Result<u8, String> {
    let tone_names = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];
    let s_upper = s.to_uppercase();
    if s_upper.len() < 3 {
        if let Ok(k) = s_upper.parse::<u8>() {
            if k <= 119 { return Ok(k); }
        }
        return Err(format!("Invalid note: '{s}'"));
    }
    let tone_str = &s_upper[..2];
    let octave_str = &s_upper[2..];
    let tone = tone_names.iter().position(|&t| t == tone_str)
        .ok_or_else(|| format!("Unknown note name: '{s}'"))?;
    let octave = octave_str.parse::<u8>().map_err(|_| format!("Invalid octave in '{s}'"))?;
    let key = octave * 12 + tone as u8;
    if key > 119 { return Err(format!("Note '{s}' out of range")); }
    Ok(key)
}

fn cmd_instrument_map_note(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let note_str = get_str!(params, "note").ok_or("Missing 'note'")?;
    let sample_idx = get_i64!(params, "sample_index").ok_or("Missing 'sample_index'")? as u8;
    let note = note_name_to_midi_key(&note_str)?;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {idx} out of range"));
            }
            let old_sample = arc_module.instruments[idx].sample_map[note as usize];
            let cmd = Box::new(MapNoteToSampleCommand { instrument_index: idx, note, old_sample, new_sample: sample_idx });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "note": note_str, "sample": sample_idx}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_instrument_map_range(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let note_start_str = get_str!(params, "note_start").ok_or("Missing 'note_start'")?;
    let note_end_str = get_str!(params, "note_end").ok_or("Missing 'note_end'")?;
    let sample_idx = get_i64!(params, "sample_index").ok_or("Missing 'sample_index'")? as u8;
    let note_start = note_name_to_midi_key(&note_start_str)?;
    let note_end = note_name_to_midi_key(&note_end_str)?;
    if note_start > note_end {
        return Err("note_start must be <= note_end".into());
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {idx} out of range"));
            }
            let old_map = arc_module.instruments[idx].sample_map;
            let cmd = Box::new(SetSampleMapCommand { instrument_index: idx, new_sample_index: sample_idx, old_map });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "notes_affected": (note_end - note_start + 1) as u64, "sample": sample_idx}));
        }
    }
    Err("No module loaded".into())
}

// ── Sample handlers ──

fn cmd_sample_load(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let path = get_str!(params, "path").ok_or("Missing 'path'")?;
    let name = get_str!(params, "name");
    let wav_data = std::fs::read(&path).map_err(|e| format!("Failed to read '{path}': {e}"))?;
    let mut sample = crate::formats::wav::import_wav(&wav_data)
        .map_err(|e| format!("Failed to import WAV: {e}"))?;
    if let Some(n) = name {
        sample.name = n;
    } else {
        if let Some(stem) = std::path::Path::new(&path).file_stem().and_then(|s| s.to_str()) {
            sample.name = stem.to_string();
        }
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let idx = arc_module.samples.len();
            arc_module.samples.push(sample);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "index": idx, "path": path}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_sample_remove(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.samples.len() {
                return Err(format!("Sample {idx} out of range"));
            }
            let name = arc_module.samples[idx].name.clone();
            arc_module.samples.remove(idx);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "removed_index": idx, "name": name}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_sample_set_property(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let idx = get_i64!(params, "index").ok_or("Missing 'index'")? as usize;
    let prop_name = get_str!(params, "property").ok_or("Missing 'property'")?;
    let value = params.get("value").ok_or("Missing 'value'")?;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if idx >= arc_module.samples.len() {
                return Err(format!("Sample {idx} out of range"));
            }
            let sample = &arc_module.samples[idx];
            let (property, old_property): (SampleProperty, SampleProperty) = match prop_name.as_str() {
                "name" => {
                    let v = value.as_str().ok_or("'value' must be a string")?.to_string();
                    (SampleProperty::Name(v.clone()), SampleProperty::Name(sample.name.clone()))
                }
                "default_volume" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (SampleProperty::DefaultVolume(v), SampleProperty::DefaultVolume(sample.default_volume))
                }
                "default_panning" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (SampleProperty::DefaultPanning(v), SampleProperty::DefaultPanning(sample.default_panning))
                }
                "global_volume" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as u8;
                    (SampleProperty::GlobalVolume(v), SampleProperty::GlobalVolume(sample.global_volume))
                }
                "loop_type" => {
                    let s = value.as_str().ok_or("'value' must be a string")?;
                    let v = match s {
                        "none" | "off" => LoopType::None,
                        "forward" | "fw" => LoopType::Forward,
                        "pingpong" | "pp" => LoopType::PingPong,
                        "backward" | "bw" => LoopType::Backward,
                        _ => return Err(format!("Unknown loop type: '{s}'")),
                    };
                    (SampleProperty::LoopType(v), SampleProperty::LoopType(sample.loop_type))
                }
                "loop_start" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as usize;
                    (SampleProperty::LoopStart(v), SampleProperty::LoopStart(sample.loop_start))
                }
                "loop_end" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as usize;
                    (SampleProperty::LoopEnd(v), SampleProperty::LoopEnd(sample.loop_end))
                }
                "relative_note" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as i8;
                    (SampleProperty::RelativeNote(v), SampleProperty::RelativeNote(sample.relative_note))
                }
                "fine_tune" => {
                    let v = value.as_i64().ok_or("'value' must be an integer")? as i8;
                    (SampleProperty::FineTune(v), SampleProperty::FineTune(sample.fine_tune))
                }
                _ => return Err(format!("Unknown sample property: '{prop_name}'")),
            };
            let cmd = Box::new(SetSamplePropertyCommand { sample_index: idx, property, old_property });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

// ── Envelope handlers ──

fn parse_envelope_type(s: &str) -> Result<EnvelopeType, String> {
    match s {
        "volume" => Ok(EnvelopeType::Volume),
        "panning" => Ok(EnvelopeType::Panning),
        "pitch" => Ok(EnvelopeType::Pitch),
        "filter" => Ok(EnvelopeType::Filter),
        _ => Err(format!("Unknown envelope type: '{s}'")),
    }
}

fn get_envelope<'a>(inst: &'a Instrument, et: EnvelopeType) -> &'a Option<crate::sequencer::instrument::Envelope> {
    match et {
        EnvelopeType::Volume => &inst.volume_envelope,
        EnvelopeType::Panning => &inst.panning_envelope,
        EnvelopeType::Pitch => &inst.pitch_envelope,
        EnvelopeType::Filter => &inst.filter_envelope,
    }
}

fn get_envelope_mut<'a>(inst: &'a mut Instrument, et: EnvelopeType) -> &'a mut Option<crate::sequencer::instrument::Envelope> {
    match et {
        EnvelopeType::Volume => &mut inst.volume_envelope,
        EnvelopeType::Panning => &mut inst.panning_envelope,
        EnvelopeType::Pitch => &mut inst.pitch_envelope,
        EnvelopeType::Filter => &mut inst.filter_envelope,
    }
}

fn cmd_envelope_set(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let inst_idx = get_i64!(params, "instrument").ok_or("Missing 'instrument'")? as usize;
    let type_str = get_str!(params, "type").ok_or("Missing 'type'")?;
    let et = parse_envelope_type(&type_str)?;
    let points_val = params.get("points").and_then(|v| v.as_array()).ok_or("Missing 'points'")?;

    let mut points = Vec::with_capacity(points_val.len());
    for p in points_val {
        let tick = p.get("tick").and_then(|v| v.as_i64()).ok_or("Each point needs 'tick'")? as u16;
        let value = p.get("value").and_then(|v| v.as_i64()).ok_or("Each point needs 'value'")? as u8;
        points.push(EnvelopePoint { tick, value });
    }
    let num_points = points.len();

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if inst_idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {inst_idx} out of range"));
            }
            let inst = &arc_module.instruments[inst_idx];
            let old_env = get_envelope(inst, et).clone();
            let old_points = match &old_env {
                Some(e) => e.points.clone(),
                None => Vec::new(),
            };
            let cmd = Box::new(SetEnvelopePointsCommand {
                instrument_index: inst_idx,
                envelope_type: et,
                new_points: points,
                old_points,
                old_envelope: old_env,
            });
            let _ = core.undo_manager.execute(cmd, arc_module);

            // Apply sustain/loop settings if provided
            if let Some(inst2) = Arc::get_mut(module) {
                if inst_idx < inst2.instruments.len() {
                    let env = get_envelope_mut(&mut inst2.instruments[inst_idx], et);
                    if let Some(e) = env {
                        if let Some(sp) = params.get("sustain_point").and_then(|v| v.as_i64()) {
                            e.sustain_point = if sp >= 0 { Some(sp as usize) } else { None };
                        }
                        if let Some(ls) = params.get("loop_start").and_then(|v| v.as_i64()) {
                            e.loop_start = if ls >= 0 { Some(ls as usize) } else { None };
                        }
                        if let Some(le) = params.get("loop_end").and_then(|v| v.as_i64()) {
                            e.loop_end = if le >= 0 { Some(le as usize) } else { None };
                        }
                        if let Some(enabled) = params.get("enabled").and_then(|v| v.as_bool()) {
                            e.flags.enabled = enabled;
                        }
                        if let Some(sustain) = params.get("sustain").and_then(|v| v.as_bool()) {
                            e.flags.sustain = sustain;
                        }
                        if let Some(loop_flag) = params.get("loop").and_then(|v| v.as_bool()) {
                            e.flags.loop_ = loop_flag;
                        }
                    }
                }
            }

            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "num_points": num_points}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_envelope_add_point(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let inst_idx = get_i64!(params, "instrument").ok_or("Missing 'instrument'")? as usize;
    let type_str = get_str!(params, "type").ok_or("Missing 'type'")?;
    let et = parse_envelope_type(&type_str)?;
    let tick = get_i64!(params, "tick").ok_or("Missing 'tick'")? as u16;
    let value = get_i64!(params, "value").ok_or("Missing 'value'")? as u8;
    let point = EnvelopePoint { tick, value };

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if inst_idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {inst_idx} out of range"));
            }
            let cmd = Box::new(AddEnvelopePointCommand { instrument_index: inst_idx, envelope_type: et, point });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_envelope_remove_point(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let inst_idx = get_i64!(params, "instrument").ok_or("Missing 'instrument'")? as usize;
    let type_str = get_str!(params, "type").ok_or("Missing 'type'")?;
    let et = parse_envelope_type(&type_str)?;
    let point_idx = get_i64!(params, "point_index").ok_or("Missing 'point_index'")? as usize;

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if inst_idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {inst_idx} out of range"));
            }
            // Capture old_point for undo
            let old_point = {
                let inst = &arc_module.instruments[inst_idx];
                let env = get_envelope(inst, et);
                match env {
                    Some(e) => e.points.get(point_idx).copied(),
                    None => None,
                }
            };
            match old_point {
                Some(op) => {
                    let cmd = Box::new(RemoveEnvelopePointCommand {
                        instrument_index: inst_idx,
                        envelope_type: et,
                        point_index: point_idx,
                        old_point: op,
                    });
                    let _ = core.undo_manager.execute(cmd, arc_module);
                    core.sync_module_to_audio();
                    Ok(serde_json::json!({"ok": true}))
                }
                None => Err(format!("Point index {point_idx} out of range")),
            }
        } else {
            Err("Failed to get exclusive module access".into())
        }
    } else {
        Err("No module loaded".into())
    }
}

fn cmd_envelope_generate(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let inst_idx = get_i64!(params, "instrument").ok_or("Missing 'instrument'")? as usize;
    let type_str = get_str!(params, "type").ok_or("Missing 'type'")?;
    let et = parse_envelope_type(&type_str)?;
    let shape_str = get_str!(params, "shape").ok_or("Missing 'shape'")?;
    let num_points = get_i64!(params, "num_points").ok_or("Missing 'num_points'")? as u16;
    let tick_span = get_i64!(params, "tick_span").ok_or("Missing 'tick_span'")? as u16;
    let amplitude = get_f64!(params, "amplitude").ok_or("Missing 'amplitude'")? as f32;
    let offset = get_f64!(params, "offset").unwrap_or(0.5) as f32;

    let shape = match shape_str.as_str() {
        "sine" => crate::sequencer::envelope_generator::GeneratorShape::Sine,
        "square" => crate::sequencer::envelope_generator::GeneratorShape::Square,
        "triangle" => crate::sequencer::envelope_generator::GeneratorShape::Triangle,
        "saw_up" | "sawup" => crate::sequencer::envelope_generator::GeneratorShape::SawUp,
        "saw_down" | "sawdown" => crate::sequencer::envelope_generator::GeneratorShape::SawDown,
        "pulse" => crate::sequencer::envelope_generator::GeneratorShape::Pulse,
        "random" => crate::sequencer::envelope_generator::GeneratorShape::Random,
        _ => return Err(format!("Unknown shape: '{shape_str}'")),
    };

    if tick_span < 2 {
        return Err("tick_span must be at least 2".into());
    }
    if num_points < 2 {
        return Err("num_points must be at least 2".into());
    }

    let generated = crate::sequencer::envelope_generator::generate_values(
        shape, tick_span, num_points as f32, amplitude, offset, 50.0,
    );

    let mut points: Vec<EnvelopePoint> = generated.into_iter()
        .map(|(tick, val)| {
            let v = (val * 64.0).round().clamp(0.0, 64.0) as u8;
            EnvelopePoint { tick, value: v }
        })
        .collect();
    points.sort_by_key(|p| p.tick);
    points.dedup_by_key(|p| p.tick);
    let num_points_out = points.len();

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if inst_idx >= arc_module.instruments.len() {
                return Err(format!("Instrument {inst_idx} out of range"));
            }
            let inst = &arc_module.instruments[inst_idx];
            let old_env = get_envelope(inst, et).clone();
            let old_points = match &old_env {
                Some(e) => e.points.clone(),
                None => Vec::new(),
            };
            let cmd = Box::new(SetEnvelopePointsCommand {
                instrument_index: inst_idx,
                envelope_type: et,
                new_points: points,
                old_points,
                old_envelope: old_env,
            });
            let _ = core.undo_manager.execute(cmd, arc_module);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "num_points": num_points_out}));
        }
    }
    Err("No module loaded".into())
}

// ── Automation handlers ──

fn parse_target_type(s: &str) -> Result<AutomationTarget, String> {
    match s {
        "channel_volume" | "ChannelVolume" => Ok(AutomationTarget::ChannelVolume),
        "channel_panning" | "ChannelPanning" => Ok(AutomationTarget::ChannelPanning),
        "filter_cutoff" | "FilterCutoff" => Ok(AutomationTarget::FilterCutoff),
        "filter_resonance" | "FilterResonance" => Ok(AutomationTarget::FilterResonance),
        "global_volume" | "GlobalVolume" => Ok(AutomationTarget::GlobalVolume),
        "tempo" | "Tempo" => Ok(AutomationTarget::Tempo),
        "speed" | "Speed" => Ok(AutomationTarget::Speed),
        "send_level" | "SendLevel" => Ok(AutomationTarget::SendLevel { bus: 0 }),
        "send_return" | "SendReturnLevel" => Ok(AutomationTarget::SendReturnLevel { bus: 0 }),
        "send_bus_param" | "SendBusParam" => Ok(AutomationTarget::SendBusParam { bus: 0, param: 0 }),
        _ => Err(format!("Unknown automation target type: '{s}'")),
    }
}

fn parse_interpolation(s: &str) -> Result<InterpolationMode, String> {
    match s {
        "hold" => Ok(InterpolationMode::Hold),
        "linear" => Ok(InterpolationMode::Linear),
        "smooth" => Ok(InterpolationMode::Smooth),
        "exponential" => Ok(InterpolationMode::Exponential),
        _ => Err(format!("Unknown interpolation mode: '{s}'")),
    }
}

fn cmd_automation_create(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let target_obj = params.get("target").ok_or("Missing 'target'")?;
    let target_type_str = target_obj.get("type").and_then(|v| v.as_str()).ok_or("target needs 'type'")?;
    let channel = target_obj.get("channel").and_then(|v| v.as_i64()).map(|v| v as usize);
    let interp_str = get_str!(params, "interpolation").unwrap_or_else(|| "linear".into());
    let interp = parse_interpolation(&interp_str)?;

    let mut target = parse_target_type(target_type_str)?;
    if let Some(ch) = channel {
        match &mut target {
            AutomationTarget::SendLevel { bus } => *bus = ch as u8,
            AutomationTarget::SendReturnLevel { bus } => *bus = ch as u8,
            AutomationTarget::SendBusParam { bus, param: _ } => *bus = ch as u8,
            _ => {}
        }
    }
    if let Some(bus_val) = target_obj.get("bus").and_then(|v| v.as_i64()) {
        match &mut target {
            AutomationTarget::SendLevel { bus } => *bus = bus_val as u8,
            AutomationTarget::SendReturnLevel { bus } => *bus = bus_val as u8,
            AutomationTarget::SendBusParam { bus, param: _ } => *bus = bus_val as u8,
            _ => {}
        }
    }
    if let Some(param_val) = target_obj.get("param").and_then(|v| v.as_i64()) {
        if let AutomationTarget::SendBusParam { bus: _, param } = &mut target {
            *param = param_val as u8;
        }
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let id = arc_module.next_automation_id;
            arc_module.next_automation_id += 1;
            let mut track = AutomationTrack::new(id, target, channel);
            track.default_interp = interp;
            arc_module.automation_tracks.push(track);
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "track_id": id}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_automation_remove(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let track_id = get_i64!(params, "track_id").ok_or("Missing 'track_id'")? as u32;
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            let before = arc_module.automation_tracks.len();
            arc_module.automation_tracks.retain(|t| t.id != track_id);
            if arc_module.automation_tracks.len() == before {
                return Err(format!("Track {track_id} not found"));
            }
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_automation_add_point(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let track_id = get_i64!(params, "track_id").ok_or("Missing 'track_id'")? as u32;
    let order = get_i64!(params, "order").ok_or("Missing 'order'")? as u16;
    let row = get_i64!(params, "row").ok_or("Missing 'row'")? as u16;
    let value = get_f64!(params, "value").ok_or("Missing 'value'")? as f32;
    let interp = get_str!(params, "interp").and_then(|s| parse_interpolation(&s).ok()).unwrap_or(InterpolationMode::Linear);

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if let Some(track) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                track.insert_point(AutomationPoint { order, row, value, interp_to_next: interp });
                core.sync_module_to_audio();
                return Ok(serde_json::json!({"ok": true}));
            }
            return Err(format!("Track {track_id} not found"));
        }
    }
    Err("No module loaded".into())
}

fn cmd_automation_clear(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let track_id = get_i64!(params, "track_id").ok_or("Missing 'track_id'")? as u32;
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if let Some(track) = arc_module.automation_tracks.iter_mut().find(|t| t.id == track_id) {
                track.points.clear();
                core.sync_module_to_audio();
                return Ok(serde_json::json!({"ok": true}));
            }
            return Err(format!("Track {track_id} not found"));
        }
    }
    Err("No module loaded".into())
}

// ── Playback handlers ──

fn cmd_playback_play(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    if let Some(order) = get_i64!(params, "from_order") {
        let row = get_i64!(params, "from_row").unwrap_or(0) as u16;
        core.send_command(AudioCommand::PlayFrom { order: order as u16, row });
    } else {
        core.send_command(AudioCommand::Play);
    }
    Ok(serde_json::json!({"ok": true}))
}

fn cmd_playback_stop(core: &mut HtrkCore, _params: &serde_json::Value) -> CmdResult {
    core.send_command(AudioCommand::Stop);
    Ok(serde_json::json!({"ok": true}))
}

fn cmd_playback_set_position(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let order = get_i64!(params, "order").ok_or("Missing 'order'")? as u16;
    let row = get_i64!(params, "row").ok_or("Missing 'row'")? as u16;
    core.send_command(AudioCommand::PlayFrom { order, row });
    core.send_command(AudioCommand::Stop);
    Ok(serde_json::json!({"ok": true, "order": order, "row": row}))
}

fn cmd_playback_set_bpm(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let bpm = get_i64!(params, "bpm").ok_or("Missing 'bpm'")? as u16;
    if bpm < 32 || bpm > 999 {
        return Err("BPM must be between 32 and 999".into());
    }
    core.send_command(AudioCommand::SetBPM(bpm));
    Ok(serde_json::json!({"ok": true, "bpm": bpm}))
}

fn cmd_playback_set_speed(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let speed = get_i64!(params, "speed").ok_or("Missing 'speed'")? as u8;
    if speed < 1 || speed > 31 {
        return Err("Speed must be between 1 and 31".into());
    }
    core.send_command(AudioCommand::SetSpeed(speed));
    Ok(serde_json::json!({"ok": true, "speed": speed}))
}

// ── Channel handlers ──

fn cmd_channel_set_panning(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let channel = get_i64!(params, "channel").ok_or("Missing 'channel'")? as usize;
    let pan = get_i64!(params, "pan").ok_or("Missing 'pan'")? as u8;
    let pan = pan.min(64);
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if channel >= arc_module.channel_panning.len() {
                return Err("Channel out of range".into());
            }
            arc_module.channel_panning[channel] = pan;
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_channel_set_volume(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let channel = get_i64!(params, "channel").ok_or("Missing 'channel'")? as usize;
    let volume = get_i64!(params, "volume").ok_or("Missing 'volume'")? as u8;
    let volume = volume.min(64);
    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            if channel >= arc_module.channel_volume.len() {
                return Err("Channel out of range".into());
            }
            arc_module.channel_volume[channel] = volume;
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_channel_set_mute(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let channel = get_i64!(params, "channel").ok_or("Missing 'channel'")? as usize;
    let muted = get_bool!(params, "muted").ok_or("Missing 'muted'")?;
    if channel < core.muted_channels.len() {
        core.muted_channels[channel] = muted;
        core.send_command(AudioCommand::SetChannelMuted { channel, muted });
    }
    Ok(serde_json::json!({"ok": true}))
}

fn cmd_channel_set_solo(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let channel = get_i64!(params, "channel").ok_or("Missing 'channel'")? as usize;
    let solo = get_bool!(params, "solo").ok_or("Missing 'solo'")?;
    if channel < core.solo_channels.len() {
        core.solo_channels[channel] = solo;
        core.send_command(AudioCommand::SetChannelSolo { channel, solo });
    }
    Ok(serde_json::json!({"ok": true}))
}

// ── Send FX handlers ──

fn cmd_sendfx_set_bus(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let bus_index = get_i64!(params, "bus_index").ok_or("Missing 'bus_index'")? as usize;
    let effect_str = get_str!(params, "effect_type").ok_or("Missing 'effect_type'")?;
    let effect_type = match effect_str.as_str() {
        "none" => SendEffectType::None,
        "delay" => SendEffectType::Delay,
        "reverb" => SendEffectType::Reverb,
        "chorus" => SendEffectType::Chorus,
        "flanger" => SendEffectType::Flanger,
        "phaser" => SendEffectType::Phaser,
        _ => return Err(format!("Unknown effect type: '{effect_str}'")),
    };

    if bus_index >= NUM_SEND_BUSES {
        return Err(format!("Bus index {bus_index} out of range (max {})", NUM_SEND_BUSES - 1));
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            arc_module.send_bus_config[bus_index] = effect_type;
            core.send_command(AudioCommand::SetSendEffectType { send_index: bus_index, effect_type });
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true, "bus_index": bus_index, "effect": effect_str}));
        }
    }
    Err("No module loaded".into())
}

fn cmd_sendfx_set_return_level(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let send_index = get_i64!(params, "send_index").ok_or("Missing 'send_index'")? as usize;
    let level = get_f64!(params, "level").ok_or("Missing 'level'")? as f32;
    let level = level.clamp(0.0, 1.0);

    if send_index >= NUM_SEND_BUSES {
        return Err(format!("Send index {send_index} out of range (max {})", NUM_SEND_BUSES - 1));
    }

    core.ensure_module_ownership();
    if let Some(ref mut module) = core.module {
        if let Some(arc_module) = Arc::get_mut(module) {
            arc_module.send_return_levels[send_index] = level;
            core.send_command(AudioCommand::SetSendReturnLevel { send_index, level });
            core.sync_module_to_audio();
            return Ok(serde_json::json!({"ok": true}));
        }
    }
    Err("No module loaded".into())
}
