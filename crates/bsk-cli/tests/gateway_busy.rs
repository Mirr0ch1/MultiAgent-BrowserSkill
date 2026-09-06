//! M2: cross-agent Busy semantics over the gateway TCP IPC transport.
//!
//! Two CLI agents authenticate over TCP to the same gateway daemon. When
//! agent A owns a session on browser X, agent B selecting X must get a
//! BUSY error naming the owner; the same agent may open more sessions
//! (multi-tab is legal); `share: true` explicitly overrides the refusal.

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bsk::daemon::{self, DaemonConfig};
use bsk::ipc_client::tcp::TcpClient;
use bsk_protocol::{ErrorCode, Method, ResponseBody, ResponseFrame};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde_json::Value;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::protocol::Message;

const AGENT_TOKEN: &str = "sekrit-agent-token";
const EXT_ID_A: &str = "aaaaaaaabbbbbbbbccccccccdddddddd";
const EXT_ID_B: &str = "ddddddddccccccccbbbbbbbbaaaaaaaa";

type TestWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn tempfile_path(prefix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rng = rand::thread_rng();
    let suffix: String = (0..8)
        .map(|_| char::from_digit(rng.gen_range(0..16), 16).unwrap())
        .collect();
    p.push(format!("{prefix}-{}-{suffix}.sock", std::process::id()));
    p
}

/// Spawn a gateway daemon with TCP IPC (random port) + agent token.
async fn spawn_gateway_daemon() -> (daemon::DaemonHandle, PathBuf) {
    let mut config = DaemonConfig::new(0);
    config.agent_port = Some(0);
    config.agent_token = Some(AGENT_TOKEN.to_string());
    let sock = tempfile_path("bsk-test-gateway-busy");
    let handle = daemon::run(config, Some(sock.clone())).await.unwrap();
    (handle, sock)
}

fn tcp_port(handle: &daemon::DaemonHandle) -> u16 {
    handle
        .tcp_addr()
        .expect("gateway daemon must expose a TCP IPC listener")
        .port()
}

async fn connect_ext(addr: std::net::SocketAddr, ext_id: &str) -> TestWs {
    let origin = format!("chrome-extension://{ext_id}");
    let url = format!("ws://{addr}/");
    let req = Request::builder()
        .method("GET")
        .uri(&url)
        .header("Host", addr.to_string())
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_key())
        .header("Origin", origin)
        .body(())
        .unwrap();
    let (ws, _resp) = tokio_tungstenite::connect_async(req).await.unwrap();
    ws
}

async fn handshake_as_ext(ws: &mut TestWs, ext_id: &str) {
    let params = bsk_protocol::system::HandshakeParams {
        client: "browser-skill-extension".into(),
        version: "0.1.0-dev.0".parse().unwrap(),
        protocol_version: "1.2".into(),
        instance_id: ext_id.into(),
        browser: bsk_protocol::BrowserPeerInfo {
            name: "chrome".into(),
            version: "131.0".into(),
        },
        min_compatible_peer: Some("0.1.0-dev.0".parse().unwrap()),
        min_compatible_protocol: Some("1.0".into()),
        label: "Busy-test ext".into(),
        token: None,
        agent_id: None,
    };
    let req = bsk_protocol::RequestFrame {
        id: "hs".into(),
        method: Method::SystemHandshake,
        params: Some(serde_json::to_value(params).unwrap()),
    };
    ws.send(Message::Text(serde_json::to_string(&req).unwrap()))
        .await
        .unwrap();
    let resp = ws.next().await.unwrap().unwrap();
    let text = match resp {
        Message::Text(t) => t,
        _ => panic!("expected handshake text response"),
    };
    let resp: ResponseFrame = serde_json::from_str(&text).unwrap();
    match resp.body {
        ResponseBody::Ok(_) => {}
        ResponseBody::Err(e) => panic!("handshake rejected: {e:?}"),
    }
}

