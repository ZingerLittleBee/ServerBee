use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{task, task_result};
use crate::error::{ok, ApiResponse, AppError};
use crate::state::AppState;
use serverbee_common::protocol::ServerMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tasks", post(create_task))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}/results", get(get_task_results))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateTaskRequest {
    command: String,
    server_ids: Vec<String>,
    #[serde(default)]
    timeout: Option<u32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TaskResponse {
    id: String,
    command: String,
    server_ids: Vec<String>,
    created_at: chrono::DateTime<Utc>,
}

#[utoipa::path(
    post,
    path = "/api/tasks",
    tag = "tasks",
    request_body = CreateTaskRequest,
    responses(
        (status = 200, description = "Task created and dispatched", body = TaskResponse),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateTaskRequest>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    if input.server_ids.is_empty() {
        return Err(AppError::Validation("server_ids cannot be empty".to_string()));
    }
    if input.command.trim().is_empty() {
        return Err(AppError::Validation("command cannot be empty".to_string()));
    }

    let task_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let server_ids_json = serde_json::to_string(&input.server_ids)
        .map_err(|e| AppError::Internal(format!("Serialization error: {e}")))?;

    // Save task to DB
    let new_task = task::ActiveModel {
        id: Set(task_id.clone()),
        command: Set(input.command.clone()),
        server_ids_json: Set(server_ids_json),
        created_by: Set("admin".to_string()),
        created_at: Set(now),
    };
    new_task.insert(&state.db).await?;

    // Dispatch command to each online agent
    let mut dispatched = 0;
    for sid in &input.server_ids {
        if let Some(tx) = state.agent_manager.get_sender(sid) {
            let msg = ServerMessage::Exec {
                task_id: task_id.clone(),
                command: input.command.clone(),
                timeout: input.timeout,
            };
            if tx.send(msg).await.is_ok() {
                dispatched += 1;
            }
        }
    }

    tracing::info!(
        "Task {} dispatched to {}/{} agents",
        task_id,
        dispatched,
        input.server_ids.len()
    );

    ok(TaskResponse {
        id: task_id,
        command: input.command,
        server_ids: input.server_ids,
        created_at: now,
    })
}

#[utoipa::path(
    get,
    path = "/api/tasks/{id}",
    tag = "tasks",
    params(("id" = String, Path, description = "Task ID")),
    responses(
        (status = 200, description = "Task details", body = TaskResponse),
        (status = 404, description = "Task not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    let t = task::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {id} not found")))?;

    let server_ids: Vec<String> =
        serde_json::from_str(&t.server_ids_json).unwrap_or_default();

    ok(TaskResponse {
        id: t.id,
        command: t.command,
        server_ids,
        created_at: t.created_at,
    })
}

#[utoipa::path(
    get,
    path = "/api/tasks/{id}/results",
    tag = "tasks",
    params(("id" = String, Path, description = "Task ID")),
    responses(
        (status = 200, description = "Task results", body = Vec<task_result::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_task_results(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<task_result::Model>>>, AppError> {
    let results = task_result::Entity::find()
        .filter(task_result::Column::TaskId.eq(&id))
        .order_by_asc(task_result::Column::FinishedAt)
        .all(&state.db)
        .await?;
    ok(results)
}
