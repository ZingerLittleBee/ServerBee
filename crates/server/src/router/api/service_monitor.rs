use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entity::{service_monitor, service_monitor_record};
use crate::error::{ok, ApiResponse, AppError};
use crate::service::checker;
use crate::service::service_monitor::{
    CreateServiceMonitor, ServiceMonitorService, UpdateServiceMonitor,
};
use crate::state::AppState;

/// GET endpoints accessible to all authenticated users (admin + member).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/service-monitors", get(list_monitors))
        .route("/service-monitors/{id}", get(get_monitor))
        .route("/service-monitors/{id}/records", get(get_records))
}

/// Write endpoints (POST/PUT/DELETE) restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/service-monitors", post(create_monitor))
        .route("/service-monitors/{id}", put(update_monitor))
        .route("/service-monitors/{id}", delete(delete_monitor))
        .route("/service-monitors/{id}/check", post(trigger_check))
}

// ---------------------------------------------------------------------------
// Query parameter structs
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ListQuery {
    /// Filter by monitor type (ssl, dns, http_keyword, tcp, whois).
    #[serde(rename = "type")]
    pub monitor_type: Option<String>,
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct RecordsQuery {
    /// Start of time range (inclusive).
    pub from: Option<DateTime<Utc>>,
    /// End of time range (inclusive).
    pub to: Option<DateTime<Utc>>,
    /// Maximum number of records to return.
    pub limit: Option<u64>,
}

// ---------------------------------------------------------------------------
// Response structs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MonitorWithRecord {
    #[serde(flatten)]
    pub monitor: service_monitor::Model,
    pub latest_record: Option<service_monitor_record::Model>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/service-monitors",
    tag = "service-monitors",
    params(ListQuery),
    responses(
        (status = 200, description = "List all service monitors", body = Vec<service_monitor::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_monitors(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiResponse<Vec<service_monitor::Model>>>, AppError> {
    let monitors =
        ServiceMonitorService::list(&state.db, q.monitor_type.as_deref()).await?;
    ok(monitors)
}

#[utoipa::path(
    get,
    path = "/api/service-monitors/{id}",
    operation_id = "get_service_monitor",
    tag = "service-monitors",
    params(("id" = String, Path, description = "Service monitor ID")),
    responses(
        (status = 200, description = "Service monitor with latest record", body = MonitorWithRecord),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_monitor(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<MonitorWithRecord>>, AppError> {
    let monitor = ServiceMonitorService::get(&state.db, &id).await?;
    let latest_record = ServiceMonitorService::get_latest_record(&state.db, &id).await?;
    ok(MonitorWithRecord {
        monitor,
        latest_record,
    })
}

#[utoipa::path(
    post,
    path = "/api/service-monitors",
    operation_id = "create_service_monitor",
    tag = "service-monitors",
    request_body = CreateServiceMonitor,
    responses(
        (status = 200, description = "Service monitor created", body = service_monitor::Model),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_monitor(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateServiceMonitor>,
) -> Result<Json<ApiResponse<service_monitor::Model>>, AppError> {
    let monitor = ServiceMonitorService::create(&state.db, input).await?;
    ok(monitor)
}

#[utoipa::path(
    put,
    path = "/api/service-monitors/{id}",
    operation_id = "update_service_monitor",
    tag = "service-monitors",
    params(("id" = String, Path, description = "Service monitor ID")),
    request_body = UpdateServiceMonitor,
    responses(
        (status = 200, description = "Service monitor updated", body = service_monitor::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn update_monitor(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateServiceMonitor>,
) -> Result<Json<ApiResponse<service_monitor::Model>>, AppError> {
    let monitor = ServiceMonitorService::update(&state.db, &id, input).await?;
    ok(monitor)
}

#[utoipa::path(
    delete,
    path = "/api/service-monitors/{id}",
    operation_id = "delete_service_monitor",
    tag = "service-monitors",
    params(("id" = String, Path, description = "Service monitor ID")),
    responses(
        (status = 200, description = "Service monitor deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_monitor(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    ServiceMonitorService::delete(&state.db, &id).await?;
    ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/service-monitors/{id}/records",
    operation_id = "get_service_monitor_records",
    tag = "service-monitors",
    params(
        ("id" = String, Path, description = "Service monitor ID"),
        RecordsQuery,
    ),
    responses(
        (status = 200, description = "Service monitor records", body = Vec<service_monitor_record::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_records(
    State(state): State<Arc<AppState>>,
    Path(monitor_id): Path<String>,
    Query(q): Query<RecordsQuery>,
) -> Result<Json<ApiResponse<Vec<service_monitor_record::Model>>>, AppError> {
    let records =
        ServiceMonitorService::get_records(&state.db, &monitor_id, q.from, q.to, q.limit).await?;
    ok(records)
}

#[utoipa::path(
    post,
    path = "/api/service-monitors/{id}/check",
    operation_id = "trigger_service_monitor_check",
    tag = "service-monitors",
    params(("id" = String, Path, description = "Service monitor ID")),
    responses(
        (status = 200, description = "Check result", body = service_monitor_record::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn trigger_check(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<service_monitor_record::Model>>, AppError> {
    let monitor = ServiceMonitorService::get(&state.db, &id).await?;

    let config: serde_json::Value =
        serde_json::from_str(&monitor.config_json).unwrap_or_default();

    let result = checker::run_check(&monitor.monitor_type, &monitor.target, &config).await;

    // Insert the record
    let record = ServiceMonitorService::insert_record(
        &state.db,
        &monitor.id,
        result.success,
        result.latency,
        result.detail,
        result.error,
    )
    .await?;

    // Update monitor state
    let consecutive_failures = if result.success {
        0
    } else {
        monitor.consecutive_failures + 1
    };

    ServiceMonitorService::update_check_state(
        &state.db,
        &monitor.id,
        record.success,
        consecutive_failures,
    )
    .await?;

    ok(record)
}
