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
    ok("ok")
}

#[utoipa::path(
    put,
    path = "/api/ip-quality/settings",
    tag = "ip-quality",
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

    tx.send(ServerMessage::IpQualityRunNow)
        .await
        .map_err(|_| AppError::Internal("Failed to send IpQualityRunNow to agent".to_string()))?;

    ok("ok")
}
