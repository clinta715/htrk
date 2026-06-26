use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Helper: send a JSON-RPC request line and read the response line.
fn jsonrpc(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .expect("connect to MCP server");
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    writeln!(stream, "{request}").expect("write request");
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read response");
    line
}

/// Each test gets its own fresh `Arc<RwLock<PresetLibrary>>`, mirroring how
/// `HtrkApp::from_config` shares the same Arc with the MCP server.
fn fresh_preset_library() -> Arc<RwLock<htrk::audio::plugins::PresetLibrary>> {
    Arc::new(RwLock::new(htrk::audio::plugins::PresetLibrary::new()))
}

#[test]
fn test_mcp_ping() {
    let mut server = htrk::mcp::McpServer::start(0, None, fresh_preset_library());
    assert!(server.join_handle.is_some(), "server should bind");
    let port = server.port;
    assert!(port > 0);

    let resp = jsonrpc(port, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    assert!(resp.contains(r#""result":"pong""#), "ping response: {resp}");

    server.stop();
}

#[test]
fn test_mcp_tools_list() {
    let mut server = htrk::mcp::McpServer::start(0, None, fresh_preset_library());
    assert!(server.join_handle.is_some());
    let port = server.port;

    let resp = jsonrpc(port, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    assert!(resp.contains(r#""tools""#), "tools/list response: {resp}");
    // Should contain both read-only and mutation tools
    assert!(resp.contains("module.info"), "expected module.info tool");
    assert!(resp.contains("cell.set"), "expected cell.set tool");

    server.stop();
}

#[test]
fn test_mcp_resources_list() {
    let mut server = htrk::mcp::McpServer::start(0, None, fresh_preset_library());
    assert!(server.join_handle.is_some());
    let port = server.port;

    let resp = jsonrpc(port, r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#);
    assert!(resp.contains(r#""resources""#), "resources/list response: {resp}");
    assert!(resp.contains("htrk://state"), "expected htrk://state");

    server.stop();
}

#[test]
fn test_mcp_read_state_resource() {
    let mut server = htrk::mcp::McpServer::start(0, None, fresh_preset_library());
    assert!(server.join_handle.is_some());
    let port = server.port;

    // Populate snapshots so the resource has data
    {
        let mut snap = server.snapshot.write().unwrap();
        snap.module_json = Some(serde_json::json!({
            "name": "test_song",
            "format": "HTK",
            "order_list": [0, 1, 2]
        }));
    }
    {
        let mut ch = server.channels_snapshot.write().unwrap();
        ch.panning = vec![32, 32, 32, 32];
        ch.volume = vec![64, 64, 64, 64];
    }

    let resp = jsonrpc(
        port,
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"htrk://state"}}"#,
    );
    assert!(resp.contains("test_song"), "state resource: {resp}");

    server.stop();
}

#[test]
fn test_mcp_unknown_method() {
    let mut server = htrk::mcp::McpServer::start(0, None, fresh_preset_library());
    assert!(server.join_handle.is_some());
    let port = server.port;

    let resp = jsonrpc(port, r#"{"jsonrpc":"2.0","id":1,"method":"bogus"}"#);
    assert!(resp.contains("-32601"), "expected Method not found error: {resp}");

    server.stop();
}

#[test]
fn test_mcp_invalid_json() {
    let mut server = htrk::mcp::McpServer::start(0, None, fresh_preset_library());
    let port = server.port;

    let resp = jsonrpc(port, "not-json");
    assert!(resp.contains("-32700"), "expected Parse error: {resp}");

    server.stop();
}
