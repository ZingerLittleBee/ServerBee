//! Firewall blocklist service. Holds the canonicalization, guardrail, and
//! agent-apply-state logic. CRUD wiring lives in `router::api::firewall`;
//! WS push is invoked from there and from auto-block (`service::security`).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, LazyLock};

use chrono::{DateTime, Utc};
use ipnet::IpNet;
use sea_orm::DatabaseConnection;
use serverbee_common::firewall::BlocklistEntryState;
use tokio::sync::{RwLock, broadcast};

use crate::config::AppConfig;
use crate::entity::block_list;
use crate::error::AppError;

/// `(block_id, server_id) → ApplyState` derived from acks since boot.
pub type ApplyStateMap = Arc<RwLock<HashMap<(String, String), ApplyState>>>;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ApplyState {
    pub state: BlocklistEntryState,
    pub reason: Option<String>,
    pub at: DateTime<Utc>,
}

/// Hard-coded protected CIDRs (tier 1).
const PROTECTED_CIDRS: &[&str] = &[
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "0.0.0.0/8",
    "224.0.0.0/4",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
    "ff00::/8",
    "::/128",
];

/// Parsed form of [`PROTECTED_CIDRS`]. Built once on first access so the hot
/// path through [`FirewallService::is_protected`] does not re-parse the 12
/// hard-coded strings on every call. Stable since Rust 1.80.
static PROTECTED_NETS: LazyLock<Vec<IpNet>> = LazyLock::new(|| {
    PROTECTED_CIDRS
        .iter()
        .map(|s| s.parse().expect("hard-coded valid CIDR"))
        .collect()
});

#[allow(dead_code)]
pub struct FirewallService {
    pub(crate) db: DatabaseConnection,
    pub(crate) config: Arc<AppConfig>,
    pub(crate) apply_state: ApplyStateMap,
    /// Each connected agent's external IP, populated by the agent WS handler
    /// when a `SystemInfo` or `IpChanged` arrives. Keyed by `server_id` so we
    /// can update an agent's reported IP in place and drop it on disconnect.
    pub(crate) external_ips: Arc<RwLock<HashMap<String, IpAddr>>>,
    /// BrowserMessage broadcast handle — re-uses the existing `AppState.browser_tx`.
    pub(crate) browser_tx: broadcast::Sender<serverbee_common::protocol::BrowserMessage>,
}

impl FirewallService {
    /// Construct a `FirewallService` with internally-managed `apply_state` and
    /// `external_ips` maps. Callers only supply the externally-owned dependencies
    /// (`db`, `config`, `browser_tx`), keeping the apply-state invariants encapsulated.
    pub fn new(
        db: DatabaseConnection,
        config: Arc<AppConfig>,
        browser_tx: broadcast::Sender<serverbee_common::protocol::BrowserMessage>,
    ) -> Self {
        Self {
            db,
            config,
            apply_state: Arc::new(RwLock::new(HashMap::new())),
            external_ips: Arc::new(RwLock::new(HashMap::new())),
            browser_tx,
        }
    }

    /// Parse and canonicalize a client-supplied target.
    /// Returns `(target_canonical, family)` where `family` is 4 or 6.
    pub fn canonicalize_target(input: &str) -> Result<(String, u8), AppError> {
        // Try IpAddr first → /32 or /128 CIDR.
        if let Ok(addr) = input.parse::<IpAddr>() {
            let net = IpNet::new(addr, if addr.is_ipv4() { 32 } else { 128 })
                .expect("prefix is valid");
            let family = if addr.is_ipv4() { 4 } else { 6 };
            return Ok((net.to_string(), family));
        }
        // Then IpNet.
        let net: IpNet = input
            .parse()
            .map_err(|_| AppError::BadRequest(format!("invalid IP or CIDR: {input}")))?;
        let canonical =
            IpNet::new(net.network(), net.prefix_len()).expect("network ok");
        let family = match canonical {
            IpNet::V4(_) => 4,
            IpNet::V6(_) => 6,
        };
        Ok((canonical.to_string(), family))
    }

    /// Returns `Some(reason)` if `target_cidr` overlaps a protected range
    /// (tier 1 hard-coded) or any entry in `extra_allow` (tier 2 server config
    /// + tier-2.5 runtime allow set such as agent external IPs).
    pub fn is_protected(target_cidr: &str, extra_allow: &[String]) -> Option<String> {
        let target: IpNet = match target_cidr.parse() {
            Ok(n) => n,
            Err(_) => return Some("invalid CIDR".into()),
        };
        for prot in PROTECTED_NETS.iter() {
            if Self::overlaps(&target, prot) {
                return Some(format!("hits hard-coded guardrail: {prot}"));
            }
        }
        for raw in extra_allow {
            if let Ok(prot) = raw.parse::<IpNet>()
                && Self::overlaps(&target, &prot)
            {
                return Some(format!("hits allow_list: {raw}"));
            }
            if let Ok(addr) = raw.parse::<IpAddr>() {
                let prot = IpNet::new(addr, if addr.is_ipv4() { 32 } else { 128 })
                    .expect("prefix ok");
                if Self::overlaps(&target, &prot) {
                    return Some(format!("hits allow_list: {raw}"));
                }
            }
        }
        None
    }

