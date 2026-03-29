use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

use crate::entity::dashboard;
use crate::error::{ApiResponse, AppError, ok};
use crate::service::dashboard::{
    CreateDashboardInput, DashboardService, DashboardWithWidgets, UpdateDashboardInput,
};
use crate::state::AppState;

/// GET endpoints accessible to all authenticated users (admin + member).
///
/// **Important:** `/dashboards/default` is registered before `/dashboards/{id}`
/// so that the literal path is matched before the path parameter captures "default" as an id.
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/dashboards", get(list_dashboards))
        .route("/dashboards/default", get(get_default_dashboard))
        .route("/dashboards/{id}", get(get_dashboard))
}

/// Write endpoints (POST/PUT/DELETE) restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/dashboards", post(create_dashboard))
        .route("/dashboards/{id}", put(update_dashboard))
        .route("/dashboards/{id}", delete(delete_dashboard))
}

#[utoipa::path(
    get,
    path = "/api/dashboards",
    tag = "dashboards",
    responses(
        (status = 200, description = "List all dashboards", body = Vec<dashboard::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_dashboards(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<dashboard::Model>>>, AppError> {
    let dashboards = DashboardService::list(&state.db).await?;
    ok(dashboards)
}

#[utoipa::path(
    get,
    path = "/api/dashboards/default",
    tag = "dashboards",
    responses(
        (status = 200, description = "Default dashboard with widgets (auto-creates if none exists)", body = DashboardWithWidgets),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_default_dashboard(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<DashboardWithWidgets>>, AppError> {
    let dashboard = DashboardService::get_default(&state.db).await?;
    ok(dashboard)
}

#[utoipa::path(
    get,
    path = "/api/dashboards/{id}",
    tag = "dashboards",
    params(("id" = String, Path, description = "Dashboard ID")),
    responses(
        (status = 200, description = "Dashboard with widgets", body = DashboardWithWidgets),
        (status = 404, description = "Dashboard not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_dashboard(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<DashboardWithWidgets>>, AppError> {
    let dashboard = DashboardService::get_with_widgets(&state.db, &id).await?;
    ok(dashboard)
}

#[utoipa::path(
    post,
    path = "/api/dashboards",
    tag = "dashboards",
    request_body = CreateDashboardInput,
    responses(
        (status = 200, description = "Dashboard created", body = dashboard::Model),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn create_dashboard(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateDashboardInput>,
) -> Result<Json<ApiResponse<dashboard::Model>>, AppError> {
    let dashboard = DashboardService::create(&state.db, input).await?;
    ok(dashboard)
}

#[utoipa::path(
    put,
    path = "/api/dashboards/{id}",
    tag = "dashboards",
    params(("id" = String, Path, description = "Dashboard ID")),
    request_body = UpdateDashboardInput,
    responses(
        (status = 200, description = "Dashboard updated with widgets", body = DashboardWithWidgets),
        (status = 404, description = "Dashboard not found"),
        (status = 400, description = "Validation error (e.g. cannot unset default, unknown widget type)"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn update_dashboard(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateDashboardInput>,
) -> Result<Json<ApiResponse<DashboardWithWidgets>>, AppError> {
    let dashboard = DashboardService::update(&state.db, &id, input).await?;
    ok(dashboard)
}

#[utoipa::path(
    delete,
    path = "/api/dashboards/{id}",
    tag = "dashboards",
    params(("id" = String, Path, description = "Dashboard ID")),
    responses(
        (status = 200, description = "Dashboard deleted"),
        (status = 400, description = "Cannot delete default or last dashboard"),
        (status = 404, description = "Dashboard not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn delete_dashboard(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    DashboardService::delete(&state.db, &id).await?;
    ok("ok")
}
