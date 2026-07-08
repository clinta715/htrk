//! Order-list mutation handlers: `order.set` / `order.append` / `order.insert` /
//! `order.remove` / `order.set_entry`.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::mcp::protocol::CmdResult;

use super::common::get_i64;

/// Apply a closure to the loaded module's order list, syncing to audio on
/// success. Returns `"No module loaded"` when no module is present.
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

pub(super) fn cmd_order_set(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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

pub(super) fn cmd_order_append(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let pat = get_i64!(params, "pattern_index").ok_or("Missing 'pattern_index'")? as u8;
    order_mutate(core, |order_list| {
        order_list.push(pat);
        Ok(())
    })
}

pub(super) fn cmd_order_insert(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let pos = get_i64!(params, "position").ok_or("Missing 'position'")? as usize;
    let pat = get_i64!(params, "pattern_index").ok_or("Missing 'pattern_index'")? as u8;
    order_mutate(core, |order_list| {
        let pos = pos.min(order_list.len());
        order_list.insert(pos, pat);
        Ok(())
    })
}

pub(super) fn cmd_order_remove(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let pos = get_i64!(params, "position").ok_or("Missing 'position'")? as usize;
    order_mutate(core, |order_list| {
        if pos >= order_list.len() {
            return Err(format!("Position {pos} out of range (len={})", order_list.len()));
        }
        order_list.remove(pos);
        Ok(())
    })
}

pub(super) fn cmd_order_set_entry(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
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
