use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ok, ApiResponse, AppError};
use crate::service::docker::DockerService;
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::constants::{has_capability, CAP_DOCKER};
use serverbee_common::docker_types::*;
use serverbee_common::protocol::{AgentMessage, ServerMessage};

// ---------------------------------------------------------------------------
// Request / Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ContainersResponse {
    containers: Vec<DockerContainer>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatsResponse {
    stats: Vec<DockerContainerStats>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DockerInfoResponse {
    info: DockerSystemInfo,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EventsResponse {
    events: Vec<DockerEventInfo>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct EventsQueryParams {
    #[serde(default = "default_events_limit")]
    limit: u64,
}

fn default_events_limit() -> u64 {
    100
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct NetworksResponse {
    networks: Vec<DockerNetwork>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VolumesResponse {
    volumes: Vec<DockerVolume>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ContainerActionRequest {
    action: DockerAction,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActionResultResponse {
    success: bool,
    error: Option<String>,
}

// ---------------------------------------------------------------------------
// Routers
// ---------------------------------------------------------------------------

/// Read endpoints accessible to all authenticated users (admin + member).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/servers/{id}/docker/containers",
            get(get_containers),
        )
        .route("/servers/{id}/docker/stats", get(get_stats))
        .route("/servers/{id}/docker/info", get(get_info))
        .route("/servers/{id}/docker/events", get(get_events))
        .route(
            "/servers/{id}/docker/networks",
            get(get_networks),
        )
        .route(
            "/servers/{id}/docker/volumes",
            get(get_volumes),
        )
}

/// Write endpoints restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/servers/{id}/docker/containers/{cid}/action",
        post(container_action),
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Guard: checks both capability bit (CAP_DOCKER) and runtime feature ("docker").
async fn require_docker(state: &AppState, server_id: &str) -> Result<(), AppError> {
    let server = ServerService::get_server(&state.db, server_id).await?;
    let caps = server.capabilities as u32;
    if !has_capability(caps, CAP_DOCKER) {
        return Err(AppError::Forbidden(
            "Docker capability disabled for this server".into(),
        ));
    }
    if !state.agent_manager.has_feature(server_id, "docker") {
        return Err(AppError::Forbidden(
            "Docker is not available on this server".into(),
        ));
    }
    if !state.agent_manager.is_online(server_id) {
        return Err(AppError::NotFound("Server offline".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Read handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/servers/{id}/docker/containers",
    tag = "docker",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Cached containers list", body = ContainersResponse),
        (status = 403, description = "Docker capability disabled"),
        (status = 404, description = "Server not found or offline"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_containers(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ContainersResponse>>, AppError> {
    require_docker(&state, &id).await?;

    let containers = state
        .agent_manager
        .get_docker_containers(&id)
        .unwrap_or_default();
    ok(ContainersResponse { containers })
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/docker/stats",
    tag = "docker",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Cached container stats", body = StatsResponse),
        (status = 403, description = "Docker capability disabled"),
        (status = 404, description = "Server not found or offline"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_stats(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<StatsResponse>>, AppError> {
    require_docker(&state, &id).await?;

    let stats = state
        .agent_manager
        .get_docker_stats(&id)
        .unwrap_or_default();
    ok(StatsResponse { stats })
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/docker/info",
    tag = "docker",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Cached Docker system info", body = DockerInfoResponse),
        (status = 403, description = "Docker capability disabled"),
        (status = 404, description = "Server not found or no info cached"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_info(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<DockerInfoResponse>>, AppError> {
    require_docker(&state, &id).await?;

    let info = state
        .agent_manager
        .get_docker_info(&id)
        .ok_or_else(|| AppError::NotFound("Docker info not yet available".into()))?;
    ok(DockerInfoResponse { info })
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/docker/events",
    tag = "docker",
    params(
        ("id" = String, Path, description = "Server ID"),
        EventsQueryParams,
    ),
    responses(
        (status = 200, description = "Docker events from DB", body = EventsResponse),
        (status = 403, description = "Docker capability disabled"),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<EventsQueryParams>,
) -> Result<Json<ApiResponse<EventsResponse>>, AppError> {
    // For events, we only need the server to exist and have the capability.
    // The server doesn't need to be online (events are persisted in DB).
    let server = ServerService::get_server(&state.db, &id).await?;
    let caps = server.capabilities as u32;
    if !has_capability(caps, CAP_DOCKER) {
        return Err(AppError::Forbidden(
            "Docker capability disabled for this server".into(),
        ));
    }

    let events = DockerService::get_events(&state.db, &id, params.limit)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to query docker events: {e}")))?;
    ok(EventsResponse { events })
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/docker/networks",
    tag = "docker",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Docker networks", body = NetworksResponse),
        (status = 403, description = "Docker capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_networks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<NetworksResponse>>, AppError> {
    require_docker(&state, &id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state
        .agent_manager
        .register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::DockerListNetworks {
            msg_id: msg_id.clone(),
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::DockerNetworks { networks, .. })) => {
            ok(NetworksResponse { networks })
        }
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/docker/volumes",
    tag = "docker",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Docker volumes", body = VolumesResponse),
        (status = 403, description = "Docker capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_volumes(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<VolumesResponse>>, AppError> {
    require_docker(&state, &id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state
        .agent_manager
        .register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::DockerListVolumes {
            msg_id: msg_id.clone(),
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::DockerVolumes { volumes, .. })) => ok(VolumesResponse { volumes }),
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Write handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/servers/{id}/docker/containers/{cid}/action",
    tag = "docker",
    params(
        ("id" = String, Path, description = "Server ID"),
        ("cid" = String, Path, description = "Container ID"),
    ),
    request_body = ContainerActionRequest,
    responses(
        (status = 200, description = "Action result", body = ActionResultResponse),
        (status = 403, description = "Docker capability disabled"),
        (status = 404, description = "Server not found or offline"),
        (status = 408, description = "Agent timeout"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn container_action(
    State(state): State<Arc<AppState>>,
    Path((id, cid)): Path<(String, String)>,
    Json(body): Json<ContainerActionRequest>,
) -> Result<Json<ApiResponse<ActionResultResponse>>, AppError> {
    require_docker(&state, &id).await?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state
        .agent_manager
        .register_pending_request(msg_id.clone());

    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or(AppError::NotFound("Server offline".into()))?;
    sender
        .send(ServerMessage::DockerContainerAction {
            msg_id: msg_id.clone(),
            container_id: cid,
            action: body.action,
        })
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::DockerActionResult {
            success, error, ..
        })) => ok(ActionResultResponse { success, error }),
        Ok(Ok(_)) => Err(AppError::Internal("Unexpected response from agent".into())),
        Ok(Err(_)) => Err(AppError::Internal("Agent disconnected".into())),
        Err(_) => Err(AppError::RequestTimeout(
            "Agent did not respond within 30s".into(),
        )),
    }
}
