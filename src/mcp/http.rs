use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use crate::mcp::library::SampleLibrary;
use crate::audio::plugins::{PluginLibrary, PresetLibrary};
use crate::mcp::protocol::*;

pub struct HttpServer {
    pub port: u16,
    pub join_handle: Option<thread::JoinHandle<()>>,
}

struct Shared {
    sessions: Mutex<HashMap<String, mpsc::Sender<String>>>,
    next_id: AtomicU64,
    cmd_tx: mpsc::Sender<McpCommand>,
    snapshot: Arc<RwLock<ModuleSnapshot>>,
    playback_snapshot: Arc<RwLock<PlaybackSnapshot>>,
    channels_snapshot: Arc<RwLock<ChannelsSnapshot>>,
    library: Arc<RwLock<SampleLibrary>>,
    plugin_library: Arc<RwLock<PluginLibrary>>,
    preset_library: Arc<RwLock<PresetLibrary>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl HttpServer {
    pub fn start(
        port: u16,
        cmd_tx: mpsc::Sender<McpCommand>,
        snapshot: Arc<RwLock<ModuleSnapshot>>,
        playback_snapshot: Arc<RwLock<PlaybackSnapshot>>,
        channels_snapshot: Arc<RwLock<ChannelsSnapshot>>,
        library: Arc<RwLock<SampleLibrary>>,
        plugin_library: Arc<RwLock<PluginLibrary>>,
        preset_library: Arc<RwLock<PresetLibrary>>,
        shutdown: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let addr = format!("127.0.0.1:{port}");
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[mcph] Failed to bind HTTP to {addr}: {e}");
                return HttpServer { port, join_handle: None };
            }
        };
        let actual_port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(false).ok();  // blocking accept

        let shared = Arc::new(Shared {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            cmd_tx,
            snapshot,
            playback_snapshot,
            channels_snapshot,
            library,
            plugin_library,
            preset_library,
            shutdown,
        });

        let join_handle = thread::Builder::new()
            .name("htrk-mcp-http".into())
            .spawn(move || {
                eprintln!("[mcph] HTTP server listening on 127.0.0.1:{actual_port}");
                let mut clean_counter = 0u64;

                // For each accepted connection, spawn a blocking handler
                // thread.  The raw `try_clone()` + BufReader approach
                // that the original code used would buffer the entire
                // HTTP request into the BufReader's internal buffer
                // during the first-line read, discard it when the
                // reader is dropped, and leave the handler thread with
                // a stream whose OS read position was already advanced
                // past the data — resulting in empty headers, zero
                // content_length, and a silent parse failure.
                loop {
                    if shared.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }

                    let (stream, addr) = match listener.accept() {
                        Ok(s) => s,
                        Err(e) => {
                            // Blocking listener only errors on real problems.
                            eprintln!("[mcph] Accept error: {e}");
                            thread::sleep(Duration::from_millis(100));
                            continue;
                        }
                    };

                    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
                    eprintln!("[mcph] Connection {id} from {addr}");

                    let shared_clone = shared.clone();
                    thread::Builder::new()
                        .name(format!("htrk-http-{id}"))
                        .spawn(move || {
                            handle_http_connection(stream, shared_clone, id);
                        })
                        .ok();

                    // Periodic session cleanup (every ~500 connections)
                    clean_counter += 1;
                    if clean_counter % 500 == 0 {
                        if let Ok(mut sessions) = shared.sessions.lock() {
                            sessions.retain(|_, tx| tx.send(String::new()).is_ok() == false);
                            drop(sessions);
                        }
                    }
                }
                eprintln!("[mcph] HTTP server shutting down");
            })
            .ok();

        HttpServer { port: actual_port, join_handle }
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_http_connection(
    stream: TcpStream,
    shared: Arc<Shared>,
    conn_id: u64,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let (read_stream, mut write_stream) = match stream.try_clone() {
        Ok(c) => (c, stream),
        Err(_) => return,
    };
    let mut reader = BufReader::new(read_stream);

    // Read the request line.
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let request_line = request_line.trim().to_string();
    if !request_line.starts_with("GET ") && !request_line.starts_with("POST ") {
        let body = "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n";
        let _ = write!(write_stream, "{body}");
        return;
    }

    // Read headers.
    let mut headers = Vec::new();
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() {
                    break;
                }
                let lower = trimmed.to_lowercase();
                if lower.starts_with("content-length:") {
                    if let Ok(n) = trimmed[15..].trim().parse::<usize>() {
                        content_length = n;
                    }
                }
                headers.push(trimmed);
            }
        }
    }

    if request_line.starts_with("GET ") {
        handle_sse_get(&mut write_stream, &request_line, &headers, shared, conn_id);
    } else if request_line.starts_with("POST ") {
        handle_post(&mut write_stream, &request_line, &headers, content_length, &mut reader, shared);
    }
}

