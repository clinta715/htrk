use crate::mcp::protocol::*;

pub fn list_resources() -> Vec<ResourceDefinition> {
    vec![
        ResourceDefinition {
            uri: "htrk://state".into(),
            name: "Module State".into(),
            description: "Top-level song summary".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://order".into(),
            name: "Order List".into(),
            description: "Play order — array of pattern indices".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://pattern".into(),
            name: "All Patterns".into(),
            description: "All pattern definitions with cell data".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://pattern/{index}".into(),
            name: "Single Pattern".into(),
            description: "Pattern data for a specific index".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://instrument".into(),
            name: "All Instruments".into(),
            description: "All instrument definitions with envelope data".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://instrument/{index}".into(),
            name: "Single Instrument".into(),
            description: "Instrument data for a specific index".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://sample".into(),
            name: "All Samples".into(),
            description: "All sample metadata (no PCM data)".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://sample/{index}".into(),
            name: "Single Sample".into(),
            description: "Sample metadata for a specific index".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://playback".into(),
            name: "Playback State".into(),
            description: "Live playback position and timing".into(),
            mime_type: "application/json".into(),
        },
        ResourceDefinition {
            uri: "htrk://channels".into(),
            name: "Channel Configuration".into(),
            description: "Per-channel panning, volume, mute, solo".into(),
            mime_type: "application/json".into(),
        },
    ]
}

pub fn read_resource(uri: &str, ctx: &ToolContext) -> CmdResult {
    match uri {
        "htrk://state" => read_state(ctx),
        "htrk://order" => read_order(ctx),
        "htrk://pattern" => read_all_patterns(ctx),
        "htrk://instrument" => read_all_instruments(ctx),
        "htrk://sample" => read_all_samples(ctx),
        "htrk://playback" => read_playback(ctx),
        "htrk://channels" => read_channels(ctx),
        _ => {
            // try parameterized URIs: htrk://pattern/{index}, htrk://instrument/{index}, htrk://sample/{index}
            if let Some(idx) = strip_prefix(uri, "htrk://pattern/") {
                read_pattern(idx, ctx)
            } else if let Some(idx) = strip_prefix(uri, "htrk://instrument/") {
                read_instrument(idx, ctx)
            } else if let Some(idx) = strip_prefix(uri, "htrk://sample/") {
                read_sample(idx, ctx)
            } else {
                Err(format!("Unknown resource URI '{uri}'. Valid URIs: htrk://state, htrk://order, htrk://pattern/&lt;index&gt;, htrk://instrument/&lt;index&gt;, htrk://sample/&lt;index&gt;, htrk://playback, htrk://channels"))
            }
        }
    }
}

fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.strip_prefix(prefix)
}

fn as_usize(idx_str: &str) -> Result<usize, String> {
    idx_str.parse::<usize>().map_err(|e| format!("Invalid index '{idx_str}': {e}"))
}

fn read_state(ctx: &ToolContext) -> CmdResult {
    let module = &ctx.module_snapshot;
    let pb = &ctx.playback_snapshot;
    let json = serde_json::json!({
        "name": module.module_json.as_ref().and_then(|j| j.get("name")).and_then(|v| v.as_str()).unwrap_or(""),
        "format": module.module_json.as_ref().and_then(|j| j.get("format")).and_then(|v| v.as_str()).unwrap_or("HTK"),
        "channels": ctx.channels_snapshot.panning.len(),
        "bpm": pb.bpm,
        "speed": pb.speed,
        "num_patterns": module.patterns_json.len(),
        "num_instruments": module.instruments_json.len(),
        "num_samples": module.samples_json.len(),
        "num_orders": module.module_json.as_ref().and_then(|j| j.get("order_list")).map(|v| v.as_array().map(|a| a.len()).unwrap_or(0)).unwrap_or(0),
        "playing": pb.playing,
    });
    Ok(json)
}

fn read_order(ctx: &ToolContext) -> CmdResult {
    let order = ctx.module_snapshot.module_json.as_ref()
        .and_then(|j| j.get("order_list").cloned())
        .unwrap_or(serde_json::Value::Array(vec![]));
    Ok(order)
}

fn read_all_patterns(ctx: &ToolContext) -> CmdResult {
    let arr: Vec<serde_json::Value> = ctx.module_snapshot.patterns_json.iter()
        .map(|(idx, data)| serde_json::json!({"index": idx, "data": data}))
        .collect();
    Ok(serde_json::Value::Array(arr))
}

fn read_pattern(idx_str: &str, ctx: &ToolContext) -> CmdResult {
    let idx = as_usize(idx_str)?;
    ctx.module_snapshot.patterns_json.iter()
        .find(|(i, _)| *i == idx)
        .map(|(_, data)| data.clone())
        .ok_or_else(|| format!("Pattern {idx} not found"))
}

fn read_all_instruments(ctx: &ToolContext) -> CmdResult {
    let arr: Vec<serde_json::Value> = ctx.module_snapshot.instruments_json.iter()
        .map(|(idx, data)| serde_json::json!({"index": idx, "data": data}))
        .collect();
    Ok(serde_json::Value::Array(arr))
}

fn read_instrument(idx_str: &str, ctx: &ToolContext) -> CmdResult {
    let idx = as_usize(idx_str)?;
    ctx.module_snapshot.instruments_json.iter()
        .find(|(i, _)| *i == idx)
        .map(|(_, data)| data.clone())
        .ok_or_else(|| format!("Instrument {idx} not found"))
}

fn read_all_samples(ctx: &ToolContext) -> CmdResult {
    let arr: Vec<serde_json::Value> = ctx.module_snapshot.samples_json.iter()
        .map(|(idx, data)| serde_json::json!({"index": idx, "data": data}))
        .collect();
    Ok(serde_json::Value::Array(arr))
}

fn read_sample(idx_str: &str, ctx: &ToolContext) -> CmdResult {
    let idx = as_usize(idx_str)?;
    ctx.module_snapshot.samples_json.iter()
        .find(|(i, _)| *i == idx)
        .map(|(_, data)| data.clone())
        .ok_or_else(|| format!("Sample {idx} not found"))
}

fn read_playback(ctx: &ToolContext) -> CmdResult {
    let pb = &ctx.playback_snapshot;
    Ok(serde_json::json!({
        "playing": pb.playing,
        "order": pb.current_order,
        "row": pb.current_row,
        "pattern": pb.current_pattern,
        "tick": pb.current_tick,
        "bpm": pb.bpm,
        "speed": pb.speed,
        "active_voices": pb.active_voices,
        "cpu_usage_pct": pb.cpu_usage_pct,
    }))
}

fn read_channels(ctx: &ToolContext) -> CmdResult {
    let ch = &ctx.channels_snapshot;
    let items: Vec<serde_json::Value> = (0..ch.panning.len()).map(|i| {
        serde_json::json!({
            "channel": i,
            "panning": ch.panning.get(i).copied().unwrap_or(32),
            "volume": ch.volume.get(i).copied().unwrap_or(64),
            "muted": ch.muted.get(i).copied().unwrap_or(false),
            "solo": ch.solo.get(i).copied().unwrap_or(false),
        })
    }).collect();
    Ok(serde_json::Value::Array(items))
}
