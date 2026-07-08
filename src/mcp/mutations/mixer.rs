//! Mixer mutation handlers: channel panning/volume/mute/solo and send-FX bus
//! configuration / return levels.

use std::sync::Arc;

use crate::audio::commands::AudioCommand;
use crate::core::HtrkCore;
use crate::mcp::protocol::CmdResult;
use crate::sequencer::effect::{SendEffectType, NUM_SEND_BUSES};

use super::common::{get_bool, get_f64, get_i64, get_str};

// ── Channel handlers ──

pub(super) fn cmd_channel_set_panning(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_channel_set_volume(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_channel_set_mute(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let channel = get_i64!(params, "channel").ok_or("Missing 'channel'")? as usize;
    let muted = get_bool!(params, "muted").ok_or("Missing 'muted'")?;
    if channel < core.muted_channels.len() {
        core.muted_channels[channel] = muted;
        core.send_command(AudioCommand::SetChannelMuted { channel, muted });
    }
    Ok(serde_json::json!({"ok": true}))
}

pub(super) fn cmd_channel_set_solo(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let channel = get_i64!(params, "channel").ok_or("Missing 'channel'")? as usize;
    let solo = get_bool!(params, "solo").ok_or("Missing 'solo'")?;
    if channel < core.solo_channels.len() {
        core.solo_channels[channel] = solo;
        core.send_command(AudioCommand::SetChannelSolo { channel, solo });
    }
    Ok(serde_json::json!({"ok": true}))
}

// ── Send FX handlers ──

pub(super) fn cmd_sendfx_set_bus(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_sendfx_set_return_level(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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
