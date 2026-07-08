//! Envelope mutation handlers: set / add_point / remove_point / generate.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::edit::{AddEnvelopePointCommand, RemoveEnvelopePointCommand, SetEnvelopePointsCommand, EnvelopeType};
use crate::mcp::protocol::CmdResult;
use crate::sequencer::instrument::EnvelopePoint;

use super::common::{get_f64, get_i64, get_str};

fn parse_envelope_type(s: &str) -> Result<EnvelopeType, String> {
    match s {
        "volume" => Ok(EnvelopeType::Volume),
        "panning" => Ok(EnvelopeType::Panning),
        "pitch" => Ok(EnvelopeType::Pitch),
        "filter" => Ok(EnvelopeType::Filter),
        _ => Err(format!("Unknown envelope type: '{s}'")),
    }
}

pub(super) fn cmd_envelope_set(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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
            let old_env = inst.envelope(et).clone();
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
                    let env = inst2.instruments[inst_idx].envelope_mut(et);
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

pub(super) fn cmd_envelope_add_point(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_envelope_remove_point(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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
                let env = inst.envelope(et);
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

pub(super) fn cmd_envelope_generate(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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
            let old_env = inst.envelope(et).clone();
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
