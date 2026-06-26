use serde_json::json;

use crate::mcp::library::DirFilter;
use crate::mcp::protocol::*;

/// Tools that require mutation access (routed through main-thread command queue).
pub const MUTATION_TOOLS: &[&str] = &[
    "module.create", "module.load", "module.save",
    "order.set", "order.append", "order.insert", "order.remove", "order.set_entry",
    "pattern.ensure",
    "cell.set", "cell.set_batch",
    "pattern.fill", "pattern.clear", "pattern.transpose", "pattern.interpolate",
    "instrument.create", "instrument.remove", "instrument.set_property",
    "instrument.map_note", "instrument.map_range",
    "sample.load", "sample.remove", "sample.set_property",
    "envelope.set", "envelope.add_point", "envelope.remove_point", "envelope.generate",
    "automation.create", "automation.remove", "automation.add_point", "automation.clear",
    "playback.play", "playback.stop", "playback.set_position",
    "playback.set_bpm", "playback.set_speed",
    "channel.set_panning", "channel.set_volume", "channel.set_mute", "channel.set_solo",
    "sendfx.set_bus", "sendfx.set_return_level",
    "sample_library.import",
];

pub fn list_tools() -> Vec<ToolDefinition> {
    vec![
        // ── Module lifecycle ──
        tool_def("module.create", "Create a new song", json!({
            "type": "object",
            "properties": {
                "name":    {"type": "string", "description": "Song name"},
                "channels":{"type": "integer", "description": "Number of channels (default 4)", "default": 4},
                "bpm":     {"type": "integer", "description": "Beats per minute (default 125)", "default": 125},
                "speed":   {"type": "integer", "description": "Ticks per row (default 6)", "default": 6},
            },
            "required": ["name"]
        })),
        tool_def("module.load", "Load an existing song file", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to .htk or tracker file"}
            },
            "required": ["path"]
        })),
        tool_def("module.save", "Save the current song", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Save path (default: current file path)"}
            }
        })),
        tool_def("module.info", "Get module metadata", json!({
            "type": "object",
            "properties": {}
        })),

        // ── Order list ──
        tool_def("order.get", "Get the full order list", json!({
            "type": "object", "properties": {}
        })),
        tool_def("order.set", "Replace the entire order list", json!({
            "type": "object",
            "properties": {
                "entries": {"type": "array", "items": {"type": "integer"}, "description": "Array of pattern indices"}
            },
            "required": ["entries"]
        })),
        tool_def("order.append", "Append a pattern to the end of the order list", json!({
            "type": "object",
            "properties": {
                "pattern_index": {"type": "integer", "description": "Pattern index to append"}
            },
            "required": ["pattern_index"]
        })),
        tool_def("order.insert", "Insert a pattern at the given position", json!({
            "type": "object",
            "properties": {
                "position": {"type": "integer", "description": "Insert position"},
                "pattern_index": {"type": "integer", "description": "Pattern index to insert"}
            },
            "required": ["position", "pattern_index"]
        })),
        tool_def("order.remove", "Remove the order entry at the given position", json!({
            "type": "object",
            "properties": {
                "position": {"type": "integer", "description": "Position to remove"}
            },
            "required": ["position"]
        })),
        tool_def("order.set_entry", "Change a specific order entry", json!({
            "type": "object",
            "properties": {
                "position": {"type": "integer", "description": "Position in order list"},
                "pattern_index": {"type": "integer", "description": "New pattern index"}
            },
            "required": ["position", "pattern_index"]
        })),

        // ── Pattern editing ──
        tool_def("pattern.ensure", "Create a pattern if it doesn't exist", json!({
            "type": "object",
            "properties": {
                "index": {"type": "integer", "description": "Pattern index"},
                "num_rows": {"type": "integer", "description": "Number of rows (default 64)"}
            },
            "required": ["index"]
        })),
        tool_def("cell.set", "Set a single cell in the pattern", json!({
            "type": "object",
            "properties": {
                "order":   {"type": "integer", "description": "Position in the order list"},
                "row":     {"type": "integer", "description": "Row index (0-based)"},
                "channel": {"type": "integer", "description": "Channel index (0-based)"},
                "note":    {"type": "string", "description": "Note name: C-5, D#4, A--2, ... or OFF, CUT, FADE, ... to clear"},
                "instrument": {"type": "integer", "description": "Instrument number (0 = clear)"},
                "volume":  {"type": "integer", "description": "Volume 0-64"},
                "volume_effect": {"$ref": "#/definitions/Effect"},
                "effect":  {"$ref": "#/definitions/Effect"}
            },
            "required": ["order", "row", "channel"]
        })),
        tool_def("cell.get", "Get a single cell value", json!({
            "type": "object",
            "properties": {
                "order":   {"type": "integer", "description": "Position in the order list"},
                "row":     {"type": "integer", "description": "Row index (0-based)"},
                "channel": {"type": "integer", "description": "Channel index (0-based)"}
            },
            "required": ["order", "row", "channel"]
        })),
        tool_def("cell.set_batch", "Set multiple cells in one call", json!({
            "type": "object",
            "properties": {
                "entries": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "order":   {"type": "integer"},
                            "row":     {"type": "integer"},
                            "channel": {"type": "integer"},
                            "note":    {"type": "string"},
                            "instrument": {"type": "integer"},
                            "volume":  {"type": "integer"},
                            "volume_effect": {"$ref": "#/definitions/Effect"},
                            "effect":  {"$ref": "#/definitions/Effect"}
                        },
                        "required": ["order", "row", "channel"]
                    }
                }
            },
            "required": ["entries"]
        })),
        tool_def("pattern.fill", "Fill a rectangular region of a pattern with values", json!({
            "type": "object",
            "properties": {
                "index":         {"type": "integer", "description": "Pattern index"},
                "row_start":     {"type": "integer"},
                "row_end":       {"type": "integer"},
                "channel_start": {"type": "integer", "default": 0},
                "channel_end":   {"type": "integer"},
                "note":          {"type": "string"},
                "instrument":    {"type": "integer"},
                "volume":        {"type": "integer"},
                "volume_effect": {"$ref": "#/definitions/Effect"},
                "effect":        {"$ref": "#/definitions/Effect"}
            },
            "required": ["index", "row_start", "row_end"]
        })),
        tool_def("pattern.clear", "Clear a pattern or a region within it", json!({
            "type": "object",
            "properties": {
                "index":         {"type": "integer"},
                "row_start":     {"type": "integer"},
                "row_end":       {"type": "integer"},
                "channel_start": {"type": "integer"},
                "channel_end":   {"type": "integer"}
            },
            "required": ["index"]
        })),
        tool_def("pattern.transpose", "Transpose notes in a pattern by semitones", json!({
            "type": "object",
            "properties": {
                "index":     {"type": "integer"},
                "semitones": {"type": "integer"},
                "channel":   {"type": "integer"},
                "row_start": {"type": "integer"},
                "row_end":   {"type": "integer"}
            },
            "required": ["index", "semitones"]
        })),
        tool_def("pattern.interpolate", "Linearly interpolate values across a range of rows", json!({
            "type": "object",
            "properties": {
                "index":    {"type": "integer"},
                "channel":  {"type": "integer"},
                "row_start":{"type": "integer"},
                "row_end":  {"type": "integer"},
                "target":   {"type": "string", "enum": ["volume", "effect_value"], "description": "Which field to interpolate"}
            },
            "required": ["index", "channel", "row_start", "row_end", "target"]
        })),

        // ── Instruments ──
        tool_def("instrument.create", "Create a new instrument", json!({
            "type": "object",
            "properties": {
                "name":         {"type": "string", "default": ""},
                "base_sample":  {"type": "integer", "description": "Optional sample index to map to all notes"}
            }
        })),
        tool_def("instrument.remove", "Remove an instrument", json!({
            "type": "object",
            "properties": {
                "index": {"type": "integer"}
            },
            "required": ["index"]
        })),
        tool_def("instrument.set_property", "Set an instrument property", json!({
            "type": "object",
            "properties": {
                "index":    {"type": "integer"},
                "property": {"type": "string", "description": "Property name: name, fade_out, global_volume, filter_cutoff, filter_resonance, nna, dct, dca, random_volume, random_panning, etc."},
                "value":    {"description": "Property value (type depends on property)"}
            },
            "required": ["index", "property", "value"]
        })),
        tool_def("instrument.map_note", "Map a note to a sample", json!({
            "type": "object",
            "properties": {
                "index":         {"type": "integer", "description": "Instrument index"},
                "note":          {"type": "string", "description": "Note name (e.g. C-5, D#3, or drum key)"},
                "sample_index":  {"type": "integer", "description": "Sample index to map"},
                "transpose_note":{"type": "string", "description": "Optional transposition note"}
            },
            "required": ["index", "note", "sample_index"]
        })),
        tool_def("instrument.map_range", "Map a range of notes to the same sample", json!({
            "type": "object",
            "properties": {
                "index":        {"type": "integer"},
                "note_start":   {"type": "string"},
                "note_end":     {"type": "string"},
                "sample_index": {"type": "integer"}
            },
            "required": ["index", "note_start", "note_end", "sample_index"]
        })),

        // ── Samples ──
        tool_def("sample.load", "Import an audio file as a sample", json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to WAV or other audio file"},
                "name": {"type": "string", "description": "Optional display name"}
            },
            "required": ["path"]
        })),
        tool_def("sample.remove", "Remove a sample", json!({
            "type": "object",
            "properties": {
                "index": {"type": "integer"}
            },
            "required": ["index"]
        })),
        tool_def("sample.set_property", "Set a sample property", json!({
            "type": "object",
            "properties": {
                "index":    {"type": "integer"},
                "property": {"type": "string", "description": "Property: name, default_volume, default_panning, global_volume, relative_note, fine_tune, loop_type, loop_start, loop_end, sample_rate"},
                "value":    {"description": "Property value"}
            },
            "required": ["index", "property", "value"]
        })),

        // ── Envelopes ──
        tool_def("envelope.set", "Set the full envelope for an instrument", json!({
            "type": "object",
            "properties": {
                "instrument":   {"type": "integer"},
                "type":         {"type": "string", "enum": ["volume", "panning", "pitch", "filter"]},
                "points":       {"type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "tick":  {"type": "integer"},
                        "value": {"type": "integer", "description": "0-64"}
                    },
                    "required": ["tick", "value"]
                }},
                "sustain_point": {"type": "integer"},
                "loop_start":    {"type": "integer"},
                "loop_end":      {"type": "integer"},
                "enabled":       {"type": "boolean"},
                "sustain":       {"type": "boolean"},
                "loop":          {"type": "boolean"}
            },
            "required": ["instrument", "type", "points"]
        })),
        tool_def("envelope.add_point", "Add an envelope point", json!({
            "type": "object",
            "properties": {
                "instrument": {"type": "integer"},
                "type":       {"type": "string", "enum": ["volume", "panning", "pitch", "filter"]},
                "tick":       {"type": "integer"},
                "value":      {"type": "integer", "description": "0-64"}
            },
            "required": ["instrument", "type", "tick", "value"]
        })),
        tool_def("envelope.remove_point", "Remove an envelope point by index", json!({
            "type": "object",
            "properties": {
                "instrument":  {"type": "integer"},
                "type":        {"type": "string", "enum": ["volume", "panning", "pitch", "filter"]},
                "point_index": {"type": "integer"}
            },
            "required": ["instrument", "type", "point_index"]
        })),
        tool_def("envelope.generate", "Generate envelope points from a waveform shape", json!({
            "type": "object",
            "properties": {
                "instrument": {"type": "integer"},
                "type":       {"type": "string", "enum": ["volume", "panning", "pitch", "filter"]},
                "shape":      {"type": "string", "enum": ["sine", "square", "triangle", "saw_up", "saw_down", "pulse", "random"]},
                "num_points": {"type": "integer", "description": "Number of points to generate"},
                "tick_span":  {"type": "integer", "description": "Total tick duration"},
                "amplitude":  {"type": "number", "description": "Peak amplitude 0.0-1.0"},
                "offset":     {"type": "number", "description": "Vertical offset 0.0-1.0 (default 0.5)"}
            },
            "required": ["instrument", "type", "shape", "num_points", "tick_span", "amplitude"]
        })),

        // ── Automation ──
        tool_def("automation.create", "Create a new automation track", json!({
            "type": "object",
            "properties": {
                "target": {"type": "object", "description": "Automation target: {type: string, channel: int}",
                    "properties": {
                        "type":    {"type": "string"},
                        "channel": {"type": "integer"}
                    },
                    "required": ["type"]
                },
                "interpolation": {"type": "string", "enum": ["hold", "linear", "smooth", "exponential"], "default": "linear"}
            },
            "required": ["target"]
        })),
        tool_def("automation.remove", "Remove an automation track", json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer"}
            },
            "required": ["track_id"]
        })),
        tool_def("automation.add_point", "Add a point to an automation track", json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer"},
                "order":    {"type": "integer"},
                "row":      {"type": "integer"},
                "value":    {"type": "number", "description": "Normalized value 0.0-1.0"},
                "interp":   {"type": "string", "enum": ["hold", "linear", "smooth", "exponential"]}
            },
            "required": ["track_id", "order", "row", "value"]
        })),
        tool_def("automation.clear", "Clear all points from an automation track", json!({
            "type": "object",
            "properties": {
                "track_id": {"type": "integer"}
            },
            "required": ["track_id"]
        })),

        // ── Playback ──
        tool_def("playback.play", "Start playing from the current position", json!({
            "type": "object",
            "properties": {
                "from_order": {"type": "integer"},
                "from_row":   {"type": "integer"}
            }
        })),
        tool_def("playback.stop", "Stop playback", json!({
            "type": "object", "properties": {}
        })),
        tool_def("playback.set_position", "Jump to a specific position", json!({
            "type": "object",
            "properties": {
                "order": {"type": "integer"},
                "row":   {"type": "integer"}
            },
            "required": ["order", "row"]
        })),
        tool_def("playback.set_bpm", "Set the tempo in BPM", json!({
            "type": "object",
            "properties": {
                "bpm": {"type": "integer", "description": "Beats per minute"}
            },
            "required": ["bpm"]
        })),
        tool_def("playback.set_speed", "Set ticks per row", json!({
            "type": "object",
            "properties": {
                "speed": {"type": "integer", "description": "Ticks per row (1-31)"}
            },
            "required": ["speed"]
        })),
        tool_def("playback.state", "Get current playback state", json!({
            "type": "object", "properties": {}
        })),

        // ── Channel config ──
        tool_def("channel.set_panning", "Set panning for a channel", json!({
            "type": "object",
            "properties": {
                "channel": {"type": "integer"},
                "pan":     {"type": "integer", "description": "Panning -64 to 64 (0 = center)"}
            },
            "required": ["channel", "pan"]
        })),
        tool_def("channel.set_volume", "Set volume for a channel", json!({
            "type": "object",
            "properties": {
                "channel": {"type": "integer"},
                "volume":  {"type": "integer", "description": "Volume 0-64"}
            },
            "required": ["channel", "volume"]
        })),
        tool_def("channel.set_mute", "Mute or unmute a channel", json!({
            "type": "object",
            "properties": {
                "channel": {"type": "integer"},
                "muted":   {"type": "boolean"}
            },
            "required": ["channel", "muted"]
        })),
        tool_def("channel.set_solo", "Solo or unsolo a channel", json!({
            "type": "object",
            "properties": {
                "channel": {"type": "integer"},
                "solo":    {"type": "boolean"}
            },
            "required": ["channel", "solo"]
        })),

        // ── Send FX ──
        tool_def("sendfx.set_bus", "Configure a send effect bus", json!({
            "type": "object",
            "properties": {
                "bus_index":   {"type": "integer", "description": "Bus index 0-3"},
                "effect_type": {"type": "string", "enum": ["none", "delay", "reverb", "chorus", "flanger", "phaser"]}
            },
            "required": ["bus_index", "effect_type"]
        })),
        tool_def("sendfx.set_return_level", "Set the return level for a send bus", json!({
            "type": "object",
            "properties": {
                "send_index": {"type": "integer", "description": "Send index 0-3"},
                "level":      {"type": "number", "description": "Return level 0.0-1.0"}
            },
            "required": ["send_index", "level"]
        })),

        // ── Sample Library (read-only) ──
        tool_def("sample_library.configure", "Configure sample library root directories", json!({
            "type": "object",
            "properties": {
                "roots": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "List of root directory paths for the sample library"
                }
            },
            "required": ["roots"]
        })),
        tool_def("sample_library.list_dir", "List a directory in the sample library", json!({
            "type": "object",
            "properties": {
                "path":      {"type": "string", "description": "Directory path to list"},
                "page":      {"type": "integer", "description": "Page number (0-based)", "default": 0},
                "page_size": {"type": "integer", "description": "Items per page (default 50)", "default": 50},
                "filter": {
                    "type": "object",
                    "description": "Optional filter",
                    "properties": {
                        "name_contains":  {"type": "string", "description": "Substring match on filename"},
                        "min_duration":   {"type": "number", "description": "Minimum duration in seconds"},
                        "max_duration":   {"type": "number", "description": "Maximum duration in seconds"},
                        "category":       {"type": "string", "description": "Substring match on heuristically detected category"},
                        "min_bpm":        {"type": "number", "description": "Minimum BPM (matches when entry.bpm is in range)"},
                        "max_bpm":        {"type": "number", "description": "Maximum BPM (matches when entry.bpm is in range)"},
                        "key":            {"type": "string", "description": "Substring match on detected musical key (e.g. 'Cmaj', 'Amin')"},
                        "tag":            {"type": "string", "description": "Substring match on any detected tag (e.g. 'kick', 'wet')"},
                        "channels_filter":{"type": "integer", "description": "Exact channel count: 1 = mono only, 2 = stereo only"}
                    }
                }
            },
            "required": ["path"]
        })),
        tool_def("sample_library.search", "Search indexed samples across library roots", json!({
            "type": "object",
            "properties": {
                "query":   {"type": "string", "description": "Search query (filename or category substring)"},
                "scope_roots": {
                    "type": "array", "items": {"type": "string"},
                    "description": "Restrict search to these roots (default: all)"
                },
                "page":      {"type": "integer", "description": "Page number (0-based)", "default": 0},
                "page_size": {"type": "integer", "description": "Items per page (default 50)", "default": 50}
            },
            "required": ["query"]
        })),

        // ── Plugin tools (read-only) ──
        tool_def("plugin.scan", "Scan filesystem for CLAP plugins in default and configured scan paths", json!({
            "type": "object",
            "properties": {
                "extra_paths": {
                    "type": "array", "items": {"type": "string"},
                    "description": "Additional paths to scan beyond the system defaults"
                }
            }
        })),
        tool_def("plugin.list", "List all discovered plugins, optionally filtered by name substring", json!({
            "type": "object",
            "properties": {
                "name_contains": {"type": "string", "description": "Filter by substring in name or id (case-insensitive)"}
            }
        })),
        tool_def("plugin.info", "Get detailed information about a specific plugin (triggers a full plugin load)", json!({
            "type": "object",
            "properties": {
                "format": {"type": "string", "enum": ["clap"], "description": "Plugin format"},
                "path":   {"type": "string", "description": "Path to the plugin bundle or DLL"},
                "plugin_id": {"type": "string", "description": "Plugin stable id (CLAP id)"}
            },
            "required": ["format", "path", "plugin_id"]
        })),

        // ── Preset discovery tools ──
        tool_def("preset.scan", "Rescan all discovered plugins for presets via CLAP preset-discovery", json!({
            "type": "object",
            "properties": {}
        })),
        tool_def("preset.list", "List cached presets with optional search, plugin filter, and pagination", json!({
            "type": "object",
            "properties": {
                "query":           {"type": "string", "description": "Search by name, feature, plugin, or description"},
                "filter_plugin_id":{"type": "string", "description": "Restrict to a specific plugin_id"},
                "page":            {"type": "integer", "description": "Page number (0-based)", "default": 0},
                "page_size":       {"type": "integer", "description": "Items per page (default 50)", "default": 50}
            }
        })),
        tool_def("preset.info", "Get a specific preset by its cache key", json!({
            "type": "object",
            "properties": {
                "key": {"type": "string", "description": "Preset cache key"}
            },
            "required": ["key"]
        })),
        tool_def("preset.list_by_plugin", "List presets for a specific plugin, with pagination", json!({
            "type": "object",
            "properties": {
                "plugin_path": {"type": "string", "description": "Path to the .clap plugin"},
                "plugin_id":   {"type": "string", "description": "The plugin's CLAP ID"},
                "page":        {"type": "integer", "description": "Page number (0-based)", "default": 0},
                "page_size":   {"type": "integer", "description": "Items per page (default 50)", "default": 50}
            },
            "required": ["plugin_path", "plugin_id"]
        })),
        tool_def("preset.status", "Get preset library statistics (total count, last scan time, unique plugins)", json!({
            "type": "object",
            "properties": {}
        })),
    ]
}

