//! Capability gate for control-plane entry points (file, docker, terminal,
//! upgrade, docker log streaming).
//!
//! Capabilities are agent-owned: the live agent-reported bitmask decides,
//! falling back to the persisted mirror only when no agent has reported yet.
//! This module is the single seam for the lookup → deny → online sequence so
//! denial semantics (status codes, deny reasons, audit trail) cannot drift
//! between routes.

use crate::entity::server;
use crate::error::AppError;
use crate::service::audit::AuditService;
use crate::service::server::ServerService;
use crate::state::AppState;

/// The raw agent-effective bitmask for consumers that need the mask itself
/// (inbound-data gates, availability checks with bespoke error shapes) rather
/// than a deny decision: live agent report first, persisted mirror before the
/// first `SystemInfo`, zero when the server row is gone. Same priority rule
/// as [`require_capability`], resolved through the same funnel.
pub async fn effective_capabilities(state: &AppState, server_id: &str) -> u32 {
    // Hot path: a reported bitmask decides without touching the DB.
    if let Some(caps) = state.agent_manager.get_effective_capabilities(server_id) {
        return caps;
    }
    let mirror = ServerService::get_server(&state.db, server_id)
        .await
        .map(|s| s.capabilities as u32)
        .unwrap_or(0);
    state
        .agent_manager
        .effective_capabilities_or(server_id, mirror)
}

/// Resolve the server row and apply the agent-owned capability policy.
/// Returns the server model on success so callers don't re-query.
pub async fn require_capability(
    state: &AppState,
    server_id: &str,
    cap: u32,
) -> Result<server::Model, AppError> {
    let server = ServerService::get_server(&state.db, server_id).await?;
    if let Some(reason) =
        state
            .agent_manager
            .capability_denied_reason(server_id, server.capabilities as u32, cap)
    {
        return Err(AppError::Forbidden(reason.into()));
    }
    Ok(server)
}

/// [`require_capability`] plus a live-connection requirement — control-plane
/// request/reply needs an agent on the other end.
pub async fn require_capability_online(
    state: &AppState,
    server_id: &str,
    cap: u32,
) -> Result<server::Model, AppError> {
    let server = require_capability(state, server_id, cap).await?;
    if !state.agent_manager.is_online(server_id) {
        return Err(AppError::NotFound("Server offline".into()));
    }
    Ok(server)
}

/// [`require_capability`] that also writes a `denied_action` audit row
/// (server_id + deny_reason, best-effort) when the gate rejects, so denials
/// leave the same trail on every control surface.
pub async fn require_capability_audited(
    state: &AppState,
    server_id: &str,
    cap: u32,
    user_id: &str,
    ip: &str,
    denied_action: &str,
) -> Result<server::Model, AppError> {
    match require_capability(state, server_id, cap).await {
        Ok(server) => Ok(server),
        Err(error) => {
            let detail = serde_json::json!({
                "server_id": server_id,
                "deny_reason": error.audit_reason(),
            })
            .to_string();
            let _ = AuditService::log(&state.db, user_id, denied_action, Some(&detail), ip).await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::test_utils::setup_test_db;
    use sea_orm::{ActiveModelTrait, Set};
    use serverbee_common::constants::{CAP_DOCKER, CAP_FILE};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    async fn setup_state_with_server(caps: u32) -> (Arc<AppState>, tempfile::TempDir) {
        let (db, tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();
        let now = chrono::Utc::now();
        server::ActiveModel {
            id: Set("srv-1".into()),
            name: Set("Srv".into()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(caps as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&state.db)
        .await
        .unwrap();
        (state, tmp)
    }

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    #[tokio::test]
    async fn effective_capabilities_prefers_report_then_mirror_then_zero() {
        let (state, _tmp) = setup_state_with_server(CAP_FILE).await;
        // No report yet: the persisted mirror decides.
        assert_eq!(effective_capabilities(&state, "srv-1").await, CAP_FILE);
        // A live report overrides the mirror entirely.
        state
            .agent_manager
            .update_agent_local_capabilities("srv-1", CAP_DOCKER);
        assert_eq!(effective_capabilities(&state, "srv-1").await, CAP_DOCKER);
        // Unknown server resolves to no capabilities, not an error.
        assert_eq!(effective_capabilities(&state, "nope").await, 0);
    }

    #[tokio::test]
    async fn missing_server_is_not_found() {
        let (state, _tmp) = setup_state_with_server(CAP_FILE).await;
        let result = require_capability(&state, "nope", CAP_FILE).await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn mirror_gates_when_no_agent_reported() {
        let (state, _tmp) = setup_state_with_server(CAP_FILE).await;
        assert!(require_capability(&state, "srv-1", CAP_FILE).await.is_ok());
        let denied = require_capability(&state, "srv-1", CAP_DOCKER).await;
        assert!(matches!(denied, Err(AppError::Forbidden(_))));
    }

    #[tokio::test]
    async fn agent_report_overrides_mirror() {
        let (state, _tmp) = setup_state_with_server(CAP_FILE).await;
        // Agent reports docker but not file: the live report wins over the mirror.
        state
            .agent_manager
            .update_agent_local_capabilities("srv-1", CAP_DOCKER);
        assert!(
            require_capability(&state, "srv-1", CAP_DOCKER)
                .await
                .is_ok()
        );
        assert!(require_capability(&state, "srv-1", CAP_FILE).await.is_err());
    }

    #[tokio::test]
    async fn online_variant_requires_live_connection() {
        let (state, _tmp) = setup_state_with_server(CAP_FILE).await;
        let offline = require_capability_online(&state, "srv-1", CAP_FILE).await;
        assert!(matches!(offline, Err(AppError::NotFound(_))));

        let (tx, _rx) = mpsc::channel(1);
        state
            .agent_manager
            .add_connection("srv-1".into(), "Srv".into(), tx, test_addr());
        assert!(
            require_capability_online(&state, "srv-1", CAP_FILE)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn audited_variant_logs_denial() {
        use sea_orm::EntityTrait;
        let (state, _tmp) = setup_state_with_server(CAP_FILE).await;
        let denied = require_capability_audited(
            &state,
            "srv-1",
            CAP_DOCKER,
            "user-1",
            "127.0.0.1",
            "docker_denied_test",
        )
        .await;
        assert!(denied.is_err());

        let logs = crate::entity::audit_log::Entity::find()
            .all(&state.db)
            .await
            .unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].action, "docker_denied_test");
        assert!(logs[0].detail.as_deref().unwrap().contains("deny_reason"));
    }
}
