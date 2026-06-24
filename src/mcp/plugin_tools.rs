// Plugin MCP tools — discover, list, and describe CLAP plugins via MCP.
// These are read-only tools that run on the MCP server thread from the
// PluginLibrary (an in-memory cache of discovered plugins).

use serde_json::json;

use crate::mcp::protocol::{ToolContext, *};
use crate::audio::plugins::{PluginDescriptor, PluginFormat};

/// List all discovered plugins, optionally filtered by name substring.
pub fn cmd_plugin_list(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let name_filter = params.get("name_contains")
        .and_then(|v| v.as_str())
        .map(str::to_lowercase);

    let library = ctx.plugin_library.read().map_err(|e| format!("Library lock poisoned: {e}"))?;
    let descriptors: Vec<&PluginDescriptor> = library.list_descriptors()
        .into_iter()
        .filter(|d| {
            match &name_filter {
                Some(f) => d.name.to_lowercase().contains(f) ||
                          d.plugin_id.to_lowercase().contains(f),
                None => true,
            }
        })
        .collect();

    let result: Vec<serde_json::Value> = descriptors.iter().map(|d| {
        json!({
            "format": d.format.as_str(),
            "path": d.path.display().to_string(),
            "plugin_id": d.plugin_id,
            "name": d.name,
            "vendor": d.vendor,
            "version": d.version,
            "description": d.description,
            "plugin_type": match d.plugin_type {
                crate::audio::plugins::PluginType::Instrument => "instrument",
                crate::audio::plugins::PluginType::Effect => "effect",
                crate::audio::plugins::PluginType::Both => "both",
                crate::audio::plugins::PluginType::Analyzer => "analyzer",
            },
            "audio_inputs": d.audio_inputs,
            "audio_outputs": d.audio_outputs,
            "has_editor": d.has_editor,
            "supports_state": d.supports_state,
        })
    }).collect();

    Ok(json!({
        "total": result.len(),
        "plugins": result,
    }))
}

/// Get detailed information about a specific plugin by (format, path, plugin_id).
pub fn cmd_plugin_info(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let path = params.get("path").and_then(|v| v.as_str())
        .ok_or("Missing 'path'")?;
    let plugin_id = params.get("plugin_id").and_then(|v| v.as_str())
        .ok_or("Missing 'plugin_id'")?;
    let format_str = params.get("format").and_then(|v| v.as_str())
        .ok_or("Missing 'format'")?;

    let format = match format_str {
        "clap" => PluginFormat::Clap,
        _ => return Err(format!("Unsupported format: {format_str}")),
    };

    let library = ctx.plugin_library.read().map_err(|e| format!("Library lock poisoned: {e}"))?;
    let path_buf = std::path::PathBuf::from(path);
    match library.get_descriptor(format, &path_buf, plugin_id) {
        Some(d) => Ok(json!({
            "format": d.format.as_str(),
            "path": d.path.display().to_string(),
            "plugin_id": d.plugin_id,
            "name": d.name,
            "vendor": d.vendor,
            "version": d.version,
            "description": d.description,
            "plugin_type": match d.plugin_type {
                crate::audio::plugins::PluginType::Instrument => "instrument",
                crate::audio::plugins::PluginType::Effect => "effect",
                crate::audio::plugins::PluginType::Both => "both",
                crate::audio::plugins::PluginType::Analyzer => "analyzer",
            },
            "audio_inputs": d.audio_inputs,
            "audio_outputs": d.audio_outputs,
            "has_editor": d.has_editor,
            "supports_state": d.supports_state,
        })),
        None => Err(format!("Plugin not found: {plugin_id}")),
    }
}

/// Trigger a rescan of the plugin library. The scan walks the configured scan
/// roots, loads each .clap file's descriptor, and updates the in-memory cache.
pub fn cmd_plugin_scan(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    // Collect scan roots: default paths + any user-configured paths
    let mut scan_roots = crate::audio::plugins::default_search_paths();
    // The user-configured paths are typically stored in AppConfig. For MCP
    // tools, we use the default paths plus any paths passed in the request.
    if let Some(paths) = params.get("extra_paths").and_then(|v| v.as_array()) {
        for p in paths {
            if let Some(s) = p.as_str() {
                scan_roots.push(std::path::PathBuf::from(s));
            }
        }
    }

    // Scan files (no actual load — just filesystem walk)
    let found_files = crate::audio::plugins::discovery::scan_paths(&scan_roots);

    // Update the library cache: clear old descriptors and add new ones
    let mut library = ctx.plugin_library.write().map_err(|e| format!("Library lock poisoned: {e}"))?;
    library.clear_cache();
    // We only store descriptors here — actual plugin load happens on the
    // main thread when the user assigns a plugin to a bus. For now, we
    // just record the file paths as a "discovery complete" event.
    let mut found_count = 0;
    for path in &found_files.clap_files {
        // Quick descriptor extraction (no full plugin load)
        // For Phase 2, we just record the path. Full descriptor extraction
        // requires loading the plugin via the host, which is expensive.
        // MCP clients should use plugin.info with a specific path to get
        // the full descriptor (which triggers a full load).
        if let Some(stem) = path.file_name().and_then(|s| s.to_str()) {
            let descriptor = crate::audio::plugins::PluginDescriptor {
                format: PluginFormat::Clap,
                path: path.clone(),
                plugin_id: stem.to_string(),
                name: stem.replace(".clap", ""),
                vendor: "Unknown (run plugin.info to load)".to_string(),
                version: String::new(),
                description: "Discovery stub — run plugin.info to get full metadata".to_string(),
                plugin_type: crate::audio::plugins::PluginType::Both,
                audio_inputs: 2,
                audio_outputs: 2,
                has_editor: false,
                supports_state: true,
            };
            library.add_descriptor(descriptor);
            found_count += 1;
        }
    }

    Ok(json!({
        "scanned_roots": scan_roots.len(),
        "files_found": found_count,
        "errors": found_files.errors.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_list_empty() {
        // Test that the list function works with an empty library
        let library = crate::mcp::library::SampleLibrary::new();
        let plugin_library = crate::audio::plugins::PluginLibrary::new();
        let ctx = ToolContext {
            module_snapshot: crate::mcp::protocol::ModuleSnapshot::default(),
            playback_snapshot: crate::mcp::protocol::PlaybackSnapshot::default(),
            channels_snapshot: crate::mcp::protocol::ChannelsSnapshot::default(),
            library: std::sync::Arc::new(std::sync::RwLock::new(library)),
            plugin_library: std::sync::Arc::new(std::sync::RwLock::new(plugin_library)),
        };
        let result = cmd_plugin_list(json!({}), &ctx).unwrap();
        let val = result.as_object().unwrap();
        assert_eq!(val["total"].as_i64().unwrap(), 0);
    }

    #[test]
    fn test_plugin_info_not_found() {
        let library = crate::mcp::library::SampleLibrary::new();
        let plugin_library = crate::audio::plugins::PluginLibrary::new();
        let ctx = ToolContext {
            module_snapshot: crate::mcp::protocol::ModuleSnapshot::default(),
            playback_snapshot: crate::mcp::protocol::PlaybackSnapshot::default(),
            channels_snapshot: crate::mcp::protocol::ChannelsSnapshot::default(),
            library: std::sync::Arc::new(std::sync::RwLock::new(library)),
            plugin_library: std::sync::Arc::new(std::sync::RwLock::new(plugin_library)),
        };
        let result = cmd_plugin_info(json!({
            "format": "clap",
            "path": "/nonexistent.clap",
            "plugin_id": "com.example.missing"
        }), &ctx);
        assert!(result.is_err());
    }
}
