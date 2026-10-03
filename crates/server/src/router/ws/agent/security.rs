//! Security-domain message handling: security events, IP-quality unlock
//! results, capability-change notifications, and firewall blocklist acks.
//!
//! Security events and unlock results share the same inbound capability
//! re-check: a capability revoked mid-run must not let a trailing batch of
//! agent data be persisted or fanned out to browsers.

use std::sync::Arc;
use std::time::Duration;

use crate::service::alert_event_intents;
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

/// Re-check `cap` before accepting inbound agent data (the priority rule —
/// live report first, mirror fallback — lives in capability_gate). On denial,
/// writes a `denied_action` audit row and returns false.
async fn gate_inbound_data(
    state: &Arc<AppState>,
    server_id: &str,
    cap: u32,
    denied_action: &str,
) -> bool {
    let caps = crate::service::capability_gate::effective_capabilities(state, server_id).await;
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
    if !gate_inbound_data(
        state,
        server_id,
        CAP_SECURITY_EVENTS,
        "security_event_denied",
    )
    .await
    {
        return;
    }
    if let Err(e) = state
        .security_service
        .retain_agent_event(server_id, payload)
        .await
    {
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
    msg_id: String,
    occurred_at: Option<chrono::DateTime<chrono::Utc>>,
    capabilities: u32,
    temporary: Vec<TemporaryGrant>,
    changes: Vec<serverbee_common::protocol::CapabilityChangeEvent>,
) -> bool {
    use crate::entity::{audit_log, capability_event_receipt as receipt};
    use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, NotSet, Set, TransactionTrait};
    use sha2::{Digest, Sha256};
    let expects_ack = occurred_at.is_some();
    // The existing connection owner holds its lifecycle lock across this call.
    // Mirror, audit, receipt and intents are one admission. Failed capture is
    // never acknowledged: the Agent retains and automatically resends its source.
    let admission = async {
        let serialized = serde_json::to_vec(&(occurred_at, &changes))
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
        let payload_hash = format!("{:x}", Sha256::digest(serialized));
        let txn = state.db.begin().await?;
        txn.execute_unprepared("UPDATE servers SET capabilities=capabilities WHERE id=''").await?;
        if let Some(old) = receipt::Entity::find_by_id((server_id.to_string(), msg_id.clone())).one(&txn).await? {
            if old.payload_hash != payload_hash {
                return Err(crate::error::AppError::Internal("Capability event identity reused with different content".into()));
            }
            ServerService::update_capabilities_mirror(&txn, server_id, capabilities).await?;
            txn.commit().await?;
            return Ok::<_, crate::error::AppError>(());
        }
        let now = chrono::Utc::now();
        let occurred_at = occurred_at.unwrap_or(now);
        if occurred_at > now + chrono::Duration::minutes(5) {
            return Err(crate::error::AppError::Internal("Capability event time is in the future".into()));
        }
        let server_name = crate::entity::server::Entity::find_by_id(server_id).one(&txn).await?
            .ok_or_else(|| crate::error::AppError::Internal("Capability event Server missing".into()))?.name;
        let ip = state.agent_manager.get_remote_addr(server_id).map(|a| a.ip().to_string()).unwrap_or_default();
        ServerService::update_capabilities_mirror(&txn, server_id, capabilities).await?;
        for ch in &changes {
            let action = match ch.action {
                serverbee_common::protocol::CapabilityChangeAction::Granted => "capability_temporarily_granted",
                serverbee_common::protocol::CapabilityChangeAction::Expired => "capability_grant_expired",
                serverbee_common::protocol::CapabilityChangeAction::Revoked => "capability_grant_revoked",
            };
            let detail = serde_json::json!({"server_id":server_id,"server_name":server_name,
                "cap":ch.cap,"expires_at":ch.expires_at,"granted_by":ch.granted_by,"reason":ch.reason}).to_string();
            audit_log::ActiveModel { id: NotSet, user_id: Set("system".into()), action: Set(action.into()),
                detail: Set(Some(detail)), ip: Set(ip.clone()), created_at: Set(occurred_at) }.insert(&txn).await?;
            if matches!(ch.action, serverbee_common::protocol::CapabilityChangeAction::Granted) && is_high_risk_cap(&ch.cap) {
                alert_event_intents::capture(&txn, server_id, "capability_grant_detected", occurred_at).await?;
            }
        }
        receipt::ActiveModel { server_id: Set(server_id.into()), msg_id: Set(msg_id.clone()),
            payload_hash: Set(payload_hash), occurred_at: Set(occurred_at) }.insert(&txn).await?;
        txn.commit().await?;
        Ok(())
    }.await;
    if let Err(error) = admission {
        tracing::error!(server_id, error = %error, "Capability event admission failed; retained at Agent source");
        return false;
    }
    {
        state
            .agent_manager
            .update_agent_local_capabilities(server_id, capabilities);
        state
            .agent_manager
            .update_temporary_grants(server_id, temporary.clone());
        if let Err(error) = state
            .agent_desired_state
            .reconcile_connection(server_id)
            .await
        {
            tracing::warn!(server_id, error = %error, "capability-change desired-state reconcile was incomplete");
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
    // Capture is durable even if immediate outbox replay fails. Startup polling
    // owns retries; the source may discard the frame after this acknowledgement.
    if expects_ack && let Some(tx) = state.agent_manager.get_sender(server_id) {
        let _ = tx
            .send(serverbee_common::protocol::ServerMessage::Ack { msg_id })
            .await;
    }
    if let Err(error) =
        alert_event_intents::replay(&state.db, &state.config, &state.alert_state_manager).await
    {
        tracing::warn!(error = %error, "Capability alert intents remain pending");
    }
    true
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