/// Fake-extension responder answering every `tool.session_start` with a
/// fresh agent window id. Runs until the socket closes.
fn spawn_session_responder(ws: TestWs) {
    let ws = Arc::new(tokio::sync::Mutex::new(ws));
    tokio::spawn(async move {
        loop {
            let next = {
                let mut g = ws.lock().await;
                g.next().await
            };
            let msg = match next {
                Some(Ok(m)) => m,
                _ => break,
            };
            let text = match msg {
                Message::Text(t) => t,
                Message::Close(_) => break,
                _ => continue,
            };
            let frame: bsk_protocol::Frame = match serde_json::from_str(&text) {
                Ok(frame) => frame,
                Err(_) => continue,
            };
            if let bsk_protocol::Frame::Request(req) = frame {
                match req.method {
                    Method::ToolSessionStart => {
                        let reply = ResponseFrame {
                            id: req.id.clone(),
                            body: ResponseBody::Ok(serde_json::json!({
                                "agent_window_id": 4242,
                            })),
                        };
                        let mut g = ws.lock().await;
                        let _ = g
                            .send(Message::Text(serde_json::to_string(&reply).unwrap()))
                            .await;
                    }
                    Method::ToolSessionStop => {
                        let reply = ResponseFrame {
                            id: req.id.clone(),
                            body: ResponseBody::Ok(serde_json::json!({})),
                        };
                        let mut g = ws.lock().await;
                        let _ = g
                            .send(Message::Text(serde_json::to_string(&reply).unwrap()))
                            .await;
                    }
                    _ => {}
                }
            }
        }
    });
}

fn start_params(browser: Option<&str>, agent: Option<&str>, share: bool) -> Value {
    serde_json::json!({
        "browser_instance_id": browser,
        "agent": agent,
        "share": share,
    })
}

/// Start a session over TCP as `agent`; returns the structured error if
/// refused.
async fn start_session_via_tcp(
    port: u16,
    agent: Option<&str>,
    share: bool,
) -> Result<Value, bsk_protocol::RpcError> {
    let mut client = TcpClient::connect(
        Ipv4Addr::LOCALHOST.into(),
        port,
        Some(AGENT_TOKEN),
    )
    .await
    .expect("valid agent token must authenticate");
    let outcome = client
        .call::<_, Value>(
            Method::SessionStart,
            &start_params(None, agent, share),
            Duration::from_secs(5),
        )
        .await
        .expect("session.start RPC on TCP should not fail at transport level");
    outcome
}

#[tokio::test]
async fn busy_semantics_second_agent_rejected_and_share_overrides() {
    let (handle, _sock) = spawn_gateway_daemon().await;
    let ws_addr = handle.ws_addr();
    let port = tcp_port(&handle);

    // Register browser X as a fake extension with a session responder.
    let ext_a = connect_ext(ws_addr, EXT_ID_A).await;
    let mut ext_a = ext_a;
    handshake_as_ext(&mut ext_a, EXT_ID_A).await;
    spawn_session_responder(ext_a);

    // Agent A starts a session (owns browser X implicitly).
    let a_start = start_session_via_tcp(port, Some("agent-A"), false)
        .await
        .expect("agent A session.start must succeed");
    assert!(
        a_start.get("session_id").and_then(Value::as_str).is_some(),
        "expected session_id in A's start reply, got {a_start:?}"
    );

    // Agent B tries the same browser → BUSY naming owner A.
    let b_start = start_session_via_tcp(port, Some("agent-B"), false)
        .await
        .expect_err("agent B must be refused on a browser owned by agent A");
    assert_eq!(b_start.code, ErrorCode::PermissionDenied);
    assert!(
        b_start.message.contains("busy") || b_start.message.contains("Busy"),
        "expected busy message, got: {}",
        b_start.message
    );

    // Same agent may open another session (multi-tab legal).
    let a_second = start_session_via_tcp(port, Some("agent-A"), false)
        .await
        .expect("same agent may open additional sessions");
    assert!(a_second.get("session_id").is_some());

    // Agent B with explicit share overrides the refusal.
    let b_share = start_session_via_tcp(port, Some("agent-B"), true)
        .await
        .expect("share=true must override the cross-agent Busy refusal");
    assert!(b_share.get("session_id").is_some());

    handle.shutdown().await;
}

#[tokio::test]
async fn unowned_legacy_sessions_do_not_block_agents() {
    let (handle, _sock) = spawn_gateway_daemon().await;
    let ws_addr = handle.ws_addr();
    let port = tcp_port(&handle);

    // Register browser Y (a second extension identity).
    let ext_b = connect_ext(ws_addr, EXT_ID_B).await;
    let mut ext_b = ext_b;
    handshake_as_ext(&mut ext_b, EXT_ID_B).await;
    spawn_session_responder(ext_b);

    // A new agent starts on it fine (unowned registry → no Busy).
    let start = start_session_via_tcp(port, Some("agent-C"), false)
        .await
        .expect("fresh browser with no other agent owner must not be busy");
    assert!(start.get("session_id").is_some());

    handle.shutdown().await;
}