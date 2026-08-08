use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{ApiResponse, AppError, ok};
use crate::service::server::ServerService;
use crate::service::traffic::{
    CycleTraffic, DailyTraffic, HourlyTraffic, ServerTrafficOverview, TrafficPrediction,
    TrafficService, compute_prediction, get_cycle_range,
};
use crate::state::AppState;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TrafficResponse {
    pub cycle_start: String,
    pub cycle_end: String,
    pub bytes_in: i64,
    pub bytes_out: i64,
    pub bytes_total: i64,
    pub traffic_limit: Option<i64>,
    pub traffic_limit_type: Option<String>,
    pub usage_percent: Option<f64>,
    pub prediction: Option<TrafficPrediction>,
    pub daily: Vec<DailyTraffic>,
    pub hourly: Vec<HourlyTraffic>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CycleResponse {
    pub current: CycleTraffic,
    pub history: Vec<CycleTraffic>,
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct OverviewDailyQuery {
    /// Number of days to include (default: 30).
    pub days: Option<u32>,
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct CycleQuery {
    /// Number of historical billing cycles to return (default: 6).
    pub history: Option<u32>,
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ServerDailyQuery {
    /// Inclusive start date as `YYYY-MM-DD` (default: start of a 30-day
    /// inclusive window ending at `to`, i.e. `to` minus 29 days).
    pub from: Option<String>,
    /// Inclusive end date as `YYYY-MM-DD` (default: today).
    pub to: Option<String>,
}

/// Number of calendar dates in the default inclusive window (including `to`).
const DEFAULT_DAILY_WINDOW_DAYS: i64 = 30;

fn parse_query_date(value: &str, field: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest(format!("Invalid {field} date, expected YYYY-MM-DD")))
}

/// Inclusive start for a default window of [`DEFAULT_DAILY_WINDOW_DAYS`] calendar
/// dates ending on `end` (`end` is day N of N).
fn default_daily_from(end: NaiveDate) -> NaiveDate {
    end - Duration::days(DEFAULT_DAILY_WINDOW_DAYS - 1)
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers/{id}/traffic", get(get_traffic))
        .route("/traffic/overview", get(get_traffic_overview))
        .route("/traffic/overview/daily", get(get_traffic_overview_daily))
        .route("/traffic/{server_id}/daily", get(get_traffic_server_daily))
        .route("/traffic/{server_id}/cycle", get(get_traffic_cycle))
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/traffic",
    params(
        ("id" = String, Path, description = "Server ID"),
    ),
    responses(
        (status = 200, description = "Traffic statistics", body = ApiResponse<TrafficResponse>),
        (status = 404, description = "Server not found"),
    ),
    tag = "servers"
)]
async fn get_traffic(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TrafficResponse>>, AppError> {
    let server = ServerService::get_server(&state.db, &id).await?;

    let billing_cycle = server.billing_cycle.as_deref().unwrap_or("monthly");
    let today = Utc::now().date_naive();
    let (cycle_start, cycle_end) = get_cycle_range(billing_cycle, server.billing_start_day, today);

    // Get total traffic for the cycle
    let (bytes_in, bytes_out) =
        TrafficService::query_cycle_traffic(&state.db, &id, cycle_start, cycle_end).await?;
    let bytes_total = bytes_in + bytes_out;

    // Calculate usage percent
    let usage_percent = server.traffic_limit.map(|limit| {
        if limit > 0 {
            let relevant = match server.traffic_limit_type.as_deref() {
                Some("up") => bytes_out,
                Some("down") => bytes_in,
                _ => bytes_total,
            };
            relevant as f64 / limit as f64 * 100.0
        } else {
            0.0
        }
    });

    // Calculate prediction
    let days_elapsed = (today - cycle_start).num_days();
    let days_remaining = (cycle_end - today).num_days();
    let recent_sum = match server.traffic_limit_type.as_deref() {
        Some("up") => bytes_out,
        Some("down") => bytes_in,
        _ => bytes_total,
    };
    let prediction = compute_prediction(
        recent_sum,
        days_elapsed,
        days_remaining,
        server.traffic_limit,
        server.traffic_limit_type.as_deref().unwrap_or("sum"),
    );

    // Get daily breakdown
    let daily =
        TrafficService::query_daily_breakdown(&state.db, &id, cycle_start, cycle_end).await?;

    // Get hourly breakdown for today
    let hourly = TrafficService::query_hourly_breakdown(&state.db, &id, today).await?;

    let response = TrafficResponse {
        cycle_start: cycle_start.format("%Y-%m-%d").to_string(),
        cycle_end: cycle_end.format("%Y-%m-%d").to_string(),
        bytes_in,
        bytes_out,
        bytes_total,
        traffic_limit: server.traffic_limit,
        traffic_limit_type: server.traffic_limit_type,
        usage_percent,
        prediction,
        daily,
        hourly,
    };

    ok(response)
}

#[utoipa::path(
    get,
    path = "/api/traffic/overview",
    responses(
        (status = 200, description = "Traffic overview for all servers with billing cycles",
         body = ApiResponse<Vec<ServerTrafficOverview>>),
    ),
    tag = "traffic",
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_traffic_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<ServerTrafficOverview>>>, AppError> {
    let overview = TrafficService::overview(&state.db).await?;
    ok(overview)
}

#[utoipa::path(
    get,
    path = "/api/traffic/overview/daily",
    params(OverviewDailyQuery),
    responses(
        (status = 200, description = "Global daily traffic aggregation across all servers",
         body = ApiResponse<Vec<DailyTraffic>>),
    ),
    tag = "traffic",
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_traffic_overview_daily(
    State(state): State<Arc<AppState>>,
    Query(q): Query<OverviewDailyQuery>,
) -> Result<Json<ApiResponse<Vec<DailyTraffic>>>, AppError> {
    let days = q.days.unwrap_or(30);
    let daily = TrafficService::overview_daily(&state.db, days).await?;
    ok(daily)
}

#[utoipa::path(
    get,
    path = "/api/traffic/{server_id}/daily",
    params(
        ("server_id" = String, Path, description = "Server ID"),
        ServerDailyQuery,
    ),
    responses(
        (status = 200, description = "Daily traffic breakdown for one server",
         body = ApiResponse<Vec<DailyTraffic>>),
        (status = 400, description = "Invalid date range"),
        (status = 404, description = "Server not found"),
    ),
    tag = "traffic",
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_traffic_server_daily(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(q): Query<ServerDailyQuery>,
) -> Result<Json<ApiResponse<Vec<DailyTraffic>>>, AppError> {
    ServerService::get_server(&state.db, &server_id).await?;

    let end = match q.to.as_deref() {
        Some(value) => parse_query_date(value, "to")?,
        None => Utc::now().date_naive(),
    };
    let start = match q.from.as_deref() {
        Some(value) => parse_query_date(value, "from")?,
        None => default_daily_from(end),
    };

    if start > end {
        return Err(AppError::BadRequest(
            "`from` must not be after `to`".to_string(),
        ));
    }

    let daily = TrafficService::query_daily_breakdown(&state.db, &server_id, start, end).await?;
    ok(daily)
}

#[utoipa::path(
    get,
    path = "/api/traffic/{server_id}/cycle",
    params(
        ("server_id" = String, Path, description = "Server ID"),
        CycleQuery,
    ),
    responses(
        (status = 200, description = "Current cycle and historical cycle traffic",
         body = ApiResponse<CycleResponse>),
        (status = 404, description = "Server not found"),
    ),
    tag = "traffic",
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_traffic_cycle(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(q): Query<CycleQuery>,
) -> Result<Json<ApiResponse<CycleResponse>>, AppError> {
    let server = ServerService::get_server(&state.db, &server_id).await?;
    let billing_cycle = server.billing_cycle.as_deref().unwrap_or("monthly");
    let today = Utc::now().date_naive();

    // Current cycle
    let (cycle_start, cycle_end) = get_cycle_range(billing_cycle, server.billing_start_day, today);
    let (bytes_in, bytes_out) =
        TrafficService::query_cycle_traffic(&state.db, &server_id, cycle_start, cycle_end).await?;

    let current = CycleTraffic {
        period: format!(
            "{} ~ {}",
            cycle_start.format("%Y-%m-%d"),
            cycle_end.format("%Y-%m-%d")
        ),
        start: cycle_start.format("%Y-%m-%d").to_string(),
        end: cycle_end.format("%Y-%m-%d").to_string(),
        bytes_in,
        bytes_out,
    };

    // Historical cycles (excluding current)
    let history_count = q.history.unwrap_or(6);
    let mut history = TrafficService::cycle_history(
        &state.db,
        &server_id,
        billing_cycle,
        server.billing_start_day,
        history_count,
    )
    .await?;

    // cycle_history starts from the current cycle; remove it since we have it separately
    if !history.is_empty() {
        history.remove(0);
    }

    ok(CycleResponse { current, history })
}
