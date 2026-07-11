//! Security-domain message handling: security events, IP-quality unlock
//! results, capability-change notifications, and firewall blocklist acks.
//!
//! Security events and unlock results share the same inbound capability
//! re-check: a capability revoked mid-run must not let a trailing batch of
//! agent data be persisted or fanned out to browsers.

use std::sync::Arc;
use std::time::Duration;

use crate::service::agent_reconcile::AgentDesiredStateDomain;
use crate::service::alert::AlertService;
use crate::service::audit::AuditService;
use crate::service::ip_quality::IpQualityService;
use crate::service::ip_risk::IpRiskService;
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::constants::has_capability;
use serverbee_common::protocol::{BrowserMessage, TemporaryGrant, UnlockResultData};

/// High-risk capabilities whose temporary grant warrants an alert evaluation
/// (`capability_grant_detected`). Low-risk caps still get audited but do not
/// fire alerts.
fn is_high_risk_cap(cap: &str) -> bool {
    matches!(cap, "terminal" | "exec" | "file" | "docker")
}

/// Effective capabilities for inbound-data gating. The hot-path cache is
/// populated once the agent sends `SystemInfo` on the current connection;
/// before that, fall back to the DB row.
async fn effective_caps_or_db(state: &Arc<AppState>, server_id: &str) -> u32 {
    match state.agent_manager.get_effective_capabilities(server_id) {
        Some(c) => c,
        None => {
            use crate::entity::server;
            use sea_orm::EntityTrait;
            server::Entity::find_by_id(server_id)
                .one(&state.db)
                .await
                .ok()
                .flatten()
                .and_then(|s| u32::try_from(s.capabilities).ok())
                .unwrap_or(0)
        }
    }
}

/// Re-check `cap` before accepting inbound agent data. On denial, writes a
/// `denied_action` audit row and returns false.
async fn gate_inbound_data(
    state: &Arc<AppState>,
    server_id: &str,
    cap: u32,
    denied_action: &str,
) -> bool {
    let caps = effective_caps_or_db(state, server_id).await;
    if has_capability(caps, cap) {
        return true;
    }
    let detail = serde_json::json!({ "server_id": server_id }).to_string();
    if let Err(e) = AuditService::log(&state.db, "system", denied_action, Some(&detail), "").await {
        tracing::warn!(server_id, error = %e, "audit log for {denied_action} failed");
    }
    false
}

pub(super) async fn on_security_event(
    state: &Arc<AppState>,
    server_id: &str,
    payload: serverbee_common::security::SecurityEventPayload,
) {
    use serverbee_common::constants::CAP_SECURITY_EVENTS;
    if !gate_inbound_data(state, server_id, CAP_SECURITY_EVENTS, "security_event_denied").await {
        return;
    }
    if let Err(e) = state.security_service.record_event(server_id, payload).await {
        tracing::error!(server_id, error = %e, "security_event record failed");
    }
}

pub(super) async fn on_unlock_results(
    state: &Arc<AppState>,
    server_id: &str,
    egress_ip: String,
    results: Vec<UnlockResultData>,
    checked_at: chrono::DateTime<chrono::Utc>,
) {
    use serverbee_common::constants::CAP_IP_QUALITY;
    if !gate_inbound_data(state, server_id, CAP_IP_QUALITY, "ip_quality_results_denied").await {
        return;
    }

    // Phase 1 (synchronous-ish): save unlock results + broadcast immediately
    // with ip_quality = None so the UI shows fresh unlock data right away.
    if let Err(e) =
        IpQualityService::save_unlock_results(&state.db, server_id, results.clone()).await
    {
        tracing::error!("Failed to save unlock results for {server_id}: {e}");
    }

    state
        .agent_manager
        .broadcast_browser(BrowserMessage::IpQualityUpdate {
            server_id: server_id.to_string(),
            unlock_results: results.clone(),
            ip_quality: None,
        });

    // Phase 2 (non-blocking): spawn a background task to run IP risk scoring
    // and emit a second broadcast with the full ip_quality snapshot.
    // Wrapped in a 30s timeout so a slow/down provider never blocks the agent loop.
    // Skip entirely when egress_ip is empty — an empty IP produces no
    // meaningful snapshot and would contaminate ip_risk_cache with a "" key.
    if egress_ip.trim().is_empty() {
        tracing::debug!(
            "UnlockResults from {server_id}: egress_ip is empty, skipping IP risk scoring"
        );
        return;
    }

    let db_bg = state.db.clone();
    let geoip_bg = Arc::clone(&state.geoip);
    let config_bg = state.config.ip_quality.clone();
    let browser_tx_bg = state.browser_tx.clone();
    let server_id_owned = server_id.to_string();
    // Keep a copy for the timeout warning (the inner async moves server_id_owned)
    let server_id_for_warn = server_id_owned.clone();

    tokio::spawn(async move {
        let result = tokio::time::timeout(Duration::from_secs(30), async move {
            let risk_service = IpRiskService::new(config_bg);
            // score_ip returns None for a blank IP (defensive double-guard).
            let Some(snapshot) = risk_service.score_ip(&db_bg, &geoip_bg, &egress_ip).await else {
                return;
            };

            if let Err(e) =
                IpQualityService::save_ip_quality_snapshot(&db_bg, &server_id_owned, &snapshot)
                    .await
            {
                // Phase 2 is a non-critical enrichment step: the UI already
                // received the unlock matrix from the Phase 1 broadcast, so a
                // failed snapshot persist is logged at warn (not error).
                tracing::warn!(
                    "Failed to save ip_quality_snapshot for {}: {e}",
                    server_id_owned
                );
            }

            let _ = browser_tx_bg.send(BrowserMessage::IpQualityUpdate {
                server_id: server_id_owned,
                unlock_results: results,
                ip_quality: Some(snapshot),
            });

            // checked_at is part of the protocol message but the server uses
            // its own Utc::now() for timestamps (the agent's clock may differ).
            let _ = checked_at;
        })
        .await;

        if result.is_err() {
            tracing::warn!("IP risk scoring timed out for agent {server_id_for_warn}");
        }
    });
}

