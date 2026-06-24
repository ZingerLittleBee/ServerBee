//! Public status surface (`/api/status/*`).
//!
//! All routes are unauthenticated. Each handler resolves the public scope from
//! the singleton `status_page` config row, then applies a feature-toggle gate
//! (`enabled` + the relevant `show_*` flag). Out-of-scope `{id}` lookups return
//! `404` — never `403` — to avoid confirming the existence of hidden or
//! unselected servers (see the spec §"Public server scope").
//!
//! DTOs and per-endpoint queries live in
//! `crate::service::public_status`. Each handler is a thin wrapper that gates
//! and then delegates to that module.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Router, middleware};

use crate::entity::status_page;
use crate::error::{ApiResponse, AppError, ok};
use crate::router::utils::extract_client_ip;
use crate::service::public_status::{self as svc, PublicScope};
use crate::service::uptime::UptimeDailyEntry;
use crate::state::{AppState, RateLimitEntry};

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Public status surface mounted under `/api`. No auth; every route is gated
/// by the singleton config plus per-IP rate limiting.
pub fn public_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/status/config", get(get_config))
        .route("/status", get(list_servers))
        .route("/status/servers/{id}", get(get_server_detail))
        .route("/status/servers/{id}/metrics", get(get_server_metrics))
        .route(
            "/status/servers/{id}/uptime-daily",
            get(get_server_uptime_daily),
        )
        .route("/status/network", get(network_overview))
        .route("/status/network/{id}", get(network_server_detail))
        .route("/status/ip-quality", get(ip_quality_overview))
        .route("/status/incidents", get(list_incidents))
        .route("/status/maintenances", get(list_maintenances))
        .layer(middleware::from_fn_with_state(
            state,
            public_status_rate_limit,
        ))
}

// ---------------------------------------------------------------------------
// Rate limit middleware
//
// 60 requests / 60 seconds per source IP. Uses a dedicated `DashMap` on
// `AppState` (`public_rate_limit`) so it does not interfere with the
// login/register limiters which use a 15-minute window with different
// budgets.
// ---------------------------------------------------------------------------

// TODO(public-rate-limit-config): hoist these constants into
// `config.rate_limit.public_max` / `public_window_seconds` so operators can
// tune the public-status budget per deployment. Mirrors the constants in
// `router/api/rate_limit.rs::{PUBLIC_MAX,PUBLIC_WINDOW_SECONDS}` — keep the
// two in sync until the config migration lands.
const PUBLIC_STATUS_WINDOW_SECONDS: i64 = 60;
const PUBLIC_STATUS_MAX_REQUESTS: u32 = 60;

