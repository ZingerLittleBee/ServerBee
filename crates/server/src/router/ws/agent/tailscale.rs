//! Tailscale status message handling: cache updates, DB persistence of primary
//! Tailscale IPv4/IPv6/DNS/Tailnet fields, catalog broadcast, and correlated
//! HTTP request/reply dispatch.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use serverbee_common::constants::{CAP_TAILSCALE, has_capability};
use serverbee_common::protocol::{AgentMessage, BrowserMessage};
use serverbee_common::tailscale_types::{
    TailscaleNodeSummary, TailscaleStatusReport, is_cgnat_ipv4, is_tailscale_ipv6,
};

use crate::service::capability_gate::effective_capabilities;
use crate::service::server::ServerService;
use crate::state::AppState;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ExtractedTailscaleFields {
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub dns_name: Option<String>,
    pub tailnet: Option<String>,
}

/// Extract primary Tailscale IPv4 (`100.64.0.0/10` preferred, else first IPv4),
/// IPv6 (`fd7a:115c:a1e0::/48` preferred, else first IPv6), normalized MagicDNS
/// name, and Tailnet name from a node summary.
pub(crate) fn extract_tailscale_fields(summary: &TailscaleNodeSummary) -> ExtractedTailscaleFields {
    let parsed_v4: Vec<Ipv4Addr> = summary
        .tailscale_ips
        .iter()
        .filter_map(|raw| raw.trim().parse::<Ipv4Addr>().ok())
        .collect();
    let ipv4 = parsed_v4
        .iter()
        .copied()
        .find(|ip| is_cgnat_ipv4(*ip))
        .or_else(|| parsed_v4.first().copied())
        .map(|ip| ip.to_string());

    let parsed_v6: Vec<Ipv6Addr> = summary
        .tailscale_ips
        .iter()
        .filter_map(|raw| raw.trim().parse::<Ipv6Addr>().ok())
        .collect();
    let ipv6 = parsed_v6
        .iter()
        .copied()
        .find(|ip| is_tailscale_ipv6(*ip))
        .or_else(|| parsed_v6.first().copied())
        .map(|ip| ip.to_string());

    let dns_name = summary
        .dns_name
        .as_deref()
        .map(|s| s.trim().trim_end_matches('.').to_string())
        .filter(|s| !s.is_empty());

    let tailnet = summary
        .tailnet_name
        .as_deref()
        .map(|s| s.trim().trim_end_matches('.').to_string())
        .filter(|s| !s.is_empty());

    ExtractedTailscaleFields {
        ipv4,
        ipv6,
        dns_name,
        tailnet,
    }
}

pub(super) async fn on_tailscale_status(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: Option<&String>,
    status: &TailscaleStatusReport,
    msg: &AgentMessage,
) {
    let caps = effective_capabilities(state, server_id).await;
    if has_capability(caps, CAP_TAILSCALE) {
        let extracted = extract_tailscale_fields(&status.summary);
        match ServerService::update_tailscale_info(
            &state.db,
            server_id,
            extracted.ipv4.as_deref(),
            extracted.ipv6.as_deref(),
            extracted.dns_name.as_deref(),
            extracted.tailnet.as_deref(),
        )
        .await
        {
            Ok(true) => {
                state
                    .agent_manager
                    .broadcast_browser(BrowserMessage::ServerCatalogChanged {
                        server_ids: vec![server_id.to_string()],
                    });
            }
            Ok(false) => {}
            Err(error) => {
                tracing::warn!("Failed to persist Tailscale summary for {server_id}: {error}");
            }
        }
        state
            .agent_manager
            .update_tailscale_status(server_id, status.clone());
    }

    if let Some(msg_id) = msg_id {
        state
            .agent_manager
            .dispatch_pending_response(server_id, msg_id, msg.clone());
    }
}

pub(super) fn on_tailscale_unavailable(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: Option<&String>,
    msg: &AgentMessage,
) {
    state.agent_manager.clear_tailscale_status(server_id);
    if let Some(msg_id) = msg_id {
        state
            .agent_manager
            .dispatch_pending_response(server_id, msg_id, msg.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(
        ips: &[&str],
        dns_name: Option<&str>,
        tailnet_name: Option<&str>,
    ) -> TailscaleNodeSummary {
        TailscaleNodeSummary {
            backend_state: "Running".to_string(),
            tailscale_ips: ips.iter().map(|s| (*s).to_string()).collect(),
            dns_name: dns_name.map(str::to_string),
            tailnet_name: tailnet_name.map(str::to_string),
            version: Some("1.76.1".to_string()),
            hostname: Some("node-1".to_string()),
            online: true,
            exit_node: false,
            exit_node_option: false,
            key_expiry: None,
        }
    }

    #[test]
    fn prefers_cgnat_ipv4_and_tailscale_ipv6() {
        let s = summary(
            &[
                "192.168.1.50",
                "100.101.102.103",
                "fe80::1",
                "fd7a:115c:a1e0::42",
            ],
            Some("node-1.tailnet.ts.net."),
            Some("tailnet.ts.net."),
        );
        let extracted = extract_tailscale_fields(&s);
        assert_eq!(extracted.ipv4.as_deref(), Some("100.101.102.103"));
        assert_eq!(extracted.ipv6.as_deref(), Some("fd7a:115c:a1e0::42"));
        assert_eq!(extracted.dns_name.as_deref(), Some("node-1.tailnet.ts.net"));
        assert_eq!(extracted.tailnet.as_deref(), Some("tailnet.ts.net"));
    }

    #[test]
    fn falls_back_to_first_v4_and_v6_for_custom_headscale_prefixes() {
        let s = summary(
            &["10.99.0.5", "fd00:dead:beef::5"],
            Some("  box.headscale.internal  "),
            Some("  default  "),
        );
        let extracted = extract_tailscale_fields(&s);
        assert_eq!(extracted.ipv4.as_deref(), Some("10.99.0.5"));
        assert_eq!(extracted.ipv6.as_deref(), Some("fd00:dead:beef::5"));
        assert_eq!(
            extracted.dns_name.as_deref(),
            Some("box.headscale.internal")
        );
        assert_eq!(extracted.tailnet.as_deref(), Some("default"));
    }

    #[test]
    fn handles_empty_and_invalid_entries() {
        let s = summary(&["not-an-ip", ""], Some("."), Some("   "));
        let extracted = extract_tailscale_fields(&s);
        assert_eq!(extracted, ExtractedTailscaleFields::default());
    }
}
