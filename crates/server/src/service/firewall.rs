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

    /// List all `block_list` rows that cover `server_id`, oldest first so the
    /// agent applies them in stable order.
    pub async fn list_for_server(
        &self,
        server_id: &str,
    ) -> Result<Vec<serverbee_common::firewall::BlockEntry>, AppError> {
        use sea_orm::{EntityTrait, QueryOrder};

        let rows = block_list::Entity::find()
            .order_by_asc(block_list::Column::CreatedAt)
            .all(&self.db)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if crate::service::alert::rule_covers_server(
                &row.cover_type,
                &row.server_ids_json,
                server_id,
            ) {
                out.push(serverbee_common::firewall::BlockEntry {
                    id: row.id,
                    target: row.target,
                    family: row.family as u8,
                });
            }
        }
        Ok(out)
    }

    /// Push a `BlocklistAdd` to every covered, online, capability-allowed
    /// agent whose negotiated protocol is high enough to understand firewall
    /// messages.
    pub async fn push_add_to_covered_agents(
        &self,
        row: &block_list::Model,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) {
        use serverbee_common::constants::{CAP_FIREWALL_BLOCK, has_capability};
        use serverbee_common::firewall::{BlockEntry, FIREWALL_MIN_PROTOCOL};
        use serverbee_common::protocol::ServerMessage;

        let entry = BlockEntry {
            id: row.id.clone(),
            target: row.target.clone(),
            family: row.family as u8,
        };

        for (server_id, protocol_version) in agent_manager.online_agents() {
            if !crate::service::alert::rule_covers_server(
                &row.cover_type,
                &row.server_ids_json,
                &server_id,
            ) {
                continue;
            }
            let caps = agent_manager
                .get_effective_capabilities(&server_id)
                .unwrap_or(0);
            if !has_capability(caps, CAP_FIREWALL_BLOCK) {
                continue;
            }
            if protocol_version < FIREWALL_MIN_PROTOCOL {
                continue;
            }
            if let Some(tx) = agent_manager.get_sender(&server_id) {
                let _ = tx
                    .send(ServerMessage::BlocklistAdd {
                        entry: entry.clone(),
                    })
                    .await;
            }
        }
    }

    /// Push a `BlocklistRemove` to every covered, online, capability-allowed
    /// agent on a high-enough protocol version.
    pub async fn push_remove_to_covered_agents(
        &self,
        row: &block_list::Model,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) {
        use serverbee_common::constants::{CAP_FIREWALL_BLOCK, has_capability};
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;
        use serverbee_common::protocol::ServerMessage;

        for (server_id, protocol_version) in agent_manager.online_agents() {
            if !crate::service::alert::rule_covers_server(
                &row.cover_type,
                &row.server_ids_json,
                &server_id,
            ) {
                continue;
            }
            let caps = agent_manager
                .get_effective_capabilities(&server_id)
                .unwrap_or(0);
            if !has_capability(caps, CAP_FIREWALL_BLOCK) {
                continue;
            }
            if protocol_version < FIREWALL_MIN_PROTOCOL {
                continue;
            }
            if let Some(tx) = agent_manager.get_sender(&server_id) {
                let _ = tx
                    .send(ServerMessage::BlocklistRemove { id: row.id.clone() })
                    .await;
            }
        }
    }

    /// Send the complete set of covering block entries to a single agent.
    /// Caller is responsible for verifying capability + protocol version.
    pub async fn push_sync_to(
        &self,
        server_id: &str,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) -> Result<(), AppError> {
        let entries = self.list_for_server(server_id).await?;
        if let Some(tx) = agent_manager.get_sender(server_id) {
            let _ = tx
                .send(serverbee_common::protocol::ServerMessage::BlocklistSync { entries })
                .await;
        }
        Ok(())
    }

    /// Tell an agent to drop every entry it currently holds and clear our
    /// `apply_state` map for that agent. Caller is responsible for verifying
    /// capability + protocol version.
    pub async fn push_reset_to(
        &self,
        server_id: &str,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) {
        if let Some(tx) = agent_manager.get_sender(server_id) {
            let _ = tx
                .send(serverbee_common::protocol::ServerMessage::BlocklistReset)
                .await;
        }
        let mut g = self.apply_state.write().await;
        g.retain(|(_block_id, srv), _| srv != server_id);
    }

    /// Update `apply_state`, write an audit row, and fan out a
    /// `FirewallApplyStateChanged` to subscribed browsers when an agent acks
    /// the apply of a single block entry.
    pub async fn record_ack(
        &self,
        server_id: &str,
        item: serverbee_common::firewall::BlocklistAckItem,
        db: &DatabaseConnection,
    ) {
        use serverbee_common::firewall::BlocklistEntryState;
        {
            let mut g = self.apply_state.write().await;
            g.insert(
                (item.id.clone(), server_id.to_string()),
                ApplyState {
                    state: item.state.clone(),
                    reason: item.reason.clone(),
                    at: chrono::Utc::now(),
                },
            );
        }
        let action = match item.state {
            BlocklistEntryState::Present => "firewall_block_applied_agent",
            BlocklistEntryState::Absent => "firewall_block_removed_agent",
            BlocklistEntryState::Failed => "firewall_block_rejected_agent",
        };
        let detail = serde_json::json!({
            "block_id": item.id,
            "server_id": server_id,
            "state": item.state,
            "reason": item.reason,
        })
        .to_string();
        if let Err(e) =
            crate::service::audit::AuditService::log(db, "system", action, Some(&detail), "").await
        {
            tracing::warn!(server_id, error = %e, "audit log for firewall ack failed");
        }
        let _ = self.browser_tx.send(
            serverbee_common::protocol::BrowserMessage::FirewallApplyStateChanged {
                block_id: item.id,
                server_id: server_id.to_string(),
                state: item.state,
                reason: item.reason,
            },
        );
    }

    /// Insert a `block_list` row from a matched security rule's
    /// `block_source_ip` action, push the new entry to covered agents, and
    /// audit/broadcast the change. Returns `Ok(Some(id))` on a fresh insert,
    /// `Ok(None)` when the target was already covered or hit a guardrail.
    pub async fn auto_block(
        &self,
        triggering_server_id: &str,
        rule: &crate::entity::alert_rule::Model,
        payload: &serverbee_common::security::SecurityEventPayload,
        event_id: &str,
        action: &crate::service::alert::AlertRuleAction,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) -> Result<Option<String>, AppError> {
        use crate::service::alert::AlertRuleAction;
        use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
        use uuid::Uuid;

        let (target, family) = Self::canonicalize_target(&payload.source_ip)?;

        let (cover_type, server_ids_json, comment_template) = match action {
            AlertRuleAction::BlockSourceIp {
                cover_type,
                server_ids_json,
                comment,
            } => (cover_type.clone(), server_ids_json.clone(), comment.clone()),
        };

        // Coverage-aware dedup. If an existing row already covers the
        // triggering server, the auto-block is genuinely redundant — silently
        // skip. If it exists but does NOT cover, audit a conflict and skip.
        if let Some(existing) = block_list::Entity::find()
            .filter(block_list::Column::Target.eq(&target))
            .one(&self.db)
            .await?
        {
            let covers = crate::service::alert::rule_covers_server(
                &existing.cover_type,
                &existing.server_ids_json,
                triggering_server_id,
            );
            if covers {
                return Ok(None);
            }
            crate::service::audit::AuditService::log(
                &self.db,
                "system",
                "firewall_auto_block_skipped_conflict",
                Some(
                    &serde_json::json!({
                        "target": target,
                        "existing_id": existing.id,
                        "current_server_id": triggering_server_id,
                        "rule_id": rule.id,
                        "event_id": event_id,
                    })
                    .to_string(),
                ),
                "",
            )
            .await
            .ok();
            return Ok(None);
        }

        let dynamic_allow = self.collect_dynamic_allow().await;
        let mut allow: Vec<String> = self.config.firewall.allow_list.clone();
        allow.extend(dynamic_allow);
        if let Some(reason) = Self::is_protected(&target, &allow) {
            crate::service::audit::AuditService::log(
                &self.db,
                "system",
                "firewall_block_rejected_server",
                Some(
                    &serde_json::json!({
                        "target": target,
                        "reason": reason,
                        "rule_id": rule.id,
                        "event_id": event_id,
                    })
                    .to_string(),
                ),
                "",
            )
            .await
            .ok();
            return Ok(None);
        }

        let id = Uuid::new_v4().to_string();
        let event_type_str = format!("{:?}", payload.event_type).to_lowercase();
        let severity_str = format!("{:?}", payload.severity).to_lowercase();
        let comment = comment_template
            .as_deref()
            .map(|t| {
                t.replace("{rule_name}", &rule.name)
                    .replace("{event_type}", &event_type_str)
                    .replace("{severity}", &severity_str)
            })
            .or_else(|| Some(format!("Auto-block from {}", rule.name)));

        let row = block_list::ActiveModel {
            id: Set(id.clone()),
            target: Set(target.clone()),
            family: Set(family as i32),
            cover_type: Set(cover_type),
            server_ids_json: Set(server_ids_json),
            comment: Set(comment),
            origin: Set(crate::service::alert::ORIGIN_AUTO.to_string()),
            origin_event_id: Set(Some(event_id.to_string())),
            origin_rule_id: Set(Some(rule.id.clone())),
            created_by: Set(None),
            created_at: Set(chrono::Utc::now()),
        }
        .insert(&self.db)
        .await?;

        crate::service::audit::AuditService::log(
            &self.db,
            "system",
            "firewall_block_created",
            Some(
                &serde_json::json!({
                    "id": row.id,
                    "target": row.target,
                    "origin": crate::service::alert::ORIGIN_AUTO,
                    "rule_id": rule.id,
                    "event_id": event_id,
                })
                .to_string(),
            ),
            "",
        )
        .await
        .ok();

        self.broadcast_changed_created(&row);
        self.push_add_to_covered_agents(&row, agent_manager).await;
        Ok(Some(row.id))
    }

    /// Audit a `BlocklistResetAck` from the agent. No `apply_state` mutation
    /// is needed because `push_reset_to` already cleared it locally.
    pub async fn record_reset_ack(
        &self,
        server_id: &str,
        ok: bool,
        reason: Option<String>,
        db: &DatabaseConnection,
    ) {
        let action = if ok {
            "firewall_reset_acked"
        } else {
            "firewall_reset_failed_agent"
        };
        let detail = serde_json::json!({
            "server_id": server_id,
            "ok": ok,
            "reason": reason,
        })
        .to_string();
        if let Err(e) =
            crate::service::audit::AuditService::log(db, "system", action, Some(&detail), "").await
        {
            tracing::warn!(server_id, error = %e, "audit log for firewall reset ack failed");
        }
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