/// In-band sweep counter. Every 100th invocation of `check_public_rate`
/// triggers eviction of long-stale entries. Mirrors `RATE_CHECK_COUNTER` in
/// `state.rs` so the two limiters use the same idiom.
static SWEEP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Per-IP token-bucket-ish rate limiter for the public status surface.
///
/// Extracts the client IP using the project's standard `extract_client_ip`
/// helper (which honours `trusted_proxies` config and refuses spoofed XFF
/// headers from untrusted sources). Denied requests return `429` via the
/// canonical `AppError::TooManyRequests` path; the underlying entry counter
/// is not incremented on denial so a flood does not extend the window
/// arbitrarily.
async fn public_status_rate_limit(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Response {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    if !check_public_rate(&state.public_rate_limit, &ip) {
        return AppError::TooManyRequests(format!(
            "Public status surface rate limit exceeded ({PUBLIC_STATUS_MAX_REQUESTS} requests per {PUBLIC_STATUS_WINDOW_SECONDS}s)."
        ))
        .into_response();
    }

    next.run(req).await
}

/// Token check against the 60req/60s public status bucket. Returns `true` when
/// the request is allowed. Mirrors the structure of `AppState::check_rate`
/// (login/register) but uses a 60-second window keyed in seconds rather than
/// minutes so callers don't need to know about that detail.
///
/// Every 100th call sweeps `map` for entries whose `window_start` is older
/// than `PUBLIC_STATUS_WINDOW_SECONDS * 2` seconds (2× margin avoids evicting
/// a window that is still actively counting). `session_cleaner.rs` runs an
/// hourly safety-net sweep on top of this in-band pruning.
fn check_public_rate(map: &dashmap::DashMap<String, RateLimitEntry>, ip: &str) -> bool {
    let count = SWEEP_COUNTER.fetch_add(1, Ordering::Relaxed);
    if count.is_multiple_of(100) {
        let cutoff =
            chrono::Utc::now() - chrono::Duration::seconds(PUBLIC_STATUS_WINDOW_SECONDS * 2);
        map.retain(|_, entry| entry.window_start > cutoff);
    }

    let now = chrono::Utc::now();
    let window = chrono::Duration::seconds(PUBLIC_STATUS_WINDOW_SECONDS);

    let mut entry = map.entry(ip.to_string()).or_insert_with(|| RateLimitEntry {
        count: 0,
        window_start: now,
    });

    if now - entry.window_start > window {
        entry.count = 1;
        entry.window_start = now;
        return true;
    }

    if entry.count >= PUBLIC_STATUS_MAX_REQUESTS {
        return false;
    }

    entry.count += 1;
    true
}

// ---------------------------------------------------------------------------
// Gate helpers
// ---------------------------------------------------------------------------

/// Resolve the public scope and verify the top-level `enabled` toggle.
async fn resolve_enabled_scope(state: &AppState) -> Result<PublicScope, AppError> {
    let scope = svc::resolve_scope(&state.db).await?;
    if !scope.config.enabled {
        return Err(AppError::Forbidden("public_status_disabled".into()));
    }
    Ok(scope)
}

/// Verify the relevant `show_*` toggle on top of `enabled` (already checked).
fn gate_subpage(
    scope: &PublicScope,
    toggle: fn(&status_page::Model) -> bool,
) -> Result<(), AppError> {
    if !toggle(&scope.config) {
        return Err(AppError::Forbidden("public_status_panel_disabled".into()));
    }
    Ok(())
}

/// Ensure the requested `id` is part of the public scope; otherwise 404.
fn gate_scope_id(scope: &PublicScope, id: &str) -> Result<(), AppError> {
    if !scope.contains(id) {
        return Err(AppError::NotFound("server".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/status/config",
    tag = "public-status",
    responses(
        (status = 200, description = "Public status page configuration", body = svc::PublicStatusConfig),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<svc::PublicStatusConfig>>, AppError> {
    // The config endpoint is intentionally NOT gated by `enabled` — the SPA
    // needs to read `enabled = false` to render a "site disabled" notice.
    let config = svc::load_config(&state.db).await?;
    ok(svc::to_public_config(&config))
}

#[utoipa::path(
    get,
    path = "/api/status",
    operation_id = "public_list_servers",
    tag = "public-status",
    responses(
        (status = 200, description = "Scoped server summaries", body = Vec<svc::PublicServerSummary>),
        (status = 403, description = "Public status page disabled"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<svc::PublicServerSummary>>>, AppError> {
    resolve_enabled_scope(&state).await?;
    let servers = svc::list_servers(&state.db, &state.agent_manager).await?;
    ok(servers)
}

#[utoipa::path(
    get,
    path = "/api/status/servers/{id}",
    tag = "public-status",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Scoped server detail", body = svc::PublicServerDetail),
        (status = 403, description = "Public status page or server-detail panel disabled"),
        (status = 404, description = "Server not in public scope"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn get_server_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<svc::PublicServerDetail>>, AppError> {
    let scope = resolve_enabled_scope(&state).await?;
    gate_subpage(&scope, |c| c.show_server_detail)?;
    gate_scope_id(&scope, &id)?;
    let detail = svc::get_server_detail(&state.db, &state.agent_manager, &id).await?;
    ok(detail)
}

#[utoipa::path(
    get,
    path = "/api/status/servers/{id}/metrics",
    tag = "public-status",
    params(
        ("id" = String, Path, description = "Server ID"),
        ("from" = String, Query, description = "ISO-8601 range start"),
        ("to" = String, Query, description = "ISO-8601 range end"),
        ("interval" = Option<String>, Query, description = "auto | raw | hourly"),
    ),
    responses(
        (status = 200, description = "Time-series metrics", body = Vec<svc::PublicMetricsPoint>),
        (status = 403, description = "Public status page or server-detail panel disabled"),
        (status = 404, description = "Server not in public scope"),
        (status = 422, description = "Invalid time range (`from` after `to`)"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn get_server_metrics(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(range): Query<svc::PublicMetricsRangeQuery>,
) -> Result<Json<ApiResponse<Vec<svc::PublicMetricsPoint>>>, AppError> {
    let scope = resolve_enabled_scope(&state).await?;
    gate_subpage(&scope, |c| c.show_server_detail)?;
    gate_scope_id(&scope, &id)?;
    let points = svc::get_server_metrics(&state.db, &id, range).await?;
    ok(points)
}

#[utoipa::path(
    get,
    path = "/api/status/servers/{id}/uptime-daily",
    tag = "public-status",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "90-day uptime band", body = Vec<UptimeDailyEntry>),
        (status = 403, description = "Public status page disabled"),
        (status = 404, description = "Server not in public scope"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn get_server_uptime_daily(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<UptimeDailyEntry>>>, AppError> {
    let scope = resolve_enabled_scope(&state).await?;
    gate_scope_id(&scope, &id)?;
    let daily = svc::get_server_uptime_daily(&state.db, &id).await?;
    ok(daily)
}

#[utoipa::path(
    get,
    path = "/api/status/network",
    tag = "public-status",
    responses(
        (status = 200, description = "Network probe overview", body = svc::PublicNetworkOverview),
        (status = 403, description = "Public status page or network panel disabled"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn network_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<svc::PublicNetworkOverview>>, AppError> {
    let scope = resolve_enabled_scope(&state).await?;
    gate_subpage(&scope, |c| c.show_network)?;
    let overview =
        svc::network_overview(&state.db, &state.agent_manager, &state.config.network_probe).await?;
    ok(overview)
}

#[utoipa::path(
    get,
    path = "/api/status/network/{id}",
    tag = "public-status",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Per-server network detail", body = svc::PublicNetworkServerDetail),
        (status = 403, description = "Public status page or network panel disabled"),
        (status = 404, description = "Server not in public scope"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn network_server_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<svc::PublicNetworkServerDetail>>, AppError> {
    let scope = resolve_enabled_scope(&state).await?;
    gate_subpage(&scope, |c| c.show_network)?;
    gate_scope_id(&scope, &id)?;
    let detail = svc::network_server_detail(
        &state.db,
        &state.agent_manager,
        &state.config.network_probe,
        &id,
    )
    .await?;
    ok(detail)
}

#[utoipa::path(
    get,
    path = "/api/status/ip-quality",
    tag = "public-status",
    responses(
        (status = 200, description = "Redacted IP-quality overview", body = svc::PublicIpQualityOverview),
        (status = 403, description = "Public status page or IP-quality panel disabled"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn ip_quality_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<svc::PublicIpQualityOverview>>, AppError> {
    let scope = resolve_enabled_scope(&state).await?;
    gate_subpage(&scope, |c| c.show_ip_quality)?;
    let overview = svc::ip_quality_overview(&state.db).await?;
    ok(overview)
}

/// Payload shape for `/api/status/incidents`. Splits active vs recent
/// (resolved-within-7-days) incidents so the SPA can render them in separate
/// regions without a second query.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct PublicIncidentsResponse {
    pub active: Vec<svc::PublicIncident>,
    pub recent: Vec<svc::PublicIncident>,
}

#[utoipa::path(
    get,
    path = "/api/status/incidents",
    operation_id = "public_list_incidents",
    tag = "public-status",
    responses(
        (status = 200, description = "Active and recently-resolved incidents", body = PublicIncidentsResponse),
        (status = 403, description = "Public status page or incidents panel disabled"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn list_incidents(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<PublicIncidentsResponse>>, AppError> {
    let scope = resolve_enabled_scope(&state).await?;
    gate_subpage(&scope, |c| c.show_incidents)?;
    let (active, recent) = svc::list_incidents(&state.db).await?;
    ok(PublicIncidentsResponse { active, recent })
}

#[utoipa::path(
    get,
    path = "/api/status/maintenances",
    operation_id = "public_list_maintenances",
    tag = "public-status",
    responses(
        (status = 200, description = "Upcoming and active maintenance windows", body = Vec<svc::PublicMaintenance>),
        (status = 403, description = "Public status page or maintenance panel disabled"),
        (status = 429, description = "Rate limit exceeded"),
    )
)]
async fn list_maintenances(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<svc::PublicMaintenance>>>, AppError> {
    let scope = resolve_enabled_scope(&state).await?;
    gate_subpage(&scope, |c| c.show_maintenance)?;
    let rows = svc::list_maintenances(&state.db).await?;
    ok(rows)
}
