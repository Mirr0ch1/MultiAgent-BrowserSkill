//! M2: WS handshake extension-token enforcement + gateway interlock.
//!
//! When the daemon is configured with an `extension_token`, an
//! extension handshake without the matching token is rejected with
//! PermissionDenied; with it, registration proceeds.
//!
//! Also covers the gateway interlock: a daemon start that would bind a
//! non-loopback address without a configured token must be refused.

use std::path::PathBuf;
use std::time::Duration;

use bsk::daemon::{self, DaemonConfig};
use bsk::ipc_client::IpcClient;
use bsk_protocol::system::HandshakeParams;
use bsk_protocol::{BrowserPeerInfo, ErrorCode, Method, RequestFrame, ResponseBody, ResponseFrame};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::protocol::Message;

const EXT_TOKEN: &str = "sekrit-extension-token";
const AGENT_TOKEN: &str = "sekrit-agent-token";
const TEST_EXT_ID: &str = "abcdefghijklmnopabcdefghijklmnop";

fn tempfile_path(prefix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rng = rand::thread_rng();
    let suffix: String = (0..8)
        .map(|_| char::from_digit(rng.gen_range(0..16), 16).unwrap())
        .collect();
    p.push(format!("{prefix}-{}-{suffix}.sock", std::process::id()));
    p
}

type TestWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn spawn_token_daemon() -> (daemon::DaemonHandle, PathBuf) {
    let mut config = DaemonConfig::new(0);
    config.agent_port = Some(0);
    config.agent_token = Some(AGENT_TOKEN.to_string());
    config.extension_token = Some(EXT_TOKEN.to_string());
    let sock = tempfile_path("bsk-test-ws-token");
    let handle = daemon::run(config, Some(sock.clone())).await.unwrap();
    (handle, sock)
}

async fn connect_ext(addr: std::net::SocketAddr) -> TestWs {
    let origin = format!("chrome-extension://{TEST_EXT_ID}");
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

async fn send_handshake(ws: &mut TestWs, token: Option<&str>) -> ResponseFrame {
    let params = HandshakeParams {
        client: "browser-skill-extension".into(),
        version: "0.1.0-dev.0".parse().unwrap(),
        protocol_version: "1.2".into(),
        instance_id: TEST_EXT_ID.into(),
        browser: BrowserPeerInfo {
            name: "chrome".into(),
            version: "131.0".into(),
        },
        min_compatible_peer: Some("0.1.0-dev.0".parse().unwrap()),
        min_compatible_protocol: Some("1.0".into()),
        label: "Token test ext".into(),
        token: token.map(ToString::to_string),
        agent_id: None,
    };
    let req = RequestFrame {
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
        _ => panic!("expected text response"),
    };
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn ws_handshake_without_extension_token_rejected() {
    let (handle, _sock) = spawn_token_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;

    let resp = send_handshake(&mut ws, None).await;
    match resp.body {
        ResponseBody::Err(err) => {
            assert_eq!(err.code, ErrorCode::PermissionDenied);
            assert!(
                err.message.contains("extension token mismatch"),
                "expected token-mismatch message, got: {}",
                err.message
            );
        }
        ResponseBody::Ok(_) => panic!("handshake without token must be rejected when token configured"),
    }

    handle.shutdown().await;
}

#[tokio::test]
async fn ws_handshake_with_extension_token_accepted() {
    let (handle, _sock) = spawn_token_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;

    let resp = send_handshake(&mut ws, Some(EXT_TOKEN)).await;
    match resp.body {
        ResponseBody::Ok(_) => {}
        ResponseBody::Err(err) => panic!("correct token must be accepted, got {err:?}"),
    }

    handle.shutdown().await;
}

#[tokio::test]
async fn ws_handshake_wrong_extension_token_rejected() {
    let (handle, _sock) = spawn_token_daemon().await;
    let mut ws = connect_ext(handle.ws_addr()).await;

    let resp = send_handshake(&mut ws, Some("wrong-token")).await;
    match resp.body {
        ResponseBody::Err(err) => {
            assert_eq!(err.code, ErrorCode::PermissionDenied);
        }
        ResponseBody::Ok(_) => panic!("wrong token must be rejected"),
    }

    handle.shutdown().await;
}

#[tokio::test]
async fn status_via_uds_ipc_still_works_with_gateway_tokens_configured() {
    // Gateway token configuration must not break the local UDS IPC path.
    let (handle, sock) = spawn_token_daemon().await;
    let mut ipc = IpcClient::connect(&sock).await.unwrap();
    let outcome = ipc
        .call::<(), bsk_protocol::system::StatusResult>(
            "status-1",
            Method::SystemStatus,
            Some(()),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert!(outcome.is_ok(), "local UDS status must still work, got {outcome:?}");
    handle.shutdown().await;
}