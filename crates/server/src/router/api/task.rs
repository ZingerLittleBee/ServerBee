use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{server, task, task_result};
use crate::error::{ApiResponse, AppError, ok};
use crate::state::AppState;
use serverbee_common::constants::{CAP_EXEC, has_capability};
use serverbee_common::protocol::ServerMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tasks", get(list_tasks).post(create_task))
        .route(
            "/tasks/{id}",
            get(get_task).put(update_task).delete(delete_task),
        )
        .route("/tasks/{id}/results", get(get_task_results))
        .route("/tasks/{id}/run", post(run_task))
}

// --- Request / Response types ---

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateTaskRequest {
    pub command: String,
    pub server_ids: Vec<String>,
    #[serde(default)]
    pub timeout: Option<u32>,
    /// "oneshot" (default) or "scheduled"
    #[serde(default = "default_oneshot")]
    pub task_type: String,
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    #[serde(default)]
    pub retry_count: Option<i32>,
    #[serde(default)]
    pub retry_interval: Option<i32>,
}

fn default_oneshot() -> String {
    "oneshot".to_string()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub server_ids: Option<Vec<String>>,
    pub cron_expression: Option<String>,
    pub enabled: Option<bool>,
    pub timeout: Option<i32>,
    pub retry_count: Option<i32>,
    pub retry_interval: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListTasksQuery {
    #[serde(rename = "type")]
    pub task_type: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TaskResponse {
    pub id: String,
    pub command: String,
    pub server_ids: Vec<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub task_type: String,
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    pub enabled: bool,
    pub timeout: Option<i32>,
    pub retry_count: i32,
    pub retry_interval: i32,
    pub last_run_at: Option<chrono::DateTime<Utc>>,
    pub next_run_at: Option<chrono::DateTime<Utc>>,
}

impl From<task::Model> for TaskResponse {
    fn from(t: task::Model) -> Self {
        let server_ids: Vec<String> = serde_json::from_str(&t.server_ids_json).unwrap_or_default();
        Self {
            id: t.id,
            command: t.command,
            server_ids,
            created_at: t.created_at,
            task_type: t.task_type,
            name: t.name,
            cron_expression: t.cron_expression,
            enabled: t.enabled,
            timeout: t.timeout,
            retry_count: t.retry_count,
            retry_interval: t.retry_interval,
            last_run_at: t.last_run_at,
            next_run_at: t.next_run_at,
        }
    }
}

// --- Handlers ---

#[utoipa::path(
    get,
    path = "/api/tasks",
    tag = "tasks",
    params(ListTasksQuery),
    responses(
        (status = 200, description = "List tasks", body = Vec<TaskResponse>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<ApiResponse<Vec<TaskResponse>>>, AppError> {
    let mut q = task::Entity::find();
    if let Some(t) = &query.task_type {
        q = q.filter(task::Column::TaskType.eq(t));
    }
    let tasks = q
        .order_by_desc(task::Column::CreatedAt)
        .all(&state.db)
        .await?;
    let results: Vec<TaskResponse> = tasks.into_iter().map(|t| t.into()).collect();
    ok(results)
}

#[utoipa::path(
    post,
    path = "/api/tasks",
    tag = "tasks",
    request_body = CreateTaskRequest,
    responses(
        (status = 200, description = "Task created", body = TaskResponse),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateTaskRequest>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    if input.server_ids.is_empty() {
        return Err(AppError::Validation(
            "server_ids cannot be empty".to_string(),
        ));
    }
    if input.command.trim().is_empty() {
        return Err(AppError::Validation("command cannot be empty".to_string()));
    }

    let is_scheduled = input.task_type == "scheduled";

    if is_scheduled {
        let cron = input.cron_expression.as_deref().ok_or_else(|| {
            AppError::Validation("cron_expression is required for scheduled tasks".into())
        })?;
        // Validate cron expression
        cron::Schedule::from_str(cron)
            .map_err(|e| AppError::Validation(format!("Invalid cron expression: {e}")))?;
    }

    // Validate numeric bounds
    if matches!(input.timeout, Some(0)) {
        return Err(AppError::Validation("timeout must be > 0".into()));
    }
    if let Some(rc) = input.retry_count
        && !(0..=10).contains(&rc)
    {
        return Err(AppError::Validation(
            "retry_count must be between 0 and 10".into(),
        ));
    }
    if matches!(input.retry_interval, Some(ri) if ri < 1) {
        return Err(AppError::Validation("retry_interval must be >= 1".into()));
    }

    let task_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let server_ids_json = serde_json::to_string(&input.server_ids)
        .map_err(|e| AppError::Internal(format!("Serialization error: {e}")))?;

    // Compute next_run_at using the configured scheduler timezone
    let tz: chrono_tz::Tz = state
        .task_scheduler
        .timezone()
        .parse()
        .unwrap_or(chrono_tz::UTC);
    let next_run = input.cron_expression.as_deref().and_then(|c| {
        cron::Schedule::from_str(c)
            .ok()
            .and_then(|s| s.upcoming(tz).next().map(|dt| dt.with_timezone(&Utc)))
    });

    let new_task = task::ActiveModel {
        id: Set(task_id.clone()),
        command: Set(input.command.clone()),
        server_ids_json: Set(server_ids_json),
        created_by: Set("admin".to_string()),
        task_type: Set(input.task_type.clone()),
        name: Set(input.name.clone()),
        cron_expression: Set(input.cron_expression.clone()),
        enabled: Set(true),
        timeout: Set(input.timeout.map(|t| t as i32)),
        retry_count: Set(input.retry_count.unwrap_or(0)),
        retry_interval: Set(input.retry_interval.unwrap_or(60)),
        last_run_at: NotSet,
        next_run_at: Set(next_run),
        created_at: Set(now),
    };
    let task_model = new_task.insert(&state.db).await?;

    if is_scheduled {
        // Register cron job in scheduler; rollback DB row on failure
        if let Err(e) = state
            .task_scheduler
            .add_job(&task_model, state.clone())
            .await
        {
            let _ = task::Entity::delete_by_id(&task_id).exec(&state.db).await;
            return Err(e);
        }
        tracing::info!("Scheduled task {} registered", task_id);
    } else {
        // One-shot: dispatch immediately
        dispatch_oneshot(
            &state,
            &task_id,
            &input.command,
            &input.server_ids,
            input.timeout,
        )
        .await?;
    }

    ok(task_model.into())
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    let t = task::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {id} not found")))?;
    ok(t.into())
}

#[utoipa::path(
    put,
    path = "/api/tasks/{id}",
    tag = "tasks",
    params(("id" = String, Path, description = "Task ID")),
    request_body = UpdateTaskRequest,
    responses(
        (status = 200, description = "Task updated", body = TaskResponse),
        (status = 404, description = "Task not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn update_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateTaskRequest>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    let existing = task::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {id} not found")))?;

    let existing_backup = existing.clone();
    let mut model: task::ActiveModel = existing.into();

    if let Some(name) = input.name {
        model.name = Set(Some(name));
    }
    if let Some(command) = input.command {
        model.command = Set(command);
    }
    if let Some(server_ids) = &input.server_ids {
        let json = serde_json::to_string(server_ids)
            .map_err(|e| AppError::Internal(format!("Serialization error: {e}")))?;
        model.server_ids_json = Set(json);
    }
    if let Some(cron) = &input.cron_expression {
        cron::Schedule::from_str(cron)
            .map_err(|e| AppError::Validation(format!("Invalid cron expression: {e}")))?;
        // Recompute next_run_at using configured timezone
        let tz: chrono_tz::Tz = state
            .task_scheduler
            .timezone()
            .parse()
            .unwrap_or(chrono_tz::UTC);
        let next = cron::Schedule::from_str(cron)
            .ok()
            .and_then(|s| s.upcoming(tz).next().map(|dt| dt.with_timezone(&Utc)));
        model.cron_expression = Set(Some(cron.clone()));
        model.next_run_at = Set(next);
    }
    if let Some(enabled) = input.enabled {
        model.enabled = Set(enabled);
        // When resuming a paused task, recompute next_run_at so the UI shows a fresh time
        if enabled {
            let cron_expr = input
                .cron_expression
                .as_deref()
                .or(existing_backup.cron_expression.as_deref());
            if let Some(cron) = cron_expr {
                let tz: chrono_tz::Tz = state
                    .task_scheduler
                    .timezone()
                    .parse()
                    .unwrap_or(chrono_tz::UTC);
                let next = cron::Schedule::from_str(cron)
                    .ok()
                    .and_then(|s| s.upcoming(tz).next().map(|dt| dt.with_timezone(&Utc)));
                model.next_run_at = Set(next);
            }
        }
    }
    if let Some(timeout) = input.timeout {
        if timeout < 1 {
            return Err(AppError::Validation("timeout must be >= 1".into()));
        }
        model.timeout = Set(Some(timeout));
    }
    if let Some(retry_count) = input.retry_count {
        if !(0..=10).contains(&retry_count) {
            return Err(AppError::Validation(
                "retry_count must be between 0 and 10".into(),
            ));
        }
        model.retry_count = Set(retry_count);
    }
    if let Some(retry_interval) = input.retry_interval {
        if retry_interval < 1 {
            return Err(AppError::Validation("retry_interval must be >= 1".into()));
        }
        model.retry_interval = Set(retry_interval);
    }

    let updated = model.update(&state.db).await?;

    // Sync scheduler; on failure, restore the original row
    if updated.task_type == "scheduled"
        && let Err(e) = state
            .task_scheduler
            .update_job(&updated, state.clone())
            .await
    {
        let rollback: task::ActiveModel = existing_backup.into();
        let _ = rollback.update(&state.db).await;
        return Err(e);
    }

    ok(updated.into())
}

#[utoipa::path(
    delete,
    path = "/api/tasks/{id}",
    tag = "tasks",
    params(("id" = String, Path, description = "Task ID")),
    responses(
        (status = 200, description = "Task deleted"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn delete_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    // Cancel active run and remove from scheduler first (idempotent, safe to call even if not registered)
    state.task_scheduler.remove_job(&id).await?;
    // Delete DB rows — if this fails, the scheduler job is already gone (acceptable:
    // next server restart will simply not re-register the task since it was removed from scheduler)
    task_result::Entity::delete_many()
        .filter(task_result::Column::TaskId.eq(&id))
        .exec(&state.db)
        .await?;
    task::Entity::delete_by_id(&id).exec(&state.db).await?;
    ok(())
}

#[utoipa::path(
    post,
    path = "/api/tasks/{id}/run",
    tag = "tasks",
    params(("id" = String, Path, description = "Task ID")),
    responses(
        (status = 200, description = "Task triggered", body = TaskResponse),
        (status = 409, description = "Task already running"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn run_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    // Validate task exists and is scheduled type
    let task_model = task::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {id} not found")))?;

    if task_model.task_type != "scheduled" {
        return Err(AppError::BadRequest(
            "Only scheduled tasks can be manually triggered".into(),
        ));
    }

    let started = crate::task::task_scheduler::execute_scheduled_task(&state, &id, true).await;
    if !started {
        return Err(AppError::Conflict(
            "Task is currently running, try again later".into(),
        ));
    }
    // Re-fetch to get updated last_run_at
    let updated = task::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Task not found".into()))?;
    ok(updated.into())
}

#[utoipa::path(
    get,
    path = "/api/tasks/{id}/results",
    tag = "tasks",
    params(("id" = String, Path, description = "Task ID")),
    responses(
        (status = 200, description = "Task results", body = Vec<task_result::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_task_results(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<task_result::Model>>>, AppError> {
    let results = task_result::Entity::find()
        .filter(task_result::Column::TaskId.eq(&id))
        .order_by_desc(task_result::Column::FinishedAt)
        .limit(500)
        .all(&state.db)
        .await?;
    ok(results)
}

// --- Helper ---

async fn dispatch_oneshot(
    state: &Arc<AppState>,
    task_id: &str,
    command: &str,
    server_ids: &[String],
    timeout: Option<u32>,
) -> Result<(), AppError> {
    let servers = server::Entity::find()
        .filter(server::Column::Id.is_in(server_ids.iter().cloned()))
        .all(&state.db)
        .await?;

    let (capable, disabled): (Vec<_>, Vec<_>) = server_ids.iter().partition(|sid| {
        servers
            .iter()
            .find(|s| &s.id == *sid)
            .map(|s| has_capability(s.capabilities as u32, CAP_EXEC))
            .unwrap_or(false)
    });

    let now = Utc::now();
    for sid in &disabled {
        let result = task_result::ActiveModel {
            id: NotSet,
            task_id: Set(task_id.to_string()),
            server_id: Set(sid.to_string()),
            output: Set("Capability 'exec' is disabled for this server".to_string()),
            exit_code: Set(-2),
            run_id: Set(None),
            attempt: Set(1),
            started_at: Set(None),
            finished_at: Set(now),
        };
        result.insert(&state.db).await?;
    }

    let mut dispatched = 0;
    for sid in &capable {
        if let Some(tx) = state.agent_manager.get_sender(sid) {
            let msg = ServerMessage::Exec {
                task_id: task_id.to_string(),
                command: command.to_string(),
                timeout,
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
        server_ids.len()
    );
    Ok(())
}
