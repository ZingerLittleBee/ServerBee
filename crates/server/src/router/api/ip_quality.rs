use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::{ApiResponse, AppError, ok};
use crate::service::ip_quality::{
    CreateCustomServiceInput, IpQualityService, IpQualitySettingDto, ServerIpQualityData,
    UnlockEventDto, UpdateServiceInput,
};
use crate::state::AppState;
use serverbee_common::constants::{CAP_IP_QUALITY, has_capability};
use serverbee_common::protocol::ServerMessage;

// ---------------------------------------------------------------------------
// Router construction
// ---------------------------------------------------------------------------

/// Read-only routes — accessible to all authenticated users.
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ip-quality/services", get(list_services))
        .route("/ip-quality/settings", get(get_settings))
        .route("/ip-quality/overview", get(get_overview))
        .route("/ip-quality/servers/{id}", get(get_server_summary))
        .route("/ip-quality/events", get(list_events))
}

/// Write routes — restricted to admin users only (layered with `require_admin`
/// middleware by the caller in `router/api/mod.rs`).
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ip-quality/services", post(create_service))
        .route(
            "/ip-quality/services/{id}",
            put(update_service).delete(delete_service),
        )
        .route("/ip-quality/settings", put(update_settings))
        .route("/ip-quality/servers/{id}/check", post(check_server))
}

// ---------------------------------------------------------------------------
// IpQualitySync re-broadcast helper
// ---------------------------------------------------------------------------

