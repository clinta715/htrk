// MCP tools for browsing discovered CLAP presets.
// Read-only: preset.scan is the only mutation tool that requires main-thread dispatch.

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

/// List presets for a specific plugin.
pub fn cmd_preset_list_by_plugin(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let plugin_path = params
        .get("plugin_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'plugin_path'")?;
    let plugin_id = params
        .get("plugin_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'plugin_id'")?;

    let lib = ctx
        .preset_library
        .read()
        .map_err(|e| format!("Preset library lock poisoned: {e}"))?;
    let presets = lib.list_plugin_presets(plugin_path, plugin_id);
    let result: Vec<serde_json::Value> = presets
        .iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    Ok(json!({ "total": result.len(), "presets": result }))
}

/// Trigger a rescan of presets across all discovered plugins.
/// Returns summary statistics.
pub fn cmd_preset_scan(_params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
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
            "presets_found": 0,
            "plugins_scanned": 0,
            "errors": 0,
            "message": "No plugins discovered yet — run plugin.scan first"
        }));
    }

    let (entries, errors) =
        crate::audio::plugins::preset_discovery::scan_plugins_for_presets(&plugin_paths);
    let count = entries.len();

    // Write back to the library (requires write lock)
    {
        let mut lib = ctx
            .preset_library
            .write()
            .map_err(|e| format!("Preset library lock poisoned: {e}"))?;
        lib.clear();
        lib.add_presets(entries);
        lib.set_last_scan_time(std::time::SystemTime::now());
    }

    Ok(json!({
        "presets_found": count,
        "plugins_scanned": plugin_paths.len(),
        "errors": errors.len(),
        "error_details": errors.iter().map(|(p, e)| json!({
            "plugin": p.display().to_string(),
            "error": e
        })).collect::<Vec<_>>()
    }))
}

/// Get preset library statistics and status.
pub fn cmd_preset_status(_params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
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
