use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ApiResponse, AppError, ok};
use crate::state::AppState;
use serverbee_common::protocol::ServerMessage;
use serverbee_common::types::TracerouteHop;

// --- Request / Response types ---

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TriggerTracerouteRequest {
    /// Target host or IP (e.g. "1.2.3.4" or "example.com")
    pub target: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TriggerTracerouteResponse {
    /// Unique request ID used to poll for results
    pub request_id: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TracerouteResultResponse {
    pub target: String,
    pub hops: Vec<TracerouteHop>,
    pub completed: bool,
    pub error: Option<String>,
}

// --- Routers ---

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/servers/{id}/traceroute/{request_id}",
        get(get_traceroute_result),
    )
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route("/servers/{id}/traceroute", post(trigger_traceroute))
}

// --- Handlers ---

/// Trigger a traceroute to a target from the specified server's agent.
#[utoipa::path(
    post,
    path = "/api/servers/{id}/traceroute",
    tag = "traceroute",
    params(("id" = String, Path, description = "Server ID")),
    request_body = TriggerTracerouteRequest,
    responses(
        (status = 200, description = "Traceroute triggered", body = TriggerTracerouteResponse),
        (status = 404, description = "Server not found or offline"),
        (status = 422, description = "Invalid target"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn trigger_traceroute(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(input): Json<TriggerTracerouteRequest>,
) -> Result<Json<ApiResponse<TriggerTracerouteResponse>>, AppError> {
    // Validate target: only allow alphanumeric, dots, hyphens, and colons
    if input.target.is_empty()
        || !input
            .target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
    {
        return Err(AppError::Validation(
            "Invalid target: only alphanumeric characters, dots, hyphens, and colons are allowed"
                .to_string(),
        ));
    }

    // Check server is online
    let tx = state
        .agent_manager
        .get_sender(&server_id)
        .ok_or_else(|| AppError::NotFound(format!("Server {server_id} is not online")))?;

    // Generate request_id
    let request_id = Uuid::new_v4().to_string();

    // Insert placeholder in traceroute_results cache
    state
        .agent_manager
        .insert_traceroute_placeholder(&request_id, &server_id, &input.target);

    // Send Traceroute command to agent
    let msg = ServerMessage::Traceroute {
        request_id: request_id.clone(),
        target: input.target,
        max_hops: 30,
    };
    tx.send(msg).await.map_err(|_| {
        AppError::Internal("Failed to send traceroute command to agent".to_string())
    })?;

    ok(TriggerTracerouteResponse { request_id })
}

/// Poll for the result of a previously triggered traceroute.
#[utoipa::path(
    get,
    path = "/api/servers/{id}/traceroute/{request_id}",
    tag = "traceroute",
    params(
        ("id" = String, Path, description = "Server ID"),
        ("request_id" = String, Path, description = "Traceroute request ID"),
    ),
    responses(
        (status = 200, description = "Traceroute result", body = TracerouteResultResponse),
        (status = 404, description = "Result not found or server mismatch"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn get_traceroute_result(
    State(state): State<Arc<AppState>>,
    Path((server_id, request_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<TracerouteResultResponse>>, AppError> {
    let (stored_server_id, result) = state
        .agent_manager
        .get_traceroute_result(&request_id)
        .ok_or_else(|| AppError::NotFound(format!("Traceroute result {request_id} not found")))?;

    // Validate server_id matches
    if stored_server_id != server_id {
        return Err(AppError::NotFound(format!(
            "Traceroute result {request_id} not found"
        )));
    }

    ok(TracerouteResultResponse {
        target: result.target,
        hops: result.hops,
        completed: result.completed,
        error: result.error,
    })
}