fn handle_sse_get(
    stream: &mut TcpStream,
    _request_line: &str,
    _headers: &[String],
    shared: Arc<Shared>,
    conn_id: u64,
) {
    let session_id = format!("s{conn_id}");
    let (tx, rx) = mpsc::channel::<String>();

    // Register session
    {
        if let Ok(mut sessions) = shared.sessions.lock() {
            sessions.insert(session_id.clone(), tx);
        }
    }

    // Send HTTP headers
    let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
    if write!(stream, "{resp}").is_err() || stream.flush().is_err() {
        cleanup_session(&shared, &session_id);
        return;
    }

    // Send endpoint event
    let endpoint = format!("event: endpoint\ndata: /message?sessionId={session_id}\n\n");
    if write!(stream, "{endpoint}").is_err() || stream.flush().is_err() {
        cleanup_session(&shared, &session_id);
        return;
    }

    // Keep connection alive, push SSE events
    loop {
        match rx.recv() {
            Ok(msg) => {
                let sse = format!("data: {msg}\n\n");
                if write!(stream, "{sse}").is_err() || stream.flush().is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    cleanup_session(&shared, &session_id);
}

fn handle_post(
    stream: &mut TcpStream,
    _request_line: &str,
    _headers: &[String],
    content_length: usize,
    reader: &mut BufReader<TcpStream>,
    shared: Arc<Shared>,
) {
    // Read body
    let mut body = vec![0u8; content_length];
    if reader.read_exact(&mut body).is_err() {
        let _ = write!(stream, "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
        return;
    }
    let body_str = String::from_utf8_lossy(&body).to_string();

    // Build context from snapshots
    let snapshot = match shared.snapshot.read() {
        Ok(s) => s,
        Err(_) => {
            let _ = write!(stream, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    let pb = match shared.playback_snapshot.read() {
        Ok(p) => p,
        Err(_) => {
            let _ = write!(stream, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    let ch = match shared.channels_snapshot.read() {
        Ok(c) => c,
        Err(_) => {
            let _ = write!(stream, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    let ctx = ToolContext {
        module_snapshot: snapshot.clone(),
        playback_snapshot: pb.clone(),
        channels_snapshot: ch.clone(),
        library: shared.library.clone(),
        plugin_library: shared.plugin_library.clone(),
        preset_library: shared.preset_library.clone(),
    };
    drop(snapshot);
    drop(pb);
    drop(ch);

    // Process JSON-RPC
    let response = crate::mcp::server::handle_jsonrpc(&body_str, &ctx, &shared.cmd_tx);
    let resp_str = serde_json::to_string(&response).unwrap_or_default();

    // If this is a mutation that went through the command queue, the handle_jsonrpc
    // already blocks until the response comes back. So we can return the result directly.
    let http_resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        resp_str.len(),
        resp_str
    );
    let _ = write!(stream, "{http_resp}");
    let _ = stream.flush();
}

fn cleanup_session(shared: &Arc<Shared>, session_id: &str) {
    if let Ok(mut sessions) = shared.sessions.lock() {
        sessions.remove(session_id);
    }
}
