use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

use crate::entity::maintenance;
use crate::error::{ok, ApiResponse, AppError};
use crate::service::maintenance::{CreateMaintenance, MaintenanceService, UpdateMaintenance};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/maintenances", get(list_maintenances))
        .route("/maintenances", post(create_maintenance))
        .route("/maintenances/{id}", put(update_maintenance))
        .route("/maintenances/{id}", delete(delete_maintenance))
}

#[utoipa::path(
    get,
    path = "/api/maintenances",
    operation_id = "list_maintenances",
    tag = "maintenances",
    responses(
        (status = 200, description = "List all maintenance windows", body = Vec<maintenance::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn list_maintenances(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<maintenance::Model>>>, AppError> {
    let list = MaintenanceService::list(&state.db).await?;
    ok(list)
}

#[utoipa::path(
    post,
    path = "/api/maintenances",
    operation_id = "create_maintenance",
    tag = "maintenances",
    request_body = CreateMaintenance,
    responses(
        (status = 200, description = "Maintenance window created", body = maintenance::Model),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn create_maintenance(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateMaintenance>,
) -> Result<Json<ApiResponse<maintenance::Model>>, AppError> {
    let m = MaintenanceService::create(&state.db, input).await?;
    ok(m)
}

#[utoipa::path(
    put,
    path = "/api/maintenances/{id}",
    operation_id = "update_maintenance",
    tag = "maintenances",
    params(("id" = String, Path, description = "Maintenance ID")),
    request_body = UpdateMaintenance,
    responses(
        (status = 200, description = "Maintenance window updated", body = maintenance::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn update_maintenance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateMaintenance>,
) -> Result<Json<ApiResponse<maintenance::Model>>, AppError> {
    let m = MaintenanceService::update(&state.db, &id, input).await?;
    ok(m)
}

#[utoipa::path(
    delete,
    path = "/api/maintenances/{id}",
    operation_id = "delete_maintenance",
    tag = "maintenances",
    params(("id" = String, Path, description = "Maintenance ID")),
    responses(
        (status = 200, description = "Maintenance window deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn delete_maintenance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    MaintenanceService::delete(&state.db, &id).await?;
    ok("ok")
}