pub(super) async fn on_capabilities_changed(
    state: &Arc<AppState>,
    server_id: &str,
    capabilities: u32,
    temporary: Vec<TemporaryGrant>,
    changes: Vec<serverbee_common::protocol::CapabilityChangeEvent>,
) {
    // Mirror the agent-reported effective capability bitmask and the
    // live temporary grants. The agent host is the only authority; the
    // server persists these purely for display/enforcement gating.
    state
        .agent_manager
        .update_agent_local_capabilities(server_id, capabilities);
    state
        .agent_manager
        .update_temporary_grants(server_id, temporary.clone());
    if let Err(e) =
        ServerService::update_capabilities_mirror(&state.db, server_id, capabilities).await
    {
        tracing::error!("Failed to mirror capabilities for {server_id}: {e}");
    }

    for domain in [
        AgentDesiredStateDomain::PingTasks,
        AgentDesiredStateDomain::NetworkProbes,
        AgentDesiredStateDomain::IpQuality,
        AgentDesiredStateDomain::Firewall,
    ] {
        if let Err(error) = state
            .agent_desired_state
            .reconcile_agent(server_id, domain)
            .await
        {
            tracing::warn!(
                server_id,
                ?domain,
                error = %error,
                "capability-change desired-state reconcile failed"
            );
        }
    }

    // Resolve display name + originating IP for the audit trail. Neither
    // `server_name` nor `remote_addr` is in scope here, so we look them
    // up from the DB / connection registry (mirroring the SystemInfo arm).
    let server_name = ServerService::get_server(&state.db, server_id)
        .await
        .map(|s| s.name)
        .unwrap_or_else(|_| "Unknown".to_string());
    let ip = state
        .agent_manager
        .get_remote_addr(server_id)
        .map(|a| a.ip().to_string())
        .unwrap_or_default();

    for ch in &changes {
        let action = match ch.action {
            serverbee_common::protocol::CapabilityChangeAction::Granted => {
                "capability_temporarily_granted"
            }
            serverbee_common::protocol::CapabilityChangeAction::Expired => {
                "capability_grant_expired"
            }
            serverbee_common::protocol::CapabilityChangeAction::Revoked => {
                "capability_grant_revoked"
            }
        };
        let detail = serde_json::json!({
            "server_id": server_id,
            "server_name": server_name,
            "cap": ch.cap,
            "expires_at": ch.expires_at,
            "granted_by": ch.granted_by,
            "reason": ch.reason,
        })
        .to_string();
        if let Err(e) = AuditService::log(&state.db, "system", action, Some(&detail), &ip).await {
            tracing::error!("Failed to write capability-change audit log: {e}");
        }

        // Only a temporary grant of a high-risk capability fires the
        // event-driven `capability_grant_detected` alert. Expiry/revoke
        // and low-risk caps are audited but never alerted.
        if matches!(
            ch.action,
            serverbee_common::protocol::CapabilityChangeAction::Granted
        ) && is_high_risk_cap(&ch.cap)
            && let Err(e) = AlertService::check_event_rules(
                &state.db,
                &state.config,
                &state.alert_state_manager,
                server_id,
                "capability_grant_detected",
            )
            .await
        {
            tracing::error!("capability_grant_detected alert eval failed: {e}");
        }
    }

    state
        .agent_manager
        .broadcast_browser(BrowserMessage::CapabilitiesChanged {
            server_id: server_id.to_string(),
            capabilities,
            agent_local_capabilities: Some(capabilities),
            effective_capabilities: Some(capabilities),
            temporary,
        });
}

pub(super) async fn on_blocklist_ack(
    state: &Arc<AppState>,
    server_id: &str,
    results: Vec<serverbee_common::firewall::BlocklistAckItem>,
) {
    for item in results {
        state.firewall.record_ack(server_id, item, &state.db).await;
    }
}

pub(super) async fn on_blocklist_reset_ack(
    state: &Arc<AppState>,
    server_id: &str,
    ok: bool,
    reason: Option<String>,
) {
    state
        .firewall
        .record_reset_ack(server_id, ok, reason, &state.db)
        .await;
}