/// Re-send `IpQualitySync` to every currently-online agent.
///
/// Spec §4 requires `IpQualitySync` to be pushed on connect, on catalog
/// change, and on settings change. The WS handler covers the connect case;
/// this helper covers catalog/settings mutations so a change reaches already
/// connected agents without waiting for them to reconnect. Mirrors how
/// `network_probe.rs` re-broadcasts `NetworkProbeSync` after a mutation.
async fn broadcast_ip_quality_sync(state: &Arc<AppState>) -> Result<(), AppError> {
    let services = IpQualityService::enabled_service_defs(&state.db).await?;
    let setting = IpQualityService::get_setting(&state.db).await?;

    for server_id in state.agent_manager.connected_server_ids() {
        if let Some(tx) = state.agent_manager.get_sender(&server_id) {
            let _ = tx
                .send(ServerMessage::IpQualitySync {
                    services: services.clone(),
                    interval_hours: setting.check_interval_hours as u32,
                })
                .await;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct EventsQuery {
    pub server_id: String,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_limit() -> u64 {
    100
}

// ---------------------------------------------------------------------------
// Read handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ip-quality/services",
    tag = "ip-quality",
    responses(
        (status = 200, description = "List all unlock services (built-in + custom)", body = Vec<crate::entity::unlock_service::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_services(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<crate::entity::unlock_service::Model>>>, AppError> {
    let services = IpQualityService::list_services(&state.db).await?;
    ok(services)
}

#[utoipa::path(
    get,
    path = "/api/ip-quality/settings",
    tag = "ip-quality",
    operation_id = "ip_quality_get_settings",
    responses(
        (status = 200, description = "Global IP quality settings", body = IpQualitySettingDto),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<IpQualitySettingDto>>, AppError> {
    let setting = IpQualityService::get_setting(&state.db).await?;
    ok(setting)
}

#[utoipa::path(
    get,
    path = "/api/ip-quality/overview",
    tag = "ip-quality",
    responses(
        (status = 200, description = "IP quality overview for all servers", body = Vec<ServerIpQualityData>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<ServerIpQualityData>>>, AppError> {
    let overview = IpQualityService::get_overview(&state.db).await?;
    ok(overview)
}

#[utoipa::path(
    get,
    path = "/api/ip-quality/servers/{id}",
    tag = "ip-quality",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "IP quality data for a server", body = ServerIpQualityData),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_server_summary(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ServerIpQualityData>>, AppError> {
    let summary = IpQualityService::get_server_summary(&state.db, &id).await?;
    ok(summary)
}

#[utoipa::path(
    get,
    path = "/api/ip-quality/events",
    tag = "ip-quality",
    operation_id = "ip_quality_list_events",
    params(EventsQuery),
    responses(
        (status = 200, description = "IP quality status-change events for a server", body = Vec<UnlockEventDto>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<EventsQuery>,
) -> Result<Json<ApiResponse<Vec<UnlockEventDto>>>, AppError> {
    let events = IpQualityService::list_events(&state.db, &q.server_id, q.limit).await?;
    ok(events)
}

// ---------------------------------------------------------------------------
// Write handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ip-quality/services",
    tag = "ip-quality",
    request_body = CreateCustomServiceInput,
    responses(
        (status = 200, description = "Custom unlock service created"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn create_service(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateCustomServiceInput>,
) -> Result<Json<ApiResponse<crate::entity::unlock_service::Model>>, AppError> {
    let service = IpQualityService::create_custom_service(&state.db, input).await?;
    if let Err(e) = broadcast_ip_quality_sync(&state).await {
        tracing::warn!("IpQualitySync broadcast failed: {e}");
    }
    ok(service)
}

#[utoipa::path(
    put,
    path = "/api/ip-quality/services/{id}",
    tag = "ip-quality",
    params(("id" = String, Path, description = "Service ID")),
    request_body = UpdateServiceInput,
    responses(
        (status = 200, description = "Service updated"),
        (status = 404, description = "Service not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn update_service(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateServiceInput>,
) -> Result<Json<ApiResponse<crate::entity::unlock_service::Model>>, AppError> {
    let service = IpQualityService::update_service(&state.db, &id, input).await?;
    if let Err(e) = broadcast_ip_quality_sync(&state).await {
        tracing::warn!("IpQualitySync broadcast failed: {e}");
    }
    ok(service)
}

#[utoipa::path(
    delete,
    path = "/api/ip-quality/services/{id}",
    tag = "ip-quality",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service deleted"),
        (status = 400, description = "Cannot delete a built-in service"),
        (status = 404, description = "Service not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn delete_service(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    IpQualityService::delete_service(&state.db, &id).await?;
    if let Err(e) = broadcast_ip_quality_sync(&state).await {
        tracing::warn!("IpQualitySync broadcast failed: {e}");
    }
    ok("ok")
}

#[utoipa::path(
    put,
    path = "/api/ip-quality/settings",
    tag = "ip-quality",
    operation_id = "ip_quality_update_settings",
    request_body = IpQualitySettingDto,
    responses(
        (status = 200, description = "Settings updated", body = IpQualitySettingDto),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IpQualitySettingDto>,
) -> Result<Json<ApiResponse<IpQualitySettingDto>>, AppError> {
    let setting =
        IpQualityService::update_setting(&state.db, input.check_interval_hours).await?;
    if let Err(e) = broadcast_ip_quality_sync(&state).await {
        tracing::warn!("IpQualitySync broadcast failed: {e}");
    }
    ok(setting)
}

#[utoipa::path(
    post,
    path = "/api/ip-quality/servers/{id}/check",
    tag = "ip-quality",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "IP quality check triggered"),
        (status = 404, description = "Server agent is not online"),
        (status = 409, description = "CAP_IP_QUALITY is not effective for this server"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn check_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    let tx = state
        .agent_manager
        .get_sender(&id)
        .ok_or_else(|| AppError::NotFound(format!("Server {id} is not online")))?;

    // Guard: do not send IpQualityRunNow if the agent did not enable the
    // capability. The agent would silently ignore the message, giving the UI
    // false success. Capabilities are agent-owned, so the only fix is to edit
    // the agent's config file (or CLI flags) on the host.
    let agent_has = state
        .agent_manager
        .get_agent_local_capabilities(&id)
        .map(|caps| has_capability(caps, CAP_IP_QUALITY))
        .unwrap_or(false);

    if !agent_has {
        return Err(AppError::Conflict(
            "IP Quality is disabled by the agent. Enable it in the agent's config \
             file ([capabilities] allow = [\"ip_quality\"]) or with --allow-cap \
             ip_quality, then restart the agent."
                .to_string(),
        ));
    }

    tx.send(ServerMessage::IpQualityRunNow)
        .await
        .map_err(|_| AppError::Internal("Failed to send IpQualityRunNow to agent".to_string()))?;

    ok("ok")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::test_utils::setup_test_db;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::sync::mpsc;

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    #[tokio::test]
    async fn broadcast_ip_quality_sync_reaches_online_agents() {
        let (db, _tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();

        // Register a connected agent with a receiving channel.
        let (tx, mut rx) = mpsc::channel::<ServerMessage>(8);
        state
            .agent_manager
            .add_connection("srv-online".into(), "Online".into(), tx, test_addr());

        broadcast_ip_quality_sync(&state).await.unwrap();

        // The online agent should receive an IpQualitySync with the 9 seeded
        // built-in services and the default 12h interval.
        let msg = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("agent should receive a message")
            .expect("channel should not be closed");

        match msg {
            ServerMessage::IpQualitySync {
                services,
                interval_hours,
            } => {
                assert_eq!(services.len(), 9, "all 9 enabled built-ins should be synced");
                assert_eq!(interval_hours, 12, "default interval is 12h");
            }
            other => panic!("expected IpQualitySync, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn broadcast_ip_quality_sync_with_no_agents_is_noop() {
        let (db, _tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();

        // No connected agents — must succeed without error.
        broadcast_ip_quality_sync(&state).await.unwrap();
    }

    // -----------------------------------------------------------------------
    // FIX 3: check_server returns 409 when CAP_IP_QUALITY is not effective
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn check_server_returns_conflict_when_agent_lacks_capability() {
        use serverbee_common::constants::{CAP_DEFAULT, CAP_IP_QUALITY};

        let (db, _tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();

        // Register a connected agent whose reported caps lack CAP_IP_QUALITY.
        // Capabilities are agent-owned, so this is the only thing that gates it.
        let caps_without_ip_quality = CAP_DEFAULT & !CAP_IP_QUALITY;
        let (tx, _rx) = mpsc::channel::<ServerMessage>(8);
        state
            .agent_manager
            .add_connection("srv-no-cap".into(), "NoCap".into(), tx, test_addr());
        state
            .agent_manager
            .update_agent_local_capabilities("srv-no-cap", caps_without_ip_quality);

        let result = check_server(
            axum::extract::State(state),
            axum::extract::Path("srv-no-cap".to_string()),
        )
        .await;

        match result {
            Err(AppError::Conflict(msg)) => {
                assert!(
                    msg.contains("agent's config file") && msg.contains("ip_quality"),
                    "conflict message should point at the agent config file; got: {msg}"
                );
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn check_server_succeeds_when_agent_reports_capability() {
        use serverbee_common::constants::CAP_DEFAULT;

        let (db, _tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();

        let (tx, _rx) = mpsc::channel::<ServerMessage>(8);
        state
            .agent_manager
            .add_connection("srv".into(), "Srv".into(), tx, test_addr());
        // CAP_DEFAULT includes CAP_IP_QUALITY, so the guard should pass.
        state
            .agent_manager
            .update_agent_local_capabilities("srv", CAP_DEFAULT);

        let result = check_server(
            axum::extract::State(state),
            axum::extract::Path("srv".to_string()),
        )
        .await;

        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }
}
