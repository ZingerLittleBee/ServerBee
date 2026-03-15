use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::{ok, ApiResponse, AppError};
use crate::service::network_probe::{
    CreateNetworkProbeTarget, NetworkProbeAnomaly, NetworkProbeSetting, NetworkProbeService,
    ServerOverview, TargetDto, UpdateNetworkProbeTarget,
};
use crate::state::AppState;
use serverbee_common::protocol::ServerMessage;
use serverbee_common::types::NetworkProbeTarget;

/// GET endpoints accessible to all authenticated users (admin + member).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/network-probes/targets", get(list_targets))
        .route("/network-probes/setting", get(get_setting))
        .route("/network-probes/overview", get(get_overview))
}

/// Write endpoints (POST/PUT/DELETE) restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/network-probes/targets", post(create_target))
        .route("/network-probes/targets/{id}", put(update_target))
        .route("/network-probes/targets/{id}", delete(delete_target))
        .route("/network-probes/setting", put(update_setting))
}

// ---------------------------------------------------------------------------
// Read handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/network-probes/targets",
    tag = "network-probes",
    responses(
        (status = 200, description = "List all network probe targets"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_targets(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<TargetDto>>>, AppError> {
    let targets = NetworkProbeService::list_targets(&state.db).await?;
    ok(targets)
}

#[utoipa::path(
    get,
    path = "/api/network-probes/setting",
    tag = "network-probes",
    responses(
        (status = 200, description = "Network probe global setting", body = NetworkProbeSetting),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_setting(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<NetworkProbeSetting>>, AppError> {
    let setting = NetworkProbeService::get_setting(&state.db).await?;
    ok(setting)
}

#[utoipa::path(
    get,
    path = "/api/network-probes/overview",
    tag = "network-probes",
    responses(
        (status = 200, description = "Network probe overview for all servers", body = Vec<ServerOverview>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<ServerOverview>>>, AppError> {
    let overview = NetworkProbeService::get_overview(&state.db, &state.agent_manager).await?;
    ok(overview)
}

// ---------------------------------------------------------------------------
// Write handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/network-probes/targets",
    tag = "network-probes",
    request_body = CreateNetworkProbeTarget,
    responses(
        (status = 200, description = "Network probe target created"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_target(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateNetworkProbeTarget>,
) -> Result<Json<ApiResponse<crate::entity::network_probe_target::Model>>, AppError> {
    let target = NetworkProbeService::create_target(&state.db, input).await?;
    ok(target)
}

#[utoipa::path(
    put,
    path = "/api/network-probes/targets/{id}",
    tag = "network-probes",
    params(("id" = String, Path, description = "Target ID")),
    request_body = UpdateNetworkProbeTarget,
    responses(
        (status = 200, description = "Network probe target updated"),
        (status = 404, description = "Target not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn update_target(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateNetworkProbeTarget>,
) -> Result<Json<ApiResponse<crate::entity::network_probe_target::Model>>, AppError> {
    let target = NetworkProbeService::update_target(&state.db, &id, input).await?;
    ok(target)
}

#[utoipa::path(
    delete,
    path = "/api/network-probes/targets/{id}",
    tag = "network-probes",
    params(("id" = String, Path, description = "Target ID")),
    responses(
        (status = 200, description = "Network probe target deleted"),
        (status = 404, description = "Target not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_target(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    // Find which servers have this target configured, before deletion
    use crate::entity::network_probe_config;
    use sea_orm::EntityTrait;
    use sea_orm::QueryFilter;
    use sea_orm::ColumnTrait;

    let affected_configs = network_probe_config::Entity::find()
        .filter(network_probe_config::Column::TargetId.eq(id.as_str()))
        .all(&state.db)
        .await?;
    let affected_server_ids: Vec<String> =
        affected_configs.into_iter().map(|c| c.server_id).collect();

    // Delete the target (cascades records + configs + setting cleanup)
    NetworkProbeService::delete_target(&state.db, &id).await?;

    // Notify affected agents
    let setting = NetworkProbeService::get_setting(&state.db).await?;
    for server_id in &affected_server_ids {
        let targets = NetworkProbeService::get_server_targets(&state.db, server_id).await?;
        let probe_targets: Vec<NetworkProbeTarget> = targets
            .into_iter()
            .map(|t| NetworkProbeTarget {
                target_id: t.id,
                name: t.name,
                target: t.target,
                probe_type: t.probe_type,
            })
            .collect();

        if let Some(tx) = state.agent_manager.get_sender(server_id) {
            let msg = ServerMessage::NetworkProbeSync {
                targets: probe_targets,
                interval: setting.interval,
                packet_count: setting.packet_count,
            };
            let _ = tx.send(msg).await;
        }
    }

    ok("ok")
}

#[utoipa::path(
    put,
    path = "/api/network-probes/setting",
    tag = "network-probes",
    request_body = NetworkProbeSetting,
    responses(
        (status = 200, description = "Network probe setting updated"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn update_setting(
    State(state): State<Arc<AppState>>,
    Json(input): Json<NetworkProbeSetting>,
) -> Result<Json<ApiResponse<NetworkProbeSetting>>, AppError> {
    NetworkProbeService::update_setting(&state.db, &input).await?;

    // Push updated config to all currently-online agents
    let online_ids = state.agent_manager.connected_server_ids();
    for server_id in &online_ids {
        let targets = NetworkProbeService::get_server_targets(&state.db, server_id).await?;
        let probe_targets: Vec<NetworkProbeTarget> = targets
            .into_iter()
            .map(|t| NetworkProbeTarget {
                target_id: t.id,
                name: t.name,
                target: t.target,
                probe_type: t.probe_type,
            })
            .collect();

        if let Some(tx) = state.agent_manager.get_sender(server_id) {
            let msg = ServerMessage::NetworkProbeSync {
                targets: probe_targets,
                interval: input.interval,
                packet_count: input.packet_count,
            };
            let _ = tx.send(msg).await;
        }
    }

    ok(input)
}

// ---------------------------------------------------------------------------
// Per-server read handlers (mounted in server.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct NetworkProbeRecordQuery {
    pub from: chrono::DateTime<chrono::Utc>,
    pub to: chrono::DateTime<chrono::Utc>,
    pub target_id: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct NetworkProbeAnomalyQuery {
    pub from: chrono::DateTime<chrono::Utc>,
    pub to: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/network-probes/targets",
    operation_id = "get_server_network_targets",
    tag = "network-probes",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Network probe targets for server"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn get_server_network_targets(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<TargetDto>>>, AppError> {
    let targets = NetworkProbeService::get_server_targets(&state.db, &id).await?;
    ok(targets)
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/network-probes/records",
    operation_id = "get_server_network_records",
    tag = "network-probes",
    params(
        ("id" = String, Path, description = "Server ID"),
        NetworkProbeRecordQuery,
    ),
    responses(
        (status = 200, description = "Network probe records for server"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn get_server_network_records(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<NetworkProbeRecordQuery>,
) -> Result<Json<ApiResponse<Vec<crate::service::network_probe::ProbeRecordDto>>>, AppError> {
    let records =
        NetworkProbeService::query_records(&state.db, &id, q.target_id, q.from, q.to).await?;
    ok(records)
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/network-probes/summary",
    operation_id = "get_server_network_summary",
    tag = "network-probes",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Network probe summary for server", body = crate::service::network_probe::ServerSummary),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn get_server_network_summary(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<crate::service::network_probe::ServerSummary>>, AppError> {
    let summary =
        NetworkProbeService::get_server_summary(&state.db, &state.agent_manager, &id).await?;
    ok(summary)
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/network-probes/anomalies",
    operation_id = "get_server_network_anomalies",
    tag = "network-probes",
    params(
        ("id" = String, Path, description = "Server ID"),
        NetworkProbeAnomalyQuery,
    ),
    responses(
        (status = 200, description = "Network probe anomalies for server", body = Vec<NetworkProbeAnomaly>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn get_server_network_anomalies(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<NetworkProbeAnomalyQuery>,
) -> Result<Json<ApiResponse<Vec<NetworkProbeAnomaly>>>, AppError> {
    let anomalies = NetworkProbeService::get_anomalies(&state.db, &id, q.from, q.to).await?;
    ok(anomalies)
}
