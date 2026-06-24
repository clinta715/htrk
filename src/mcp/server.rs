use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::thread;

use crate::mcp::library::SampleLibrary;
use crate::audio::plugins::PluginLibrary;
use crate::mcp::protocol::*;
use crate::mcp::resources;
use crate::mcp::tools;
use crate::mcp::http::HttpServer;

pub struct McpServer {
    pub port: u16,
    pub http_port: Option<u16>,
    pub join_handle: Option<thread::JoinHandle<()>>,
    pub http_server: Option<HttpServer>,
    pub command_tx: mpsc::Sender<McpCommand>,
    pub command_rx: mpsc::Receiver<McpCommand>,
    pub snapshot: Arc<RwLock<ModuleSnapshot>>,
    pub playback_snapshot: Arc<RwLock<PlaybackSnapshot>>,
    pub channels_snapshot: Arc<RwLock<ChannelsSnapshot>>,
    pub library: Arc<RwLock<SampleLibrary>>,
    pub plugin_library: Arc<RwLock<PluginLibrary>>,
    pub shutdown: Arc<AtomicBool>,
}

impl McpServer {
    /// Start an MCP server on the given port.
    /// If `port` is 0, the OS assigns an ephemeral port; the actual port is returned in the `port` field.
    /// If `http_port` is Some, also start an HTTP SSE transport server on that port.
    pub fn start(port: u16, http_port: Option<u16>) -> Self {
        let addr = format!("127.0.0.1:{port}");
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[mcp] Failed to bind to {addr}: {e}");
                let (command_tx, command_rx) = mpsc::channel::<McpCommand>();
                return McpServer {
                    port,
                    http_port: None,
                    join_handle: None,
                    http_server: None,
                    command_tx,
                    command_rx,
                    snapshot: Arc::new(RwLock::new(ModuleSnapshot::default())),
                    playback_snapshot: Arc::new(RwLock::new(PlaybackSnapshot::default())),
                    channels_snapshot: Arc::new(RwLock::new(ChannelsSnapshot::default())),
                    library: Arc::new(RwLock::new(SampleLibrary::new())),
                    plugin_library: Arc::new(RwLock::new(PluginLibrary::new())),
                    shutdown: Arc::new(AtomicBool::new(true)),
                };
            }
        };
        let actual_port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).ok();

        let (command_tx, command_rx) = mpsc::channel::<McpCommand>();
        let snapshot = Arc::new(RwLock::new(ModuleSnapshot::default()));
        let playback_snapshot = Arc::new(RwLock::new(PlaybackSnapshot::default()));
        let channels_snapshot = Arc::new(RwLock::new(ChannelsSnapshot::default()));
        let library = Arc::new(RwLock::new(SampleLibrary::new()));
        let plugin_library = Arc::new(RwLock::new(PluginLibrary::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let snapshot_clone = snapshot.clone();
        let pb_snapshot_clone = playback_snapshot.clone();
        let ch_snapshot_clone = channels_snapshot.clone();
        let library_clone = library.clone();
        let plugin_library_clone = plugin_library.clone();
        let shutdown_clone = shutdown.clone();
        let cmd_tx = command_tx.clone();

        let join_handle = thread::Builder::new()
            .name("htrk-mcp".into())
            .spawn(move || {
                eprintln!("[mcp] Server listening on 127.0.0.1:{actual_port}");

                let mut connections: Vec<TcpStream> = Vec::new();

                loop {
                    if shutdown_clone.load(Ordering::Relaxed) {
                        break;
                    }

                    match listener.accept() {
                        Ok((stream, addr)) => {
                            eprintln!("[mcp] Connection from {addr}");
                            stream.set_nonblocking(true).ok();
                            connections.push(stream);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => {
                            eprintln!("[mcp] Accept error: {e}");
                        }
                    }

                    let mut i = 0;
                    while i < connections.len() {
                        let mut read_buf = String::new();
                        let read_result = match connections.get(i) {
                            Some(s) => {
                                let mut reader = BufReader::new(s.try_clone().unwrap());
                                reader.read_line(&mut read_buf)
                            }
                            None => { i += 1; continue; }
                        };

                        match read_result {
                            Ok(0) => {
                                connections.remove(i);
                                continue;
                            }
                            Ok(_) => {
                                let trimmed = read_buf.trim();
                                if !trimmed.is_empty() {
                                    let snapshot = snapshot_clone.read().unwrap();
                                    let pb = pb_snapshot_clone.read().unwrap();
                                    let ch = ch_snapshot_clone.read().unwrap();
                                    let ctx = ToolContext {
                                        module_snapshot: snapshot.clone(),
                                        playback_snapshot: pb.clone(),
                                        channels_snapshot: ch.clone(),
                                        library: library_clone.clone(),
                                        plugin_library: plugin_library_clone.clone(),
                                    };
                                    drop(snapshot);
                                    drop(pb);
                                    drop(ch);

                                    let response = handle_jsonrpc(trimmed, &ctx, &cmd_tx);

                                    if let Some(s) = connections.get(i) {
                                        let mut write_stream = s.try_clone().unwrap();
                                        let resp_str = serde_json::to_string(&response).unwrap_or_default();
                                        let _ = writeln!(write_stream, "{resp_str}");
                                        let _ = write_stream.flush();
                                    }
                                }
                                i += 1;
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                i += 1;
                            }
                            Err(e) => {
                                eprintln!("[mcp] Read error: {e}");
                                connections.remove(i);
                            }
                        }
                    }

                    thread::sleep(std::time::Duration::from_millis(10));
                }
                eprintln!("[mcp] Server shutting down");
            })
            .ok();

        // Start HTTP server if requested
        let http_server = http_port.map(|hp| {
            let hp_actual = if hp == 0 { actual_port + 1 } else { hp };
            eprintln!("[mcp] Starting HTTP SSE transport on 127.0.0.1:{hp_actual}");
            HttpServer::start(
                hp_actual,
                command_tx.clone(),
                snapshot.clone(),
                playback_snapshot.clone(),
                channels_snapshot.clone(),
                library.clone(),
                plugin_library.clone(),
                shutdown.clone(),
            )
        });

        McpServer {
            port: actual_port,
            http_port: http_server.as_ref().map(|s| s.port),
            join_handle,
            http_server,
            command_tx,
            command_rx,
            snapshot,
            playback_snapshot,
            channels_snapshot,
            library,
            plugin_library,
            shutdown,
        }
    }

    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(mut hs) = self.http_server.take() {
            hs.stop();
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) fn handle_jsonrpc(
    line: &str,
    ctx: &ToolContext,
    cmd_tx: &mpsc::Sender<McpCommand>,
) -> JsonRpcResponse {
    let request: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => return JsonRpcResponse::error(None, -32700, format!("Parse error: {e}")),
    };

    let id = request.id.clone();

    match request.method.as_str() {
        "tools/list" => {
            let tools_list = tools::list_tools();
            let json = serde_json::to_value(&tools_list).unwrap_or_default();
            JsonRpcResponse::success(id, serde_json::json!({ "tools": json }))
        }

        "resources/list" => {
            let resources_list = resources::list_resources();
            let json = serde_json::to_value(&resources_list).unwrap_or_default();
            JsonRpcResponse::success(id, serde_json::json!({ "resources": json }))
        }

        "resources/read" => {
            let uri = request.params.as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match resources::read_resource(uri, ctx) {
                Ok(data) => JsonRpcResponse::success(id, serde_json::json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": serde_json::to_string_pretty(&data).unwrap_or_default() }] })),
                Err(e) => JsonRpcResponse::error(id, -32602, e),
            }
        }

        "tools/call" => {
            let name = request.params.as_ref()
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = request.params.as_ref()
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            // Try read-only path first
            match tools::call_tool(name, arguments.clone(), ctx) {
                Ok(result) => {
                    return JsonRpcResponse::success(id, serde_json::json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }));
                }
                Err(ref e) if e == "Requires mutation dispatch" => {
                    // Route through main-thread command queue
                    let (response_tx, response_rx) = mpsc::channel();
                    let cmd = McpCommand {
                        method: name.to_string(),
                        params: arguments,
                        response_tx,
                    };
                    match cmd_tx.send(cmd) {
                        Ok(()) => {
                            match response_rx.recv() {
                                Ok(Ok(result)) => {
                                    JsonRpcResponse::success(id, serde_json::json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }] }))
                                }
                                Ok(Err(e)) => JsonRpcResponse::error(id, -32603, format!("Mutation failed: {e}")),
                                Err(_) => JsonRpcResponse::error(id, -32603, "Main thread closed – the application is shutting down".into()),
                            }
                        }
                        Err(_) => JsonRpcResponse::error(id, -32603, "Command queue full – too many concurrent mutation requests".into()),
                    }
                }
                Err(e) => JsonRpcResponse::error(id, -32603, format!("Tool error: {e}")),
            }
        }

        "ping" => {
            JsonRpcResponse::success(id, serde_json::Value::String("pong".into()))
        }

        _ => JsonRpcResponse::error(id, -32601, format!("Unknown method '{}'. Use 'tools/list' and 'resources/list' to discover available methods", request.method)),
    }
}
