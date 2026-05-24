use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiResponse, AppError, ok};
use crate::service::traceroute::{
    self, TracerouteRecordSummary, TracerouteSnapshotResponse,
};
use crate::state::AppState;
use serverbee_common::protocol::{RecordedProtocol, ServerMessage, TraceProtocol};

const MAX_HOPS: u8 = 30;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TriggerTracerouteRequest {
    /// Target host or IP (e.g. "1.2.3.4" or "example.com").
    pub target: String,
    /// One of `icmp` | `udp` | `tcp`. Missing → defaults to `icmp`.
    #[serde(default)]
    pub protocol: Option<TraceProtocol>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct TriggerTracerouteResponse {
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 { 50 }

// ---------- Routers ----------

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/servers/{id}/traceroute/{request_id}",
            get(get_traceroute_snapshot),
        )
        .route("/servers/{id}/traceroute", get(list_traceroute_records))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers/{id}/traceroute", post(trigger_traceroute))
        .route(
            "/servers/{id}/traceroute/{request_id}",
            delete(delete_traceroute_record),
        )
        .route("/servers/{id}/traceroute", delete(clear_traceroute_history))
}

// ---------- Handlers ----------

#[utoipa::path(
    post, path = "/api/servers/{id}/traceroute", tag = "traceroute",
    params(("id" = String, Path, description = "Server ID")),
    request_body = TriggerTracerouteRequest,
    responses(
        (status = 200, body = TriggerTracerouteResponse),
        (status = 404), (status = 422),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn trigger_traceroute(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(input): Json<TriggerTracerouteRequest>,
) -> Result<Json<ApiResponse<TriggerTracerouteResponse>>, AppError> {
    if input.target.is_empty()
        || !input.target.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
    {
        return Err(AppError::Validation(
            "Invalid target: only alphanumeric characters, dots, hyphens, and colons are allowed".to_string(),
        ));
    }
    let tx = state.agent_manager.get_sender(&server_id)
        .ok_or_else(|| AppError::NotFound(format!("Server {server_id} is not online")))?;
    let request_id = Uuid::new_v4().to_string();
    let protocol = input.protocol.unwrap_or(TraceProtocol::Icmp);

    state.agent_manager.insert_traceroute_placeholder(
        &request_id,
        crate::service::agent_manager::TracerouteRequestMeta {
            server_id: server_id.clone(),
            target: input.target.clone(),
            protocol: RecordedProtocol::from(protocol),
            started_at: chrono::Utc::now().timestamp_millis(),
        },
    );

    let msg = ServerMessage::Traceroute {
        request_id: request_id.clone(),
        target: input.target,
        max_hops: MAX_HOPS,
        protocol: Some(protocol),
    };
    tx.send(msg).await
        .map_err(|_| AppError::Internal("Failed to send traceroute command to agent".to_string()))?;

    ok(TriggerTracerouteResponse { request_id })
}

#[utoipa::path(
    get, path = "/api/servers/{id}/traceroute/{request_id}", tag = "traceroute",
    params(("id" = String, Path), ("request_id" = String, Path)),
    responses((status = 200, body = TracerouteSnapshotResponse), (status = 404)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_traceroute_snapshot(
    State(state): State<Arc<AppState>>,
    Path((server_id, request_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<TracerouteSnapshotResponse>>, AppError> {
    // 1. In-memory cache (live or recently-completed)
    if let Some(snap) = state.agent_manager.get_traceroute_snapshot(&request_id)
        && snap.server_id == server_id
    {
        return ok(TracerouteSnapshotResponse {
            request_id,
            target: snap.target,
            protocol: snap.protocol,
            started_at: snap.started_at,
            completed_at: if snap.completed { Some(chrono::Utc::now().timestamp_millis()) } else { None },
            round: snap.round,
            total_rounds: snap.total_rounds,
            completed: snap.completed,
            hops: snap.hops,
            error: snap.error,
        });
    }
    // 2. DB fallback
    if let Some(snap) = traceroute::get_record_snapshot(&state.db, &server_id, &request_id).await? {
        return ok(snap);
    }
    Err(AppError::NotFound(format!("Traceroute {request_id} not found")))
}

#[utoipa::path(
    get, path = "/api/servers/{id}/traceroute", tag = "traceroute",
    params(("id" = String, Path)),
    responses((status = 200, body = Vec<TracerouteRecordSummary>)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_traceroute_records(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiResponse<Vec<TracerouteRecordSummary>>>, AppError> {
    let rows = traceroute::list_records_for_server(&state.db, &server_id, q.limit, q.offset).await?;
    ok(rows)
}

#[utoipa::path(
    delete, path = "/api/servers/{id}/traceroute/{request_id}", tag = "traceroute",
    params(("id" = String, Path), ("request_id" = String, Path)),
    responses((status = 204), (status = 404)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn delete_traceroute_record(
    State(state): State<Arc<AppState>>,
    Path((server_id, request_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    traceroute::delete_record(&state.db, &server_id, &request_id).await?;
    ok(())
}

#[utoipa::path(
    delete, path = "/api/servers/{id}/traceroute", tag = "traceroute",
    params(("id" = String, Path)),
    responses((status = 200, body = ClearedResponse)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn clear_traceroute_history(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Result<Json<ApiResponse<ClearedResponse>>, AppError> {
    let deleted = traceroute::delete_records_for_server(&state.db, &server_id).await?;
    ok(ClearedResponse { deleted })
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ClearedResponse {
    pub deleted: u64,
}
