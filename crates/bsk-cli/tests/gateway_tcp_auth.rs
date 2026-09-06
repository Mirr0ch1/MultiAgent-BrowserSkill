//! M2: TCP IPC authentication (gateway remote CLI peers).
//!
//! Covers the P0 auth contract: the TCP transport is network-exposed, so
//! every connection must authenticate with a first-frame
//! `system.handshake` carrying the configured agent token. Wrong or
//! missing tokens are rejected; a valid token completes the handshake
//! and can issue business RPCs.

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use bsk::daemon::{self, DaemonConfig};
use bsk::ipc_client::tcp::TcpClient;
use bsk_protocol::{Method, StatusParams, StatusResult};
use rand::Rng;

const AGENT_TOKEN: &str = "sekrit-agent-token";

fn tempfile_path(prefix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let mut rng = rand::thread_rng();
    let suffix: String = (0..8)
        .map(|_| char::from_digit(rng.gen_range(0..16), 16).unwrap())
        .collect();
    p.push(format!("{prefix}-{}-{suffix}.sock", std::process::id()));
    p
}

/// Spawn a gateway-configured daemon: WS (loopback, random port) + UDS
/// IPC + TCP IPC on a random port with `AGENT_TOKEN`.
async fn spawn_gateway_daemon() -> (daemon::DaemonHandle, PathBuf) {
    let mut config = DaemonConfig::new(0);
    config.agent_port = Some(0); // random port, read back via handle.tcp_addr()
    config.agent_token = Some(AGENT_TOKEN.to_string());
    let sock = tempfile_path("bsk-test-gateway-auth");
    let handle = daemon::run(config, Some(sock.clone())).await.unwrap();
    (handle, sock)
}

fn tcp_addr(handle: &daemon::DaemonHandle) -> std::net::SocketAddr {
    handle
        .tcp_addr()
        .expect("gateway daemon must expose a TCP IPC listener")
}

#[tokio::test]
async fn tcp_ipc_valid_token_completes_handshake_and_status() {
    let (handle, _sock) = spawn_gateway_daemon().await;
    let addr = tcp_addr(&handle);

    let mut client = TcpClient::connect(
        Ipv4Addr::LOCALHOST.into(),
        addr.port(),
        Some(AGENT_TOKEN),
    )
    .await
    .expect("valid agent token must authenticate");

    let outcome = client
        .call::<_, StatusResult>(
            Method::SystemStatus,
            &StatusParams::default(),
            Duration::from_secs(2),
        )
        .await
        .expect("status RPC over authenticated TCP IPC must succeed");
    assert!(outcome.is_ok(), "status should return Ok, got {outcome:?}");

    handle.shutdown().await;
}

#[tokio::test]
async fn tcp_ipc_wrong_token_rejected() {
    let (handle, _sock) = spawn_gateway_daemon().await;
    let addr = tcp_addr(&handle);

    let result =
        TcpClient::connect(Ipv4Addr::LOCALHOST.into(), addr.port(), Some("totally-wrong-token"))
            .await;
    let err = match result {
        Ok(_) => panic!("wrong token must be rejected at handshake"),
        Err(err) => err,
    };
    let text = format!("{err:#}");
    assert!(
        text.contains("handshake rejected") || text.contains("invalid token"),
        "expected auth-failure message, got: {text}"
    );

    handle.shutdown().await;
}

#[tokio::test]
async fn tcp_ipc_missing_token_rejected() {
    let (handle, _sock) = spawn_gateway_daemon().await;
    let addr = tcp_addr(&handle);

    let result = TcpClient::connect(Ipv4Addr::LOCALHOST.into(), addr.port(), None).await;
    let err = match result {
        Ok(_) => panic!("missing token must be rejected at handshake"),
        Err(err) => err,
    };
    let text = format!("{err:#}");
    assert!(
        text.contains("handshake rejected") || text.contains("invalid token"),
        "expected auth-failure message, got: {text}"
    );

    handle.shutdown().await;
}