//! M2: gateway runtime guards.
//!
//! 1. Interlock — a daemon start that would bind a non-loopback address
//!    without a configured token must be refused before touching the
//!    network (constructive safety, P0).
//! 2. Remote short-circuit — in `--host` (remote gateway) mode the CLI
//!    must never auto-spawn a local daemon; it reports the misconfig
//!    instead.

#![cfg(unix)]

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::Command;

fn bsk_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_bsk"))
}

#[test]
fn interlock_refuses_non_loopback_listen_without_token() {
    // 192.168.x.x is a real routable LAN address on this machine; the
    // interlock fires before any bind, so nothing is actually opened.
    let out = Command::new(bsk_bin())
        .args([
            "daemon",
            "start",
            "--listen",
            "192.168.10.99",
            "--foreground",
            "--port",
            "0",
        ])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "daemon must refuse non-loopback bind without token"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("refusing to bind non-loopback"),
        "expected interlock message, got stderr: {stderr}"
    );
}

#[test]
fn gateway_flag_without_token_also_refused() {
    let out = Command::new(bsk_bin())
        .args([
            "daemon",
            "start",
            "--gateway",
            "--listen",
            "192.168.10.99",
            "--foreground",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("refusing to bind non-loopback"),
        "expected interlock message, got stderr: {stderr}"
    );
}

#[tokio::test]
async fn remote_mode_short_circuits_auto_spawn() {
    // A remote-mode CLI must never fall back to spawning a local daemon.
    bsk::cli::init_global_flags(bsk::cli::GlobalFlags {
        host: Some(Ipv4Addr::LOCALHOST.into()),
        port: Some(52999),
        agent_token: Some("whatever".into()),
        ..Default::default()
    });
    let err = bsk::cli::ensure_daemon::ensure_daemon().unwrap_err();
    let text = format!("{err:#}");
    assert!(
        text.contains("remote gateway mode") && text.contains("auto-spawn disabled")
            || text.contains("auto-spawn"),
        "expected remote-mode short-circuit message, got: {text}"
    );
}