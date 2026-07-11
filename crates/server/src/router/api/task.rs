use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};

use crate::entity::{task, task_result};
use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::high_risk_audit::ExecAuditContext;
use crate::service::task_scheduler::{
    self as task_lifecycle, CreateTaskRequest, UpdateTaskRequest,
};
use crate::state::AppState;

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
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(input): Json<CreateTaskRequest>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let audit_detail = format!(
        "type={} name={} command={}",
        input.task_type,
        input.name.as_deref().unwrap_or("?"),
        input.command
    );
    let task_model = task_lifecycle::create_task(&state, input, &current_user.user_id, &ip).await?;
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "task_created",
        Some(&format!("task_id={} {audit_detail}", task_model.id)),
        &ip,
    )
    .await;

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
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<UpdateTaskRequest>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    let updated = task_lifecycle::update_task(&state, &id, input).await?;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "task_updated",
        Some(&format!("task_id={id}")),
        &ip,
    )
    .await;

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
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    task_lifecycle::delete_task(&state, &id).await?;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "task_deleted",
        Some(&format!("task_id={id}")),
        &ip,
    )
    .await;

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
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let updated = task_lifecycle::run_now(
        &state,
        &id,
        Some(ExecAuditContext {
            user_id: current_user.user_id.clone(),
            ip,
        }),
    )
    .await;
    ok(updated?.into())
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

#[cfg(test)]
mod audit_tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::audit_log;
    use crate::test_utils::setup_test_db;

    fn admin() -> CurrentUser {
        CurrentUser {
            user_id: "admin-1".to_string(),
            username: "admin".to_string(),
            role: "admin".to_string(),
            must_change_password: false,
        }
    }

    fn conn() -> ConnectInfo<std::net::SocketAddr> {
        ConnectInfo("203.0.113.5:6666".parse().unwrap())
    }

    async fn insert_oneshot_task(db: &DatabaseConnection, id: &str) {
        let now = Utc::now();
        let model = task::ActiveModel {
            id: Set(id.to_string()),
            command: Set("echo hi".to_string()),
            server_ids_json: Set("[]".to_string()),
            created_by: Set("admin-1".to_string()),
            task_type: Set("oneshot".to_string()),
            name: Set(Some("Nightly".to_string())),
            cron_expression: Set(None),
            enabled: Set(true),
            timeout: Set(None),
            retry_count: Set(0),
            retry_interval: Set(60),
            last_run_at: NotSet,
            next_run_at: Set(None),
            created_at: Set(now),
        };
        model.insert(db).await.unwrap();
    }

    #[tokio::test]
    async fn delete_task_writes_audit_log() {
        let (db, _tmp) = setup_test_db().await;
        insert_oneshot_task(&db, "task-del").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let res = delete_task(
            State(state.clone()),
            conn(),
            Extension(admin()),
            HeaderMap::new(),
            Path("task-del".to_string()),
        )
        .await;
        assert!(res.is_ok(), "delete should succeed: {res:?}");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "task_deleted"
                && l.user_id == "admin-1"
                && l.detail.as_deref().is_some_and(|d| d.contains("task-del"))),
            "expected a task_deleted audit row, got: {logs:?}"
        );
    }

    #[tokio::test]
    async fn update_task_writes_audit_log() {
        let (db, _tmp) = setup_test_db().await;
        insert_oneshot_task(&db, "task-upd").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let res = update_task(
            State(state.clone()),
            conn(),
            Extension(admin()),
            HeaderMap::new(),
            Path("task-upd".to_string()),
            Json(UpdateTaskRequest {
                name: Some("Renamed".to_string()),
                command: None,
                server_ids: None,
                cron_expression: None,
                enabled: None,
                timeout: None,
                retry_count: None,
                retry_interval: None,
            }),
        )
        .await;
        assert!(res.is_ok(), "update should succeed: {res:?}");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "task_updated"
                && l.detail.as_deref().is_some_and(|d| d.contains("task-upd"))),
            "expected a task_updated audit row, got: {logs:?}"
        );
    }
}