fn tool_def(name: &str, desc: &str, schema: serde_json::Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: desc.to_string(),
        input_schema: schema,
    }
}

/// Calling a tool. Read-only tools run immediately from the snapshot context.
/// Mutation tools return an error — the caller (server.rs) routes them through
/// the main-thread command queue instead.
pub fn call_tool(name: &str, params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    match name {
        "module.info" => Ok(crate::mcp::resources::read_resource("htrk://state", ctx)?),
        "order.get"   => Ok(crate::mcp::resources::read_resource("htrk://order", ctx)?),
        "cell.get"    => cell_get(params, ctx),
        "playback.state" => Ok(crate::mcp::resources::read_resource("htrk://playback", ctx)?),

        // Sample library tools
        "sample_library.configure" => cmd_sample_library_configure(params, ctx),
        "sample_library.list_dir"  => cmd_sample_library_list_dir(params, ctx),
        "sample_library.search"    => cmd_sample_library_search(params, ctx),

        // Plugin tools
        "plugin.scan" => crate::mcp::plugin_tools::cmd_plugin_scan(params, ctx),
        "plugin.list" => crate::mcp::plugin_tools::cmd_plugin_list(params, ctx),
        "plugin.info" => crate::mcp::plugin_tools::cmd_plugin_info(params, ctx),

        // Preset tools
        "preset.scan"          => crate::mcp::preset_tools::cmd_preset_scan(params, ctx),
        "preset.list"          => crate::mcp::preset_tools::cmd_preset_list(params, ctx),
        "preset.info"          => crate::mcp::preset_tools::cmd_preset_info(params, ctx),
        "preset.list_by_plugin"=> crate::mcp::preset_tools::cmd_preset_list_by_plugin(params, ctx),
        "preset.status"        => crate::mcp::preset_tools::cmd_preset_status(params, ctx),

        _ if MUTATION_TOOLS.contains(&name) => {
            Err("Requires mutation dispatch".into())
        }
        _ => Err(format!("Unknown tool '{name}'"))
    }
}

