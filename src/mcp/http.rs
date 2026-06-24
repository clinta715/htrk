use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::thread;

use crate::mcp::library::SampleLibrary;
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
        listener.set_nonblocking(true).ok();

        let shared = Arc::new(Shared {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            cmd_tx,
            snapshot,
            playback_snapshot,
            channels_snapshot,
            library,
            shutdown,
        });

        let join_handle = thread::Builder::new()
            .name("htrk-mcp-http".into())
            .spawn(move || {
                eprintln!("[mcph] HTTP server listening on 127.0.0.1:{actual_port}");
                let mut connections: Vec<(TcpStream, u64)> = Vec::new();
                let mut clean_counter = 0u64;

                loop {
                    if shared.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }

                    match listener.accept() {
                        Ok((stream, addr)) => {
                            stream.set_nonblocking(true).ok();
                            let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
                            eprintln!("[mcph] Connection {id} from {addr}");
                            connections.push((stream, id));
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => {
                            eprintln!("[mcph] Accept error: {e}");
                        }
                    }

                    let mut i = 0;
                    while i < connections.len() {
                        let conn_id = connections[i].1;
                        let mut read_buf = Vec::new();
                        let read_result = match connections.get(i) {
                            Some(s) => {
                                let mut reader = BufReader::new(s.0.try_clone().unwrap());
                                reader.read_until(b'\n', &mut read_buf)
                            }
                            None => { i += 1; continue; }
                        };

                        match read_result {
                            Ok(0) => {
                                connections.remove(i);
                                continue;
                            }
                            Ok(_) => {
                                let line = String::from_utf8_lossy(&read_buf).trim().to_string();
                                if line.is_empty() { i += 1; continue; }

                                // Determine if this is an HTTP request by checking for GET/POST prefix
                                let is_http = line.starts_with("GET ") || line.starts_with("POST ");

                                if is_http {
                                    // Read entire HTTP request from the connection
                                    let stream = connections[i].0.try_clone().unwrap();
                                    let shared_clone = shared.clone();
                                    let conn_id_clone = conn_id;

                                    // Spawn a dedicated thread to handle this HTTP request
                                    // This is needed because SSE keeps the connection alive
                                    thread::Builder::new()
                                        .name(format!("htrk-http-{conn_id_clone}"))
                                        .spawn(move || {
                                            handle_http_connection(stream, &line, shared_clone, conn_id_clone);
                                        })
                                        .ok();

                                    connections.remove(i);
                                } else {
                                    i += 1;
                                }
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                i += 1;
                            }
                            Err(e) => {
                                eprintln!("[mcph] Read error on conn {conn_id}: {e}");
                                connections.remove(i);
                            }
                        }
                    }

                    // Periodic session cleanup
                    clean_counter += 1;
                    if clean_counter % 600 == 0 {
                        if let Ok(mut sessions) = shared.sessions.lock() {
                            sessions.retain(|_, tx| tx.send(String::new()).is_ok() == false);
                            drop(sessions);
                        }
                    }

                    thread::sleep(std::time::Duration::from_millis(10));
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
    mut stream: TcpStream,
    first_line: &str,
    shared: Arc<Shared>,
    conn_id: u64,
) {
    // Blocking mode for HTTP handling
    let _ = stream.set_nonblocking(false);

    let request_line = first_line.to_string();

    // Read headers using a BufReader on a cloned handle so we don't borrow `stream`
    let read_stream = stream.try_clone().unwrap();
    let mut reader = BufReader::new(read_stream);
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
        handle_sse_get(&mut stream, &request_line, &headers, shared, conn_id);
    } else if request_line.starts_with("POST ") {
        handle_post(&mut stream, &request_line, &headers, content_length, &mut reader, shared);
    } else {
        let body = "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n";
        let _ = write!(stream, "{body}");
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
