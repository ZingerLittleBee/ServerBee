use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::{ApiResponse, AppError, ok};
use crate::service::server::ServerService;
use crate::service::uptime::{UptimeDailyEntry, UptimeService};
use crate::state::AppState;

#[derive(Deserialize, utoipa::IntoParams)]
pub struct UptimeDailyQuery {
    /// Number of days to include (default: 90, min: 1, max: 365).
    pub days: Option<u32>,
}

/// Read routes accessible to all authenticated users.
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/servers/{server_id}/uptime-daily", get(get_uptime_daily))
}

#[utoipa::path(
    get,
    path = "/api/servers/{server_id}/uptime-daily",
    operation_id = "get_uptime_daily",
    tag = "servers",
    params(
        ("server_id" = String, Path, description = "Server ID"),
        UptimeDailyQuery,
    ),
    responses(
        (status = 200, description = "Daily uptime entries", body = ApiResponse<Vec<UptimeDailyEntry>>),
        (status = 400, description = "Invalid days parameter"),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn get_uptime_daily(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(query): Query<UptimeDailyQuery>,
) -> Result<Json<ApiResponse<Vec<UptimeDailyEntry>>>, AppError> {
    let days = query.days.unwrap_or(90);

    if !(1..=365).contains(&days) {
        return Err(AppError::BadRequest(
            "days must be between 1 and 365".to_string(),
        ));
    }

    // Verify server exists (returns 404 if not)
    ServerService::get_server(&state.db, &server_id).await?;

    let entries = UptimeService::get_daily_filled(&state.db, &server_id, days).await?;
    ok(entries)
}
