//! Transport / playback mutation handlers: `playback.play` / `playback.stop` /
//! `playback.set_position` / `playback.set_bpm` / `playback.set_speed`.

use crate::audio::commands::AudioCommand;
use crate::core::HtrkCore;
use crate::mcp::protocol::CmdResult;

use super::common::get_i64;

pub(super) fn cmd_playback_play(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    if let Some(order) = get_i64!(params, "from_order") {
        let row = get_i64!(params, "from_row").unwrap_or(0) as u16;
        core.send_command(AudioCommand::PlayFrom { order: order as u16, row });
    } else {
        core.send_command(AudioCommand::Play);
    }
    Ok(serde_json::json!({"ok": true}))
}

pub(super) fn cmd_playback_stop(core: &mut HtrkCore, _params: &serde_json::Value) -> CmdResult {
    core.send_command(AudioCommand::Stop);
    Ok(serde_json::json!({"ok": true}))
}

pub(super) fn cmd_playback_set_position(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let order = get_i64!(params, "order").ok_or("Missing 'order'")? as u16;
    let row = get_i64!(params, "row").ok_or("Missing 'row'")? as u16;
    core.send_command(AudioCommand::PlayFrom { order, row });
    core.send_command(AudioCommand::Stop);
    Ok(serde_json::json!({"ok": true, "order": order, "row": row}))
}

pub(super) fn cmd_playback_set_bpm(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let bpm = get_i64!(params, "bpm").ok_or("Missing 'bpm'")? as u16;
    if bpm < 32 || bpm > 999 {
        return Err("BPM must be between 32 and 999".into());
    }
    core.send_command(AudioCommand::SetBPM(bpm));
    Ok(serde_json::json!({"ok": true, "bpm": bpm}))
}

pub(super) fn cmd_playback_set_speed(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let speed = get_i64!(params, "speed").ok_or("Missing 'speed'")? as u8;
    if speed < 1 || speed > 31 {
        return Err("Speed must be between 1 and 31".into());
    }
    core.send_command(AudioCommand::SetSpeed(speed));
    Ok(serde_json::json!({"ok": true, "speed": speed}))
}
