use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};

/// Returns `true` if `v4` falls within RFC 6598 Shared Address Space
/// (`100.64.0.0/10`), used by Tailscale (and Headscale) for Tailnet IPv4s.
pub fn is_cgnat_ipv4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    octets[0] == 100 && (octets[1] & 0b1100_0000) == 64
}

/// Returns `true` if `v6` falls within Tailscale's ULA prefix
/// (`fd7a:115c:a1e0::/48`).
pub fn is_tailscale_ipv6(v6: Ipv6Addr) -> bool {
    let seg = v6.segments();
    seg[0] == 0xfd7a && seg[1] == 0x115c && seg[2] == 0xa1e0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TailscaleNodeSummary {
    pub backend_state: String,
    pub tailscale_ips: Vec<String>,
    pub dns_name: Option<String>,
    pub tailnet_name: Option<String>,
    pub version: Option<String>,
    pub hostname: Option<String>,
    pub online: bool,
    pub exit_node: bool,
    pub exit_node_option: bool,
    pub key_expiry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TailscalePeerInfo {
    pub id: String,
    pub host_name: String,
    pub dns_name: Option<String>,
    pub os: Option<String>,
    pub tailscale_ips: Vec<String>,
    pub online: bool,
    pub active: bool,
    /// One of `"direct"`, `"derp"`, `"peer_relay"`, or `"idle"`.
    pub connection_mode: String,
    pub cur_addr: Option<String>,
    pub relay: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub last_handshake: Option<String>,
    pub last_seen: Option<String>,
    pub exit_node: bool,
    pub exit_node_option: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TailscaleStatusReport {
    pub summary: TailscaleNodeSummary,
    pub advertised_routes: Vec<String>,
    pub tags: Vec<String>,
    pub derp_region: Option<String>,
    pub peers: Vec<TailscalePeerInfo>,
    pub health_warnings: Vec<String>,
    pub collected_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cgnat_ipv4_boundaries() {
        assert!(!is_cgnat_ipv4("100.63.255.255".parse().unwrap()));
        assert!(is_cgnat_ipv4("100.64.0.0".parse().unwrap()));
        assert!(is_cgnat_ipv4("100.100.100.100".parse().unwrap()));
        assert!(is_cgnat_ipv4("100.127.255.255".parse().unwrap()));
        assert!(!is_cgnat_ipv4("100.128.0.0".parse().unwrap()));
        assert!(!is_cgnat_ipv4("192.168.1.1".parse().unwrap()));
        assert!(!is_cgnat_ipv4("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn tailscale_ipv6_boundaries() {
        assert!(is_tailscale_ipv6("fd7a:115c:a1e0::1".parse().unwrap()));
        assert!(is_tailscale_ipv6(
            "fd7a:115c:a1e0:ab12:4843:cd96:626b:6464".parse().unwrap()
        ));
        assert!(!is_tailscale_ipv6("fd7a:115c:a1e1::1".parse().unwrap()));
        assert!(!is_tailscale_ipv6("fd00::1".parse().unwrap()));
        assert!(!is_tailscale_ipv6("fe80::1".parse().unwrap()));
        assert!(!is_tailscale_ipv6("2001:4860:4860::8888".parse().unwrap()));
    }

    #[test]
    fn tailscale_status_report_serde_round_trip() {
        let report = TailscaleStatusReport {
            summary: TailscaleNodeSummary {
                backend_state: "Running".into(),
                tailscale_ips: vec!["100.64.0.10".into(), "fd7a:115c:a1e0::10".into()],
                dns_name: Some("node-a.tailnet.ts.net".into()),
                tailnet_name: Some("tailnet.ts.net".into()),
                version: Some("1.76.1".into()),
                hostname: Some("node-a".into()),
                online: true,
                exit_node: false,
                exit_node_option: true,
                key_expiry: Some("2027-01-01T00:00:00Z".into()),
            },
            advertised_routes: vec!["10.0.0.0/24".into()],
            tags: vec!["tag:prod".into()],
            derp_region: Some("tok".into()),
            peers: vec![TailscalePeerInfo {
                id: "n123".into(),
                host_name: "node-b".into(),
                dns_name: Some("node-b.tailnet.ts.net".into()),
                os: Some("linux".into()),
                tailscale_ips: vec!["100.64.0.11".into()],
                online: true,
                active: true,
                connection_mode: "direct".into(),
                cur_addr: Some("203.0.113.5:41641".into()),
                relay: Some("tok".into()),
                rx_bytes: 1024,
                tx_bytes: 2048,
                last_handshake: Some("2026-11-21T03:00:00Z".into()),
                last_seen: None,
                exit_node: false,
                exit_node_option: false,
                tags: vec!["tag:peer".into()],
            }],
            health_warnings: vec![],
            collected_at: "2026-11-21T03:35:00Z".into(),
        };

        let json = serde_json::to_string(&report).unwrap();
        let decoded: TailscaleStatusReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, report);
    }
}
