//! Undo / redo mutation handlers: `undo.last` / `undo.to` / `redo.last`.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::mcp::protocol::CmdResult;

use super::common::get_i64;

pub(super) fn cmd_undo_last(core: &mut HtrkCore, _params: &serde_json::Value) -> CmdResult {
    if !core.undo_manager.can_undo() {
        return Err("Nothing to undo".into());
    }
    core.ensure_module_ownership();
    let module = core.module.as_mut().ok_or("No module loaded")?;
    let arc_module = Arc::get_mut(module).ok_or("Cannot get unique module access")?;
    let depth_before = core.undo_manager.undo_depth();
    let (id, label) = core.undo_manager.undo(arc_module)
        .map_err(|e| format!("Undo failed: {e:?}"))?;
    core.sync_module_to_audio();
    Ok(serde_json::json!({
        "ok": true,
        "undone_id": id,
        "undone_label": label,
        "undo_depth_before": depth_before,
        "undo_depth_after": core.undo_manager.undo_depth(),
    }))
}

pub(super) fn cmd_undo_to(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let target = get_i64!(params, "undo_id").ok_or("Missing 'undo_id'")? as u64;
    core.ensure_module_ownership();
    let module = core.module.as_mut().ok_or("No module loaded")?;
    let arc_module = Arc::get_mut(module).ok_or("Cannot get unique module access")?;
    let undone = core.undo_manager.undo_to(target, arc_module)
        .map_err(|e| format!("Undo failed: {e:?}"))?;
    core.sync_module_to_audio();
    Ok(serde_json::json!({
        "ok": true,
        "undone_count": undone,
        "target_id": target,
    }))
}

pub(super) fn cmd_redo_last(core: &mut HtrkCore, _params: &serde_json::Value) -> CmdResult {
    if !core.undo_manager.can_redo() {
        return Err("Nothing to redo".into());
    }
    core.ensure_module_ownership();
    let module = core.module.as_mut().ok_or("No module loaded")?;
    let arc_module = Arc::get_mut(module).ok_or("Cannot get unique module access")?;
    let (id, label) = core.undo_manager.redo(arc_module)
        .map_err(|e| format!("Redo failed: {e:?}"))?;
    core.sync_module_to_audio();
    Ok(serde_json::json!({
        "ok": true,
        "redone_id": id,
        "redone_label": label,
    }))
}
