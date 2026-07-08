//! `midi.import` mutation handler — imports a Standard MIDI File into the
//! current song, splicing patterns in after the current order position.
//!
//! Mirrors the action-level `crate::actions::import_midi_with_opts` but runs
//! through the MCP mutation dispatch path. See `AGENTS.md` §26.

use std::sync::Arc;

use crate::core::HtrkCore;
use crate::mcp::protocol::CmdResult;

use super::common::{get_i64, get_str};

/// Import a `.mid`/`.midi` file into the current module.
///
/// Params:
/// - `path` (required): absolute path to the MIDI file.
/// - `rows_per_beat` (optional, default 4): quantization grid (4 = 16th notes).
/// - `target_order` (optional, default `selected_order`): order-list position
///   after which the new patterns are spliced in.
///
/// Returns the base pattern index, number of patterns added, channels used,
/// and detected BPM. Not undoable (structural merge, like `module.load`).
pub(super) fn cmd_midi_import(core: &mut HtrkCore, params: &serde_json::Value) -> CmdResult {
    let path = get_str!(params, "path").ok_or("Missing 'path'")?;
    let rows_per_beat = get_i64!(params, "rows_per_beat").unwrap_or(4).max(1).min(64) as u32;
    let target_order = get_i64!(params, "target_order")
        .map(|v| v as usize)
        .unwrap_or(core.selected_order);

    let data = std::fs::read(&path).map_err(|e| format!("Failed to read '{path}': {e}"))?;
    let imported = crate::formats::midi::import_midi(&data, rows_per_beat)
        .map_err(|e| format!("Failed to import MIDI: {e}"))?;

    if imported.patterns.is_empty() {
        return Err("MIDI file produced no patterns".into());
    }

    core.ensure_module_ownership();
    let module = core.module.as_mut().ok_or("No module loaded")?;
    let arc_module = Arc::get_mut(module).ok_or("Cannot get unique module access")?;

    let base = arc_module.patterns.len();
    let take = imported.patterns.len().min(256usize.saturating_sub(base));
    let patterns_added: Vec<u8> = (0..take)
        .map(|i| (base + i) as u8)
        .collect();
    for p in imported.patterns.into_iter().take(take) {
        arc_module.patterns.push(p);
    }

    let insert_at = (target_order + 1).min(arc_module.order_list.len());
    for (i, &pat_idx) in patterns_added.iter().enumerate() {
        arc_module.order_list.insert(insert_at + i, pat_idx);
    }

    let bpm = imported.bpm;
    let channels_used = imported.channels_used;
    let tracks_skipped = imported.tracks_skipped;

    // Honor MIDI tempo only when the module is still at the default tempo.
    if bpm > 0 && arc_module.initial_bpm == crate::sequencer::module::DEFAULT_BPM {
        arc_module.initial_bpm = bpm;
    }
    // Grow channel arrays to fit imported channels.
    if arc_module.channel_panning.len() < channels_used {
        arc_module.channel_panning.resize(channels_used, crate::sequencer::module::PANNING_CENTER);
    }
    if arc_module.channel_volume.len() < channels_used {
        arc_module.channel_volume.resize(channels_used, crate::sequencer::module::VOLUME_MAX);
    }

    core.sync_channel_fields();
    core.sync_module_to_audio();

    Ok(serde_json::json!({
        "ok": true,
        "path": path,
        "base_pattern": base,
        "patterns_added": take,
        "channels_used": channels_used,
        "bpm": bpm,
        "tracks_skipped": tracks_skipped,
    }))
}
