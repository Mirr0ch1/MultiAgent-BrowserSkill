//! Gateway auto-listen: pick a non-loopback interface (Tailscale first,
//! then any interface whose address falls inside the configured LAN
//! CIDRs). P2 audit fix — `--gateway` should default to a LAN-reachable
//! address, not silently stay on loopback.
//!
//! Open-source friendly: no networks are hard-coded; the probe honours
//! `--lan-cidr` / `BSK_LAN_CIDRS`.

use std::net::IpAddr;

use crate::cidr::Cidr4;

/// Return a preferred gateway listen address, or `None` when the host
/// has no suitable interface (caller falls back to loopback).
///
/// Preference:
///   1. the `tailscale*` interface address (Tailscale CGNAT),
///   2. the first non-loopback interface whose IPv4 is inside `lan_cidrs`
///      (when non-empty),
///   3. the first non-loopback interface address,
///   4. `None`.
pub fn probe_gateway_listen(lan_cidrs: &[Cidr4]) -> Option<IpAddr> {
    let Ok(ifaddrs) = if_addrs::get_if_addrs() else {
        return None;
    };

    let mut tailscale: Option<IpAddr> = None;
    let mut in_lan: Option<IpAddr> = None;
    let mut any_non_loopback: Option<IpAddr> = None;

    for ifa in ifaddrs {
        if ifa.is_loopback() {
            continue;
        }
        let ip = ifa.ip();
        if ip.is_loopback() {
            continue;
        }
        if ifa.name.starts_with("tailscale") && tailscale.is_none() {
            tailscale = Some(ip);
            continue;
        }
        if let IpAddr::V4(v4) = ip {
            if !lan_cidrs.is_empty() && crate::cidr::any_contains(lan_cidrs, v4) && in_lan.is_none() {
                in_lan = Some(ip);
                continue;
            }
        }
        if any_non_loopback.is_none() {
            any_non_loopback = Some(ip);
        }
    }

    tailscale.or(in_lan).or(any_non_loopback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cidr::Cidr4;

    #[test]
    fn parses_tailscale_and_lan_cidrs() {
        let ts = Cidr4::parse("100.64.0.0/10").unwrap();
        assert!(ts.contains("100.64.10.5".parse().unwrap()));
        assert!(!ts.contains("192.168.10.5".parse().unwrap()));
        let lan = Cidr4::parse("192.168.10.0/24").unwrap();
        assert!(lan.contains("192.168.10.99".parse().unwrap()));
        assert!(!lan.contains("192.168.30.1".parse().unwrap()));
    }
}
