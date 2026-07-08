//! `module.create` / `module.load` / `module.save` mutation handlers.

use crate::core::HtrkCore;
use crate::mcp::protocol::CmdResult;
use crate::sequencer::module::{Module, PANNING_CENTER, VOLUME_MAX};

use super::common::{get_i64, get_str};

pub(super) fn cmd_module_create(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_module_load(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_module_save(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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
