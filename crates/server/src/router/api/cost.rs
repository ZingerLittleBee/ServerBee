use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

use crate::error::{ApiResponse, AppError, ok};
use crate::service::cost::{CostOverviewResponse, CostService, ServerCostInsights};
use crate::state::AppState;

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/cost/overview", get(get_cost_overview))
        .route("/servers/{id}/cost-insights", get(get_server_cost_insights))
}

#[utoipa::path(
    get,
    path = "/api/cost/overview",
    responses(
        (status = 200, description = "Cost overview", body = ApiResponse<CostOverviewResponse>),
    ),
    tag = "cost",
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_cost_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<CostOverviewResponse>>, AppError> {
    ok(CostService::overview(&state.db, &state.agent_manager).await?)
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/cost-insights",
    params(
        ("id" = String, Path, description = "Server ID"),
    ),
    responses(
        (status = 200, description = "Server cost insights", body = ApiResponse<ServerCostInsights>),
        (status = 404, description = "Server not found"),
    ),
    tag = "cost",
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_server_cost_insights(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ServerCostInsights>>, AppError> {
    ok(CostService::server_insights(&state.db, &state.agent_manager, &id).await?)
}
