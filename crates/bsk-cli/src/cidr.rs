//! LAN CIDR allow-list (production LAN scoping, P2 audit fix).
//!
//! The gateway is bound on a LAN interface (Tailscale / 192.168.x /
//! SD-WAN). `DaemonConfig.lan_cidrs` optionally scopes which source
//! networks may talk to the daemon at all; when empty the daemon keeps
//! upstream behaviour (any reachable peer, token-gated). This module is
//! a tiny zero-dependency IPv4 CIDR matcher — deliberately no `ipnet`
//! crate so the fork stays dependency-light.
//!
//! Open-source friendly: nothing is hard-coded. Operators configure the
//! networks they actually use (`--lan-cidr 100.64.0.0/10` for
//! Tailscale CGNAT, `192.168.10.0/24`, …), possibly via `BSK_LAN_CIDRS`.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};

/// A parsed IPv4 CIDR (`a.b.c.d/prefix`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr4 {
    pub network: u32,
    pub prefix: u8,
}

impl fmt::Display for Cidr4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", Ipv4Addr::from(self.network), self.prefix)
    }
}

impl Cidr4 {
    /// Parse `a.b.c.d/N`. Returns `None` on malformed input (callers
    /// surface their own error message).
    pub fn parse(s: &str) -> Option<Self> {
        let (ip_part, prefix_part) = s.trim().split_once('/')?;
        let prefix: u8 = prefix_part.parse().ok()?;
        if prefix > 32 {
            return None;
        }
        let addr: Ipv4Addr = ip_part.trim().parse().ok()?;
        let raw = u32::from(addr);
        let mask = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix)
        };
        Some(Self {
            network: raw & mask,
            prefix,
        })
    }

    /// Does this CIDR contain `ip`?
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        let raw = u32::from(ip);
        let mask = if self.prefix == 0 {
            0
        } else {
            u32::MAX << (32 - self.prefix)
        };
        (raw & mask) == self.network
    }
}

/// Parse a comma-separated CIDR list (flag repetition or
/// `BSK_LAN_CIDRS` env). Malformed entries are skipped; returning the
/// valid subset keeps config hardening from failing the whole boot.
pub fn parse_list(value: &str) -> Vec<Cidr4> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(Cidr4::parse)
        .collect()
}

/// True when `ip` matches any CIDR in `list`.
pub fn any_contains(list: &[Cidr4], ip: Ipv4Addr) -> bool {
    list.iter().any(|c| c.contains(ip))
}

/// LAN-scope gate for a remote peer address.
///
/// - empty `lan_cidrs` → unrestricted (falls back to token gating)
/// - loopback (127.x / ::1) → always allowed (local daemon CLI / same-host)
/// - IPv4 → matched against the V4 CIDR list
/// - IPv4-mapped IPv6 (`::ffff:a.b.c.d`) → unmapped and matched as V4
/// - any other IPv6 → rejected (the whitelist is IPv4-only)
pub fn ip_allowed(list: &[Cidr4], ip: IpAddr) -> bool {
    if list.is_empty() {
        return true;
    }
    match ip {
        IpAddr::V4(v4) if v4.is_loopback() => true,
        IpAddr::V4(v4) => any_contains(list, v4),
        IpAddr::V6(v6) if v6.is_loopback() => true,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => any_contains(list, v4),
            None => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_matches_tailscale_cgnat() {
        let c = Cidr4::parse("100.64.0.0/10").unwrap();
        assert!(c.contains("100.64.0.1".parse().unwrap()));
        assert!(c.contains("100.127.255.254".parse().unwrap()));
        assert!(!c.contains("100.128.0.1".parse().unwrap()));
        assert!(!c.contains("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn parses_192_168_subnets() {
        let c10 = Cidr4::parse("192.168.10.0/24").unwrap();
        assert!(c10.contains("192.168.10.99".parse().unwrap()));
        assert!(!c10.contains("192.168.30.1".parse().unwrap()));
        let c30 = Cidr4::parse("192.168.30.0/24").unwrap();
        assert!(c30.contains("192.168.30.5".parse().unwrap()));
    }

    #[test]
    fn host_route_prefix_32() {
        let c = Cidr4::parse("10.0.0.5/32").unwrap();
        assert!(c.contains("10.0.0.5".parse().unwrap()));
        assert!(!c.contains("10.0.0.6".parse().unwrap()));
    }

    #[test]
    fn prefix_zero_means_everything() {
        let c = Cidr4::parse("0.0.0.0/0").unwrap();
        assert!(c.contains("192.168.1.1".parse().unwrap()));
        assert!(c.contains("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn malformed_entries_are_skipped() {
        let list = parse_list("100.64.0.0/10, garbage, 192.168.10.0/24");
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|c| c.contains("192.168.10.99".parse().unwrap())));
    }

    #[test]
    fn any_contains_works_across_list() {
        let list = parse_list("192.168.10.0/24, 192.168.50.0/24");
        assert!(any_contains(&list, "192.168.50.7".parse().unwrap()));
        assert!(!any_contains(&list, "192.168.30.7".parse().unwrap()));
    }

    #[test]
    fn ip_allowed_loopback_always_allowed() {
        let list = parse_list("192.168.10.0/24");
        // V4 loopback
        assert!(ip_allowed(&list, "127.0.0.1".parse().unwrap()));
        // V6 loopback (F2 audit fix)
        assert!(ip_allowed(&list, "::1".parse().unwrap()));
    }

    #[test]
    fn ip_allowed_empty_list_is_unrestricted() {
        let empty: Vec<Cidr4> = Vec::new();
        assert!(ip_allowed(&empty, "203.0.113.9".parse().unwrap()));
        assert!(ip_allowed(&empty, "2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn ip_allowed_v4_in_and_out() {
        let list = parse_list("192.168.10.0/24");
        assert!(ip_allowed(&list, "192.168.10.55".parse().unwrap()));
        assert!(!ip_allowed(&list, "192.168.30.55".parse().unwrap()));
    }

    #[test]
    fn ip_allowed_ipv4_mapped_v6_unmaps_to_v4() {
        // ::ffff:192.168.10.7 is a dual-stack IPv4 peer (F3 audit fix).
        let list = parse_list("192.168.10.0/24");
        assert!(ip_allowed(&list, "::ffff:192.168.10.7".parse().unwrap()));
        assert!(!ip_allowed(&list, "::ffff:192.168.30.7".parse().unwrap()));
    }

    #[test]
    fn ip_allowed_pure_v6_rejected_when_restricted() {
        let list = parse_list("192.168.10.0/24");
        assert!(!ip_allowed(&list, "2001:db8::1".parse().unwrap()));
    }
}