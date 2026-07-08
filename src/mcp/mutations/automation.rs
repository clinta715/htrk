//! Automation mutation handlers: create / remove / add_point / clear.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::mcp::protocol::CmdResult;
use crate::sequencer::automation::{AutomationTrack, AutomationPoint, AutomationTarget, InterpolationMode};

use super::common::{get_f64, get_i64, get_str};

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

pub(super) fn cmd_automation_create(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_automation_remove(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_automation_add_point(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_automation_clear(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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
