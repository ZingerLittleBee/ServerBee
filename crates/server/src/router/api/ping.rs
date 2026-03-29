use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::entity::{ping_record, ping_task};
use crate::error::{ApiResponse, AppError, ok};
use crate::service::ping::{CreatePingTask, PingService, UpdatePingTask};
use crate::state::AppState;

/// GET endpoints accessible to all authenticated users (admin + member).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ping-tasks", get(list_tasks))
        .route("/ping-tasks/{id}", get(get_task))
        .route("/ping-tasks/{id}/records", get(get_records))
}

/// Write endpoints (POST/PUT/DELETE) restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ping-tasks", post(create_task))
        .route("/ping-tasks/{id}", put(update_task))
        .route("/ping-tasks/{id}", delete(delete_task))
}

#[utoipa::path(
    get,
    path = "/api/ping-tasks",
    operation_id = "list_ping_tasks",
    tag = "ping-tasks",
    responses(
        (status = 200, description = "List all ping tasks", body = Vec<ping_task::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_tasks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<ping_task::Model>>>, AppError> {
    let tasks = PingService::list(&state.db).await?;
    ok(tasks)
}

#[utoipa::path(
    get,
    path = "/api/ping-tasks/{id}",
    operation_id = "get_ping_task",
    tag = "ping-tasks",
    params(("id" = String, Path, description = "Ping task ID")),
    responses(
        (status = 200, description = "Ping task details", body = ping_task::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ping_task::Model>>, AppError> {
    let task = PingService::get(&state.db, &id).await?;
    ok(task)
}

#[utoipa::path(
    post,
    path = "/api/ping-tasks",
    operation_id = "create_ping_task",
    tag = "ping-tasks",
    request_body = CreatePingTask,
    responses(
        (status = 200, description = "Ping task created", body = ping_task::Model),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreatePingTask>,
) -> Result<Json<ApiResponse<ping_task::Model>>, AppError> {
    let task = PingService::create(&state.db, &state.agent_manager, input).await?;
    ok(task)
}

#[utoipa::path(
    put,
    path = "/api/ping-tasks/{id}",
    operation_id = "update_ping_task",
    tag = "ping-tasks",
    params(("id" = String, Path, description = "Ping task ID")),
    request_body = UpdatePingTask,
    responses(
        (status = 200, description = "Ping task updated", body = ping_task::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn update_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdatePingTask>,
) -> Result<Json<ApiResponse<ping_task::Model>>, AppError> {
    let task = PingService::update(&state.db, &state.agent_manager, &id, input).await?;
    ok(task)
}

#[utoipa::path(
    delete,
    path = "/api/ping-tasks/{id}",
    operation_id = "delete_ping_task",
    tag = "ping-tasks",
    params(("id" = String, Path, description = "Ping task ID")),
    responses(
        (status = 200, description = "Ping task deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    PingService::delete(&state.db, &state.agent_manager, &id).await?;
    ok("ok")
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct RecordsQuery {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    server_id: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/ping-tasks/{id}/records",
    operation_id = "get_ping_records",
    tag = "ping-tasks",
    params(
        ("id" = String, Path, description = "Ping task ID"),
        RecordsQuery,
    ),
    responses(
        (status = 200, description = "Ping records", body = Vec<ping_record::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_records(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Query(q): Query<RecordsQuery>,
) -> Result<Json<ApiResponse<Vec<ping_record::Model>>>, AppError> {
    let records =
        PingService::get_records(&state.db, &task_id, q.from, q.to, q.server_id.as_deref()).await?;
    ok(records)
}