fn cell_get(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let order = params.get("order").and_then(|v| v.as_i64()).ok_or("Missing 'order'")? as usize;
    let row = params.get("row").and_then(|v| v.as_i64()).ok_or("Missing 'row'")? as usize;
    let channel = params.get("channel").and_then(|v| v.as_i64()).ok_or("Missing 'channel'")? as usize;

    let module = ctx.module_snapshot.module_json.as_ref().ok_or("No module loaded")?;
    let order_list = module.get("order_list").and_then(|v| v.as_array()).ok_or("No order list")?;
    let pat_idx = order_list.get(order).and_then(|v| v.as_i64()).ok_or("Order index out of range")? as usize;

    let pattern = ctx.module_snapshot.patterns_json.iter()
        .find(|(i, _)| *i == pat_idx)
        .map(|(_, data)| data)
        .ok_or("Pattern not found")?;

    let pat_data = pattern.get("data").and_then(|v| v.as_array()).ok_or("Invalid pattern data")?;
    let row_data = pat_data.get(row).and_then(|v| v.as_array()).ok_or("Row out of range")?;
    let cell = row_data.get(channel).ok_or("Channel out of range")?;
    Ok(cell.clone())
}

// ── Sample library handlers (read-only, run on MCP thread) ──

fn cmd_sample_library_configure(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let roots_arr = params.get("roots")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'roots' array")?;

    let roots: Vec<std::path::PathBuf> = roots_arr.iter()
        .filter_map(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .collect();

    let count = roots.len();
    let mut library = ctx.library.write().map_err(|e| format!("Library lock poisoned: {e}"))?;
    library.set_roots(roots);

    Ok(json!({ "roots_configured": count }))
}

fn cmd_sample_library_list_dir(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let path_str = params.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'path'")?;

    let page = params.get("page").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let page_size = params.get("page_size").and_then(|v| v.as_i64()).unwrap_or(50) as usize;

    let filter = params.get("filter").and_then(|f| {
        let name_contains = f.get("name_contains").and_then(|v| v.as_str()).map(String::from);
        let category = f.get("category").and_then(|v| v.as_str()).map(String::from);
        let min_duration = f.get("min_duration").and_then(|v| v.as_f64());
        let max_duration = f.get("max_duration").and_then(|v| v.as_f64());
        let min_bpm = f.get("min_bpm").and_then(|v| v.as_f64()).map(|n| n as f32);
        let max_bpm = f.get("max_bpm").and_then(|v| v.as_f64()).map(|n| n as f32);
        let key = f.get("key").and_then(|v| v.as_str()).map(String::from);
        let tag = f.get("tag").and_then(|v| v.as_str()).map(String::from);
        let channels_filter = f.get("channels_filter").and_then(|v| v.as_u64()).map(|n| n as u8);
        let has_any = name_contains.is_some()
            || category.is_some()
            || min_duration.is_some()
            || max_duration.is_some()
            || min_bpm.is_some()
            || max_bpm.is_some()
            || key.is_some()
            || tag.is_some()
            || channels_filter.is_some();
        if has_any {
            Some(DirFilter {
                name_contains,
                category,
                min_duration,
                max_duration,
                min_bpm,
                max_bpm,
                key,
                tag,
                channels_filter,
            })
        } else {
            None
        }
    });

    let mut library = ctx.library.write().map_err(|e| format!("Library lock poisoned: {e}"))?;
    let listing = library.list_dir(std::path::Path::new(path_str), page, page_size, filter.as_ref())?;
    serde_json::to_value(&listing).map_err(|e| format!("Serialization error: {e}"))
}

fn cmd_sample_library_search(params: serde_json::Value, ctx: &ToolContext) -> CmdResult {
    let query = params.get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'query'")?;

    let page = params.get("page").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let page_size = params.get("page_size").and_then(|v| v.as_i64()).unwrap_or(50) as usize;

    let scope_roots: Option<Vec<std::path::PathBuf>> = params.get("scope_roots")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str())
            .map(std::path::PathBuf::from)
            .collect());

    let library = ctx.library.read().map_err(|e| format!("Library lock poisoned: {e}"))?;
    let results = library.search(query, scope_roots.as_deref(), page, page_size);
    serde_json::to_value(&results).map_err(|e| format!("Serialization error: {e}"))
}