    fn overlaps(a: &IpNet, b: &IpNet) -> bool {
        // overlap iff a contains b's network or b contains a's network
        a.contains(&b.network()) || b.contains(&a.network())
    }

    /// Tier-2.5 runtime allow-list. Merges `[server] trusted_proxies` with
    /// each connected agent's most recently reported external IP, both of
    /// which we must never accidentally block.
    pub async fn collect_dynamic_allow(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .config
            .server
            .trusted_proxies
            .iter()
            .map(|n| n.to_string())
            .collect();
        let g = self.external_ips.read().await;
        for ip in g.values() {
            out.push(ip.to_string());
        }
        out
    }

    /// Record/update/clear an agent's last reported external IP.
    /// Called from the agent WS handler on `SystemInfo` and `IpChanged`.
    pub async fn note_agent_external_ip(&self, server_id: &str, ip: Option<IpAddr>) {
        let mut g = self.external_ips.write().await;
        match ip {
            Some(ip) => {
                g.insert(server_id.to_string(), ip);
            }
            None => {
                g.remove(server_id);
            }
        }
    }

    /// Broadcast a `BlocklistChanged { kind: Created }` to subscribed browsers.
    pub fn broadcast_changed_created(&self, row: &block_list::Model) {
        let _ = self.browser_tx.send(
            serverbee_common::protocol::BrowserMessage::BlocklistChanged {
                kind: serverbee_common::firewall::BlocklistChangeKind::Created,
                block_id: row.id.clone(),
                target: row.target.clone(),
            },
        );
    }

    /// Broadcast a `BlocklistChanged { kind: Deleted }` to subscribed browsers.
    pub fn broadcast_changed_deleted(&self, row: &block_list::Model) {
        let _ = self.browser_tx.send(
            serverbee_common::protocol::BrowserMessage::BlocklistChanged {
                kind: serverbee_common::firewall::BlocklistChangeKind::Deleted,
                block_id: row.id.clone(),
                target: row.target.clone(),
            },
        );
    }

    /// Push a `BlocklistAdd` to every covered, online agent. No-op until
    /// Task 2.x wires `agent_manager` into this service.
    pub async fn push_add_to_covered_agents(&self, _row: &block_list::Model) {
        // Task 2.x: resolve covered server_ids (cover_type + server_ids_json on
        // the row) → look up agent senders on AgentManager → emit
        // ServerMessage::BlocklistAdd { entry: BlockEntry }.
    }

    /// Push a `BlocklistRemove` to every covered, online agent. No-op until
    /// Task 2.x wires `agent_manager` into this service.
    pub async fn push_remove_to_covered_agents(&self, _row: &block_list::Model) {
        // Task 2.x.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bare_ipv4() {
        let (t, f) = FirewallService::canonicalize_target("1.2.3.4").unwrap();
        assert_eq!(t, "1.2.3.4/32");
        assert_eq!(f, 4);
    }

    #[test]
    fn canonical_cidr_strips_host_bits() {
        let (t, _) = FirewallService::canonicalize_target("1.2.3.4/24").unwrap();
        assert_eq!(t, "1.2.3.0/24");
    }

    #[test]
    fn canonical_ipv6_lowercases_and_collapses() {
        let (t, f) = FirewallService::canonicalize_target("001:0db8::/32").unwrap();
        assert_eq!(t, "1:db8::/32");
        assert_eq!(f, 6);
    }

    #[test]
    fn canonical_rejects_garbage() {
        assert!(FirewallService::canonicalize_target("not-an-ip").is_err());
    }

    #[test]
    fn protected_loopback() {
        assert!(FirewallService::is_protected("127.0.0.1/32", &[]).is_some());
    }

    #[test]
    fn protected_rfc1918() {
        assert!(FirewallService::is_protected("10.5.0.0/16", &[]).is_some());
    }

    #[test]
    fn protected_external_is_not() {
        assert!(FirewallService::is_protected("203.0.113.5/32", &[]).is_none());
    }

    #[test]
    fn protected_target_supersets_protected() {
        // 0.0.0.0/0 contains 127.0.0.0/8 → reject
        assert!(FirewallService::is_protected("0.0.0.0/0", &[]).is_some());
    }

    #[test]
    fn protected_allow_list_matches() {
        let allow = vec!["203.0.113.0/24".to_string()];
        assert!(FirewallService::is_protected("203.0.113.5/32", &allow).is_some());
    }

    #[test]
    fn protected_allow_list_bare_ip() {
        let allow = vec!["203.0.113.5".to_string()];
        assert!(FirewallService::is_protected("203.0.113.5/32", &allow).is_some());
    }

    #[test]
    fn protected_ipv6_loopback() {
        assert!(FirewallService::is_protected("::1/128", &[]).is_some());
    }

    #[test]
    fn protected_ipv6_ula() {
        // fc00::/7 covers fc00::/8 and fd00::/8 — the RFC4193 unique-local range.
        assert!(FirewallService::is_protected("fc00::/7", &[]).is_some());
    }
}
