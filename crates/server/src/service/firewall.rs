//! Firewall blocklist service. Holds the canonicalization, guardrail, and
//! agent-apply-state logic. CRUD wiring lives in `router::api::firewall`;
//! WS push is invoked from there and from auto-block (`service::security`).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, LazyLock};

use chrono::{DateTime, Utc};
use ipnet::IpNet;
use sea_orm::DatabaseConnection;
use serverbee_common::firewall::{BlocklistEntryState, PROTECTED_CIDRS};
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
        let event_type_str = crate::service::security::event_type_to_str(payload.event_type);
        let severity_str = crate::service::security::severity_to_str(payload.severity);
        let comment = comment_template
            .as_deref()
            .map(|t| {
                t.replace("{rule_name}", &rule.name)
                    .replace("{event_type}", event_type_str)
                    .replace("{severity}", severity_str)
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

    // ── auto_block coverage-aware dedup ──

    use crate::config::AppConfig;
    use crate::entity::{alert_rule, audit_log, block_list};
    use crate::service::agent_manager::AgentManager;
    use crate::service::alert::AlertRuleAction;
    use crate::test_utils::setup_test_db;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use serverbee_common::security::{
        DetectorSource, SecurityEventPayload, SecurityEventType, SecurityEvidence, Severity,
    };

    fn make_payload(source_ip: &str) -> SecurityEventPayload {
        SecurityEventPayload {
            event_type: SecurityEventType::SshBruteForce,
            severity: Severity::High,
            source_ip: source_ip.to_string(),
            source_port: None,
            username: None,
            started_at: 1_700_000_000,
            ended_at: 1_700_000_060,
            first_seen: false,
            detector_source: DetectorSource::Journal,
            evidence: SecurityEvidence::SshBruteForce {
                failed_count: 47,
                distinct_users: 3,
                sample_users: vec!["root".into()],
                invalid_user_count: 8,
                window_seconds: 60,
                threshold: 10,
            },
        }
    }

    fn make_rule(id: &str) -> alert_rule::Model {
        let now = chrono::Utc::now();
        alert_rule::Model {
            id: id.to_string(),
            name: "test-rule".to_string(),
            enabled: true,
            rules_json: "[]".to_string(),
            trigger_mode: "always".to_string(),
            notification_group_id: None,
            fail_trigger_tasks: None,
            recover_trigger_tasks: None,
            cover_type: "all".to_string(),
            server_ids_json: None,
            actions_json: None,
            created_at: now,
            updated_at: now,
        }
    }

    async fn fetch_audits(
        db: &sea_orm::DatabaseConnection,
        action: &str,
    ) -> Vec<audit_log::Model> {
        audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq(action))
            .all(db)
            .await
            .unwrap()
    }

    fn make_service(
        db: sea_orm::DatabaseConnection,
    ) -> (FirewallService, AgentManager) {
        let (browser_tx, _) = broadcast::channel(16);
        let config = Arc::new(AppConfig::default());
        let svc = FirewallService::new(db, config, browser_tx.clone());
        let mgr = AgentManager::new(browser_tx);
        (svc, mgr)
    }

    #[tokio::test]
    async fn auto_block_skips_when_existing_row_covers() {
        let (db, _tmp) = setup_test_db().await;
        let now = chrono::Utc::now();

        // Pre-seed an `all`-coverage row for the same canonical target.
        block_list::ActiveModel {
            id: Set("existing-1".into()),
            target: Set("203.0.113.5/32".into()),
            family: Set(4),
            cover_type: Set("all".into()),
            server_ids_json: Set(None),
            comment: Set(None),
            origin: Set("manual".into()),
            origin_event_id: Set(None),
            origin_rule_id: Set(None),
            created_by: Set(None),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        let (svc, mgr) = make_service(db.clone());
        let rule = make_rule("rule-1");
        let payload = make_payload("203.0.113.5");
        let action = AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: None,
        };
        let result = svc
            .auto_block("srv-A", &rule, &payload, "evt-1", &action, &mgr)
            .await
            .unwrap();
        assert!(result.is_none(), "expected silent dedup, got {result:?}");

        // No new row.
        let rows = block_list::Entity::find().all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);

        // No conflict audit either.
        let conflicts = fetch_audits(&db, "firewall_auto_block_skipped_conflict").await;
        assert!(conflicts.is_empty(), "should not write conflict audit when covered");
    }

    #[tokio::test]
    async fn auto_block_skips_with_conflict_when_existing_row_does_not_cover() {
        let (db, _tmp) = setup_test_db().await;
        let now = chrono::Utc::now();

        // Pre-seed an `include`-coverage row that only includes srv-B.
        block_list::ActiveModel {
            id: Set("existing-2".into()),
            target: Set("203.0.113.7/32".into()),
            family: Set(4),
            cover_type: Set("include".into()),
            server_ids_json: Set(Some(r#"["srv-B"]"#.into())),
            comment: Set(None),
            origin: Set("manual".into()),
            origin_event_id: Set(None),
            origin_rule_id: Set(None),
            created_by: Set(None),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        let (svc, mgr) = make_service(db.clone());
        let rule = make_rule("rule-2");
        let payload = make_payload("203.0.113.7");
        let action = AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: None,
        };

        let result = svc
            .auto_block("srv-A", &rule, &payload, "evt-2", &action, &mgr)
            .await
            .unwrap();
        assert!(result.is_none(), "should skip without creating a new row");

        // No new row.
        let rows = block_list::Entity::find().all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);

        // Conflict audit written.
        let conflicts = fetch_audits(&db, "firewall_auto_block_skipped_conflict").await;
        assert_eq!(conflicts.len(), 1, "expected one conflict audit entry");
    }

    #[tokio::test]
    async fn auto_block_substitutes_comment_template() {
        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db.clone());

        let mut rule = make_rule("rule-3");
        rule.name = "SSH Brute Force Watch".to_string();
        let payload = make_payload("203.0.113.9");
        let action = AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: Some("{rule_name} → {event_type} ({severity})".into()),
        };

        let id = svc
            .auto_block("srv-A", &rule, &payload, "evt-3", &action, &mgr)
            .await
            .unwrap()
            .expect("expected a new block row");

        let row = block_list::Entity::find_by_id(&id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            row.comment.as_deref(),
            Some("SSH Brute Force Watch → ssh_brute_force (high)")
        );
    }

    // ── canonicalize_target edge cases ──

    #[test]
    fn canonical_bare_ipv6_yields_128() {
        let (t, f) = FirewallService::canonicalize_target("2001:db8::1").unwrap();
        assert_eq!(t, "2001:db8::1/128");
        assert_eq!(f, 6);
    }

    #[test]
    fn canonical_ipv6_cidr_strips_host_bits() {
        // Host bits in a v6 CIDR are dropped to the network address.
        let (t, f) = FirewallService::canonicalize_target("2001:db8::dead/32").unwrap();
        assert_eq!(t, "2001:db8::/32");
        assert_eq!(f, 6);
    }

    #[test]
    fn canonical_rejects_bad_cidr_prefix() {
        // Parses as neither IpAddr nor IpNet → BadRequest.
        let err = FirewallService::canonicalize_target("1.2.3.4/40").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn protected_invalid_cidr_input_is_reported() {
        // is_protected returns Some("invalid CIDR") rather than panicking.
        let reason = FirewallService::is_protected("garbage", &[]);
        assert_eq!(reason.as_deref(), Some("invalid CIDR"));
    }

    #[test]
    fn protected_allow_list_no_match_falls_through() {
        // A non-overlapping allow entry must NOT veto an external target.
        let allow = vec!["198.51.100.0/24".to_string()];
        assert!(FirewallService::is_protected("203.0.113.5/32", &allow).is_none());
    }

    #[test]
    fn protected_allow_list_ignores_malformed_entry() {
        // A malformed allow entry is skipped, not treated as a match.
        let allow = vec!["not-a-cidr".to_string()];
        assert!(FirewallService::is_protected("203.0.113.5/32", &allow).is_none());
    }

    // ── note_agent_external_ip / collect_dynamic_allow ──

    #[tokio::test]
    async fn dynamic_allow_includes_trusted_proxies_and_agent_ips() {
        let (db, _tmp) = setup_test_db().await;
        let (svc, _mgr) = make_service(db);

        // Record one agent external IP, then confirm it appears in the merged set.
        svc.note_agent_external_ip("srv-A", Some("203.0.113.50".parse().unwrap()))
            .await;
        let allow = svc.collect_dynamic_allow().await;
        assert!(
            allow.iter().any(|s| s == "203.0.113.50"),
            "agent external IP must be in dynamic allow, got {allow:?}"
        );
        // Default trusted_proxies (private ranges) are merged in too.
        assert!(
            allow.iter().any(|s| s.contains("127.0.0.0/8")),
            "trusted_proxies should be merged, got {allow:?}"
        );
    }

    #[tokio::test]
    async fn note_agent_external_ip_none_removes_entry() {
        let (db, _tmp) = setup_test_db().await;
        let (svc, _mgr) = make_service(db);

        svc.note_agent_external_ip("srv-A", Some("203.0.113.50".parse().unwrap()))
            .await;
        // Passing None clears the recorded IP.
        svc.note_agent_external_ip("srv-A", None).await;
        let allow = svc.collect_dynamic_allow().await;
        assert!(
            !allow.iter().any(|s| s == "203.0.113.50"),
            "cleared IP must not remain, got {allow:?}"
        );
    }

    #[tokio::test]
    async fn auto_block_rejected_by_dynamic_allow_agent_ip() {
        // An agent's reported external IP is tier-2.5 protected: auto-block of
        // that exact IP must be refused and a server-side reject audit written.
        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db.clone());
        svc.note_agent_external_ip("srv-A", Some("203.0.113.77".parse().unwrap()))
            .await;

        let rule = make_rule("rule-dyn");
        let payload = make_payload("203.0.113.77");
        let action = AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: None,
        };
        let result = svc
            .auto_block("srv-A", &rule, &payload, "evt-dyn", &action, &mgr)
            .await
            .unwrap();
        assert!(result.is_none(), "guarded target must not insert a row");

        let rows = block_list::Entity::find().all(&db).await.unwrap();
        assert!(rows.is_empty(), "no block_list row should exist");

        let rejects = fetch_audits(&db, "firewall_block_rejected_server").await;
        assert_eq!(rejects.len(), 1, "expected a server-reject audit entry");
    }

    #[tokio::test]
    async fn auto_block_rejected_by_hardcoded_guardrail() {
        // A private-range source_ip hits the tier-1 hard-coded guardrail.
        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db.clone());

        let rule = make_rule("rule-guard");
        let payload = make_payload("10.0.0.5");
        let action = AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: None,
        };
        let result = svc
            .auto_block("srv-A", &rule, &payload, "evt-guard", &action, &mgr)
            .await
            .unwrap();
        assert!(result.is_none());
        assert!(block_list::Entity::find().all(&db).await.unwrap().is_empty());
        let rejects = fetch_audits(&db, "firewall_block_rejected_server").await;
        assert_eq!(rejects.len(), 1);
    }

    #[tokio::test]
    async fn auto_block_fresh_insert_uses_default_comment_and_audits() {
        // No comment template → falls back to "Auto-block from {rule_name}".
        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db.clone());

        let mut rule = make_rule("rule-fresh");
        rule.name = "Watcher".to_string();
        let payload = make_payload("203.0.113.200");
        let action = AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: None,
        };
        let id = svc
            .auto_block("srv-A", &rule, &payload, "evt-fresh", &action, &mgr)
            .await
            .unwrap()
            .expect("expected a fresh insert");

        let row = block_list::Entity::find_by_id(&id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.comment.as_deref(), Some("Auto-block from Watcher"));
        assert_eq!(row.target, "203.0.113.200/32");
        assert_eq!(row.family, 4);
        assert_eq!(row.origin, crate::service::alert::ORIGIN_AUTO);
        assert_eq!(row.origin_event_id.as_deref(), Some("evt-fresh"));
        assert_eq!(row.origin_rule_id.as_deref(), Some("rule-fresh"));

        // A create audit is written.
        let created = fetch_audits(&db, "firewall_block_created").await;
        assert_eq!(created.len(), 1);
    }

    // ── list_for_server: coverage filter + ordering ──

    /// Insert a `block_list` row and return its persisted Model.
    async fn insert_block(
        db: &sea_orm::DatabaseConnection,
        id: &str,
        target: &str,
        cover_type: &str,
        server_ids_json: Option<&str>,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> block_list::Model {
        block_list::ActiveModel {
            id: Set(id.into()),
            target: Set(target.into()),
            family: Set(4),
            cover_type: Set(cover_type.into()),
            server_ids_json: Set(server_ids_json.map(|s| s.to_string())),
            comment: Set(None),
            origin: Set("manual".into()),
            origin_event_id: Set(None),
            origin_rule_id: Set(None),
            created_by: Set(None),
            created_at: Set(created_at),
        }
        .insert(db)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn list_for_server_filters_by_coverage_and_orders_oldest_first() {
        let (db, _tmp) = setup_test_db().await;
        let base = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        // Newest first by insert, but list_for_server orders by created_at ASC.
        insert_block(&db, "b-newer", "203.0.113.2/32", "all", None, base + chrono::Duration::seconds(10)).await;
        insert_block(&db, "b-older", "203.0.113.1/32", "all", None, base).await;
        // include srv-B only → excluded for srv-A.
        insert_block(&db, "b-incl", "203.0.113.3/32", "include", Some(r#"["srv-B"]"#), base + chrono::Duration::seconds(20)).await;
        // exclude srv-A → excluded for srv-A.
        insert_block(&db, "b-excl", "203.0.113.4/32", "exclude", Some(r#"["srv-A"]"#), base + chrono::Duration::seconds(30)).await;

        let (svc, _mgr) = make_service(db);
        let entries = svc.list_for_server("srv-A").await.unwrap();
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        // Only the two `all`-coverage rows, oldest first.
        assert_eq!(ids, vec!["b-older", "b-newer"]);
        assert_eq!(entries[0].family, 4);
    }

    #[tokio::test]
    async fn list_for_server_empty_when_nothing_covers() {
        let (db, _tmp) = setup_test_db().await;
        let now = chrono::Utc::now();
        insert_block(&db, "b1", "203.0.113.9/32", "include", Some(r#"["other"]"#), now).await;
        let (svc, _mgr) = make_service(db);
        let entries = svc.list_for_server("srv-A").await.unwrap();
        assert!(entries.is_empty());
    }

    // ── push_* fan-out gating ──

    /// Wire an online agent into the manager with a ServerMessage receiver, a
    /// negotiated protocol version, and an effective capability bitmask.
    fn wire_agent(
        mgr: &AgentManager,
        server_id: &str,
        protocol_version: u32,
        caps: u32,
    ) -> tokio::sync::mpsc::Receiver<serverbee_common::protocol::ServerMessage> {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9000);
        mgr.add_connection(server_id.into(), "srv".into(), tx, addr);
        mgr.set_protocol_version(server_id, protocol_version);
        mgr.update_agent_local_capabilities(server_id, caps);
        rx
    }

    fn block_model(id: &str, target: &str, cover_type: &str, server_ids_json: Option<&str>) -> block_list::Model {
        block_list::Model {
            id: id.into(),
            target: target.into(),
            family: 4,
            cover_type: cover_type.into(),
            server_ids_json: server_ids_json.map(|s| s.to_string()),
            comment: None,
            origin: "manual".into(),
            origin_event_id: None,
            origin_rule_id: None,
            created_by: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn push_add_reaches_capable_uptodate_covered_agent() {
        use serverbee_common::constants::CAP_FIREWALL_BLOCK;
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;
        use serverbee_common::protocol::ServerMessage;

        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db);
        let mut rx = wire_agent(&mgr, "srv-A", FIREWALL_MIN_PROTOCOL, CAP_FIREWALL_BLOCK);

        let row = block_model("b1", "203.0.113.5/32", "all", None);
        svc.push_add_to_covered_agents(&row, &mgr).await;

        let msg = rx.try_recv().expect("agent should receive a message");
        match msg {
            ServerMessage::BlocklistAdd { entry } => {
                assert_eq!(entry.id, "b1");
                assert_eq!(entry.target, "203.0.113.5/32");
            }
            other => panic!("expected BlocklistAdd, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn push_add_skips_agent_without_firewall_capability() {
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;

        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db);
        // caps = 0 → no CAP_FIREWALL_BLOCK.
        let mut rx = wire_agent(&mgr, "srv-A", FIREWALL_MIN_PROTOCOL, 0);

        let row = block_model("b1", "203.0.113.5/32", "all", None);
        svc.push_add_to_covered_agents(&row, &mgr).await;
        assert!(rx.try_recv().is_err(), "no message should be sent");
    }

    #[tokio::test]
    async fn push_add_skips_agent_on_old_protocol() {
        use serverbee_common::constants::CAP_FIREWALL_BLOCK;
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;

        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db);
        // protocol below the firewall gate.
        let mut rx = wire_agent(&mgr, "srv-A", FIREWALL_MIN_PROTOCOL - 1, CAP_FIREWALL_BLOCK);

        let row = block_model("b1", "203.0.113.5/32", "all", None);
        svc.push_add_to_covered_agents(&row, &mgr).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn push_add_skips_uncovered_agent() {
        use serverbee_common::constants::CAP_FIREWALL_BLOCK;
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;

        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db);
        let mut rx = wire_agent(&mgr, "srv-A", FIREWALL_MIN_PROTOCOL, CAP_FIREWALL_BLOCK);

        // Row covers only srv-B → srv-A must not receive it.
        let row = block_model("b1", "203.0.113.5/32", "include", Some(r#"["srv-B"]"#));
        svc.push_add_to_covered_agents(&row, &mgr).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn push_remove_reaches_covered_agent() {
        use serverbee_common::constants::CAP_FIREWALL_BLOCK;
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;
        use serverbee_common::protocol::ServerMessage;

        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db);
        let mut rx = wire_agent(&mgr, "srv-A", FIREWALL_MIN_PROTOCOL, CAP_FIREWALL_BLOCK);

        let row = block_model("b-rm", "203.0.113.5/32", "all", None);
        svc.push_remove_to_covered_agents(&row, &mgr).await;

        let msg = rx.try_recv().expect("agent should receive remove");
        match msg {
            ServerMessage::BlocklistRemove { id } => assert_eq!(id, "b-rm"),
            other => panic!("expected BlocklistRemove, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn push_remove_skips_uncapable_agent() {
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;

        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db);
        let mut rx = wire_agent(&mgr, "srv-A", FIREWALL_MIN_PROTOCOL, 0);

        let row = block_model("b-rm", "203.0.113.5/32", "all", None);
        svc.push_remove_to_covered_agents(&row, &mgr).await;
        assert!(rx.try_recv().is_err());
    }

    // ── push_sync_to / push_reset_to ──

    #[tokio::test]
    async fn push_sync_sends_covering_entries_to_one_agent() {
        use serverbee_common::constants::CAP_FIREWALL_BLOCK;
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;
        use serverbee_common::protocol::ServerMessage;

        let (db, _tmp) = setup_test_db().await;
        let now = chrono::Utc::now();
        insert_block(&db, "s1", "203.0.113.1/32", "all", None, now).await;
        insert_block(&db, "s2", "203.0.113.2/32", "include", Some(r#"["srv-B"]"#), now).await;

        let (svc, mgr) = make_service(db);
        let mut rx = wire_agent(&mgr, "srv-A", FIREWALL_MIN_PROTOCOL, CAP_FIREWALL_BLOCK);

        svc.push_sync_to("srv-A", &mgr).await.unwrap();
        let msg = rx.try_recv().expect("sync should be delivered");
        match msg {
            ServerMessage::BlocklistSync { entries } => {
                // Only the `all` row covers srv-A.
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].id, "s1");
            }
            other => panic!("expected BlocklistSync, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn push_sync_noop_when_agent_not_connected() {
        let (db, _tmp) = setup_test_db().await;
        let now = chrono::Utc::now();
        insert_block(&db, "s1", "203.0.113.1/32", "all", None, now).await;

        let (svc, mgr) = make_service(db);
        // No agent wired for "ghost" → returns Ok, sends nothing.
        let res = svc.push_sync_to("ghost", &mgr).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn push_reset_sends_reset_and_clears_only_target_apply_state() {
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;
        use serverbee_common::protocol::ServerMessage;

        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db);
        let mut rx = wire_agent(&mgr, "srv-A", FIREWALL_MIN_PROTOCOL, 0);

        // Seed apply_state for srv-A and srv-B; only srv-A should be cleared.
        {
            let mut g = svc.apply_state.write().await;
            g.insert(
                ("blk-1".into(), "srv-A".into()),
                ApplyState {
                    state: BlocklistEntryState::Present,
                    reason: None,
                    at: chrono::Utc::now(),
                },
            );
            g.insert(
                ("blk-2".into(), "srv-B".into()),
                ApplyState {
                    state: BlocklistEntryState::Present,
                    reason: None,
                    at: chrono::Utc::now(),
                },
            );
        }

        svc.push_reset_to("srv-A", &mgr).await;

        let msg = rx.try_recv().expect("reset should be delivered");
        assert!(matches!(msg, ServerMessage::BlocklistReset));

        let g = svc.apply_state.read().await;
        assert!(!g.contains_key(&("blk-1".into(), "srv-A".to_string())));
        assert!(g.contains_key(&("blk-2".into(), "srv-B".to_string())));
    }

    #[tokio::test]
    async fn push_reset_clears_state_even_without_connection() {
        let (db, _tmp) = setup_test_db().await;
        let (svc, mgr) = make_service(db);
        {
            let mut g = svc.apply_state.write().await;
            g.insert(
                ("blk-1".into(), "ghost".into()),
                ApplyState {
                    state: BlocklistEntryState::Present,
                    reason: None,
                    at: chrono::Utc::now(),
                },
            );
        }
        // No connection for "ghost" — the local apply_state is still cleared.
        svc.push_reset_to("ghost", &mgr).await;
        let g = svc.apply_state.read().await;
        assert!(g.is_empty());
    }

    // ── record_ack: state mirror + audit + broadcast ──

    #[tokio::test]
    async fn record_ack_present_mirrors_state_and_audits_and_broadcasts() {
        use serverbee_common::firewall::BlocklistAckItem;
        use serverbee_common::protocol::BrowserMessage;

        let (db, _tmp) = setup_test_db().await;
        let (browser_tx, mut browser_rx) = broadcast::channel(16);
        let config = Arc::new(AppConfig::default());
        let svc = FirewallService::new(db.clone(), config, browser_tx);

        let item = BlocklistAckItem {
            id: "blk-1".into(),
            state: BlocklistEntryState::Present,
            reason: None,
        };
        svc.record_ack("srv-A", item, &db).await;

        // apply_state mirror updated.
        {
            let g = svc.apply_state.read().await;
            let st = g
                .get(&("blk-1".to_string(), "srv-A".to_string()))
                .expect("apply state recorded");
            assert_eq!(st.state, BlocklistEntryState::Present);
        }

        // audit row written with the "applied" action.
        let applied = fetch_audits(&db, "firewall_block_applied_agent").await;
        assert_eq!(applied.len(), 1);

        // broadcast emitted.
        let msg = browser_rx.try_recv().expect("broadcast emitted");
        match msg {
            BrowserMessage::FirewallApplyStateChanged { block_id, server_id, state, .. } => {
                assert_eq!(block_id, "blk-1");
                assert_eq!(server_id, "srv-A");
                assert_eq!(state, BlocklistEntryState::Present);
            }
            other => panic!("expected FirewallApplyStateChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn record_ack_absent_uses_removed_action() {
        use serverbee_common::firewall::BlocklistAckItem;

        let (db, _tmp) = setup_test_db().await;
        let (svc, _mgr) = make_service(db.clone());
        let item = BlocklistAckItem {
            id: "blk-2".into(),
            state: BlocklistEntryState::Absent,
            reason: None,
        };
        svc.record_ack("srv-A", item, &db).await;
        let removed = fetch_audits(&db, "firewall_block_removed_agent").await;
        assert_eq!(removed.len(), 1);
    }

    #[tokio::test]
    async fn record_ack_failed_uses_rejected_action_and_keeps_reason() {
        use serverbee_common::firewall::BlocklistAckItem;

        let (db, _tmp) = setup_test_db().await;
        let (svc, _mgr) = make_service(db.clone());
        let item = BlocklistAckItem {
            id: "blk-3".into(),
            state: BlocklistEntryState::Failed,
            reason: Some("nft denied".into()),
        };
        svc.record_ack("srv-A", item, &db).await;

        let rejected = fetch_audits(&db, "firewall_block_rejected_agent").await;
        assert_eq!(rejected.len(), 1);
        // The reason is mirrored into apply_state.
        let g = svc.apply_state.read().await;
        let st = g
            .get(&("blk-3".to_string(), "srv-A".to_string()))
            .unwrap();
        assert_eq!(st.state, BlocklistEntryState::Failed);
        assert_eq!(st.reason.as_deref(), Some("nft denied"));
    }

    // ── record_reset_ack: ok / failure audit actions ──

    #[tokio::test]
    async fn record_reset_ack_ok_writes_acked_audit() {
        let (db, _tmp) = setup_test_db().await;
        let (svc, _mgr) = make_service(db.clone());
        svc.record_reset_ack("srv-A", true, None, &db).await;
        let acked = fetch_audits(&db, "firewall_reset_acked").await;
        assert_eq!(acked.len(), 1);
    }

    #[tokio::test]
    async fn record_reset_ack_failure_writes_failed_audit() {
        let (db, _tmp) = setup_test_db().await;
        let (svc, _mgr) = make_service(db.clone());
        svc.record_reset_ack("srv-A", false, Some("agent error".into()), &db)
            .await;
        let failed = fetch_audits(&db, "firewall_reset_failed_agent").await;
        assert_eq!(failed.len(), 1);
        // The success action must NOT be written.
        let acked = fetch_audits(&db, "firewall_reset_acked").await;
        assert!(acked.is_empty());
    }

    // ── broadcast_changed_* ──

    #[tokio::test]
    async fn broadcast_changed_created_and_deleted_emit_correct_kind() {
        use serverbee_common::firewall::BlocklistChangeKind;
        use serverbee_common::protocol::BrowserMessage;

        let (db, _tmp) = setup_test_db().await;
        let (browser_tx, mut browser_rx) = broadcast::channel(16);
        let config = Arc::new(AppConfig::default());
        let svc = FirewallService::new(db, config, browser_tx);

        let row = block_model("blk-x", "203.0.113.5/32", "all", None);
        svc.broadcast_changed_created(&row);
        match browser_rx.try_recv().unwrap() {
            BrowserMessage::BlocklistChanged { kind, block_id, target } => {
                assert_eq!(kind, BlocklistChangeKind::Created);
                assert_eq!(block_id, "blk-x");
                assert_eq!(target, "203.0.113.5/32");
            }
            other => panic!("expected BlocklistChanged Created, got {other:?}"),
        }

        svc.broadcast_changed_deleted(&row);
        match browser_rx.try_recv().unwrap() {
            BrowserMessage::BlocklistChanged { kind, .. } => {
                assert_eq!(kind, BlocklistChangeKind::Deleted);
            }
            other => panic!("expected BlocklistChanged Deleted, got {other:?}"),
        }
    }
}
