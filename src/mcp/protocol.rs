use std::sync::{mpsc, Arc, RwLock};
use std::sync::atomic::AtomicBool;
use serde::{Deserialize, Serialize};

use crate::mcp::library::SampleLibrary;
use crate::audio::plugins::PluginLibrary;
use crate::audio::plugins::PresetLibrary;

// ── MCP JSON-RPC types (subset of the MCP spec) ──

pub type CmdResult = Result<serde_json::Value, String>;
pub type CmdTx = mpsc::Sender<CmdResult>;

/// A mutation request dispatched from the MCP thread to the main thread.
pub struct McpCommand {
    pub method: String,
    pub params: serde_json::Value,
    pub response_tx: CmdTx,
}

// ── JSON-RPC wire types ──

#[derive(Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        JsonRpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
    }
    pub fn error(id: Option<serde_json::Value>, code: i32, message: String) -> Self {
        JsonRpcResponse { jsonrpc: "2.0", id, result: None, error: Some(JsonRpcError { code, message, data: None }) }
    }
}

// ── MCP tool/resource definitions ──

#[derive(Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Clone, Serialize)]
pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

/// A tool implementation: given params, returns a JSON result.
pub type ToolFn = fn(serde_json::Value, &ToolContext) -> CmdResult;

/// Context passed to every tool/resource handler.
pub struct ToolContext {
    pub module_snapshot: ModuleSnapshot,
    pub playback_snapshot: PlaybackSnapshot,
    pub channels_snapshot: ChannelsSnapshot,
    pub library: Arc<RwLock<SampleLibrary>>,
    pub plugin_library: Arc<RwLock<PluginLibrary>>,
    pub preset_library: Arc<RwLock<PresetLibrary>>,
    pub preset_scan_in_progress: Arc<AtomicBool>,
}

// ── Snapshots (read-only, shared with MCP thread via Arc<RwLock<>>) ──

#[derive(Clone, Default)]
pub struct ModuleSnapshot {
    pub module_json: Option<serde_json::Value>,
    pub patterns_json: Vec<(usize, serde_json::Value)>,
    pub instruments_json: Vec<(usize, serde_json::Value)>,
    pub samples_json: Vec<(usize, serde_json::Value)>,
}

#[derive(Clone, Default)]
pub struct PlaybackSnapshot {
    pub playing: bool,
    pub current_order: u16,
    pub current_row: u16,
    pub current_pattern: u16,
    pub current_tick: u8,
    pub bpm: u16,
    pub speed: u8,
    pub active_voices: u8,
    pub cpu_usage_pct: u8,
}

#[derive(Clone, Default)]
pub struct ChannelsSnapshot {
    pub panning: Vec<u8>,
    pub volume: Vec<u8>,
    pub muted: Vec<bool>,
    pub solo: Vec<bool>,
}
