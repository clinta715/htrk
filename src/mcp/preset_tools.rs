// MCP tools for browsing discovered CLAP presets.
// All tools run on the MCP thread. preset.scan writes to preset_library
// via RwLock (same pattern as plugin.scan). See AGENTS.md §21.

use serde_json::json;

use crate::mcp::protocol::*;

/// List all discovered presets, with optional filtering and pagination.
pub fn cmd_preset_list(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let filter_plugin = params
        .get("filter_plugin_id")
        .and_then(|v| v.as_str());
    let page = params.get("page").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let page_size = params
        .get("page_size")
        .and_then(|v| v.as_i64())
        .unwrap_or(50) as usize;

    let lib = ctx
        .preset_library
        .read()
        .map_err(|e| format!("Preset library lock poisoned: {e}"))?;
    let results = lib.search(query, filter_plugin, page, page_size);
    serde_json::to_value(&results).map_err(|e| format!("Serialization error: {e}"))
}

/// Get a specific preset by its cache key.
pub fn cmd_preset_info(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let key = params
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'key'")?;

    let lib = ctx
        .preset_library
        .read()
        .map_err(|e| format!("Preset library lock poisoned: {e}"))?;
    match lib.get_preset(key) {
        Some(entry) => serde_json::to_value(entry).map_err(|e| format!("Serialization error: {e}")),
        None => Err(format!("Preset not found: {key}")),
    }
}

/// List presets for a specific plugin, with pagination.
pub fn cmd_preset_list_by_plugin(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let plugin_path = params
        .get("plugin_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'plugin_path'")?;
    let plugin_id = params
        .get("plugin_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'plugin_id'")?;
    let page = params.get("page").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let page_size = params
        .get("page_size")
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
        .max(1) as usize;

    let lib = ctx
        .preset_library
        .read()
        .map_err(|e| format!("Preset library lock poisoned: {e}"))?;
    let mut presets: Vec<_> = lib.list_plugin_presets(plugin_path, plugin_id);
    let total = presets.len();
    let total_pages = (total + page_size - 1) / page_size;
    let start = page * page_size;
    let page_presets: Vec<_> = presets
        .drain(start..)
        .take(page_size)
        .collect();
    let result: Vec<serde_json::Value> = page_presets
        .iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    Ok(json!({
        "total": total,
        "page": page,
        "page_size": page_size,
        "total_pages": total_pages,
        "presets": result
    }))
}

/// Trigger a rescan of presets across all discovered plugins.
///
/// The scan runs on a background thread so it never blocks the MCP server
/// thread (scanning thousands of presets can take ~30s). Returns immediately
/// with a "scan_started" status; poll `preset.status` to check progress.
pub fn cmd_preset_scan(_params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    if ctx
        .preset_scan_in_progress
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return Ok(json!({
            "status": "already_scanning",
            "message": "A preset scan is already in progress"
        }));
    }

    // Collect all discovered plugin .clap paths
    let plugin_paths: Vec<std::path::PathBuf> = {
        let plib = ctx
            .plugin_library
            .read()
            .map_err(|e| format!("Plugin library lock poisoned: {e}"))?;
        plib.list_descriptors()
            .iter()
            .map(|d| d.path.clone())
            .collect()
    };

    if plugin_paths.is_empty() {
        return Ok(json!({
            "status": "no_plugins",
            "message": "No plugins discovered yet — run plugin.scan first"
        }));
    }

    ctx.preset_scan_in_progress
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let preset_lib = ctx.preset_library.clone();
    let scan_flag = ctx.preset_scan_in_progress.clone();
    let plugin_count = plugin_paths.len();

    std::thread::Builder::new()
        .name("htrk-preset-scan".into())
        .spawn(move || {
            let (entries, errors) =
                crate::audio::plugins::preset_discovery::scan_plugins_for_presets(&plugin_paths);

            if let Ok(mut lib) = preset_lib.write() {
                lib.clear();
                lib.add_presets(entries);
                lib.set_last_scan_time(std::time::SystemTime::now());
            }

            eprintln!(
                "[preset_discovery] Background scan done: {} error(s) from {} plugin(s)",
                errors.len(),
                plugin_count
            );

            scan_flag.store(false, std::sync::atomic::Ordering::Relaxed);
        })
        .map_err(|e| format!("Failed to spawn scan thread: {e}"))?;

    Ok(json!({
        "status": "scan_started",
        "plugins_to_scan": plugin_count,
        "message": "Scan running in background. Use preset.status to check progress."
    }))
}

/// Get preset library statistics and status.
pub fn cmd_preset_status(_params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let scanning = ctx
        .preset_scan_in_progress
        .load(std::sync::atomic::Ordering::Relaxed);
    let lib = ctx
        .preset_library
        .read()
        .map_err(|e| format!("Preset library lock poisoned: {e}"))?;
    let total = lib.preset_count();
    let scan_time = lib.last_scan_time().map(|t| {
        let secs = t
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format_iso8601(secs)
    });

    // Count unique plugins with presets
    let mut plugin_names: Vec<String> = lib
        .list_presets()
        .iter()
        .map(|e| format!("{} ({})", e.plugin_name, e.plugin_id))
        .collect();
    plugin_names.sort();
    plugin_names.dedup();

    Ok(json!({
        "total_presets": total,
        "unique_plugins": plugin_names.len(),
        "last_scan": scan_time,
        "plugins": plugin_names,
        "scanning": scanning,
    }))
}

fn format_iso8601(unix_secs: u64) -> String {
    let secs = unix_secs as i64;
    let days = secs / 86400;
    let time_secs = (secs % 86400) as u32;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let mut year = 1970i64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md {
            month = i + 1;
            break;
        }
        remaining -= md;
    }
    if month == 0 {
        month = 12;
    }
    let day = remaining + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::RwLock;
    use crate::audio::plugins::PresetLibrary;
    use crate::audio::plugins::PluginLibrary;
    use crate::mcp::library::SampleLibrary;
    use crate::mcp::protocol::*;

    fn test_ctx() -> ToolContext {
        ToolContext {
            module_snapshot: ModuleSnapshot::default(),
            playback_snapshot: PlaybackSnapshot::default(),
            channels_snapshot: ChannelsSnapshot::default(),
            library: Arc::new(RwLock::new(SampleLibrary::new())),
            plugin_library: Arc::new(RwLock::new(PluginLibrary::new())),
            preset_library: Arc::new(RwLock::new(PresetLibrary::new())),
            preset_scan_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    fn add_test_preset(lib: &PresetLibrary) {
        // We need to modify the library — use the write lock
        // This is tested indirectly via list/info tests
    }

    #[test]
    fn test_preset_list_empty() {
        let ctx = test_ctx();
        let result = cmd_preset_list(json!({}), &ctx).unwrap();
        let val = result.as_object().unwrap();
        assert_eq!(val["total_results"].as_i64().unwrap(), 0);
    }

    #[test]
    fn test_preset_status_empty() {
        let ctx = test_ctx();
        let result = cmd_preset_status(json!({}), &ctx).unwrap();
        assert_eq!(result["total_presets"].as_i64().unwrap(), 0);
    }
}
