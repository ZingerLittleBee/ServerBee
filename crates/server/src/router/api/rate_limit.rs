use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::state::{AppState, RateLimitEntry};

const WINDOW_MINUTES: i64 = 15;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitScope {
    Login,
    Register,
}

impl RateLimitScope {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Register => "register",
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RateLimitEntryDto {
    pub scope: RateLimitScope,
    pub ip: String,
    pub count: u32,
    /// Configured maximum requests per window for this scope.
    pub max: u32,
    /// RFC 3339 timestamp the current window opened.
    pub window_start: String,
    /// Seconds until the window resets. Zero if already expired.
    pub seconds_remaining: i64,
    /// True if `count >= max` and the window is still open.
    pub blocked: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RateLimitListResponse {
    pub entries: Vec<RateLimitEntryDto>,
    pub window_minutes: i64,
    pub login_max: u32,
    pub register_max: u32,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RateLimitResetRequest {
    /// Optional scope filter; when omitted, clears both login and register.
    #[serde(default)]
    pub scope: Option<RateLimitScope>,
    /// Optional IP filter; when omitted, clears every entry in the selected scope(s).
    #[serde(default)]
    pub ip: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RateLimitResetResponse {
    pub cleared: u32,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin/rate-limit", get(list_rate_limits))
        .route("/admin/rate-limit/reset", post(reset_rate_limit))
}

fn collect_entries(
    map: &DashMap<String, RateLimitEntry>,
    scope: RateLimitScope,
    max: u32,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<RateLimitEntryDto> {
    let window = chrono::Duration::minutes(WINDOW_MINUTES);
    map.iter()
        .filter_map(|entry| {
            let elapsed = now - entry.window_start;
            // Skip windows that already expired — the next request will reset them anyway.
            if elapsed >= window {
                return None;
            }
            let seconds_remaining = (window - elapsed).num_seconds().max(0);
            Some(RateLimitEntryDto {
                scope,
                ip: entry.key().clone(),
                count: entry.count,
                max,
                window_start: entry.window_start.to_rfc3339(),
                seconds_remaining,
                blocked: entry.count >= max,
            })
        })
        .collect()
}

#[utoipa::path(
    get,
    path = "/api/admin/rate-limit",
    operation_id = "list_rate_limits",
    tag = "rate-limit",
    responses(
        (status = 200, description = "Current per-IP rate limit state", body = RateLimitListResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn list_rate_limits(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<RateLimitListResponse>>, AppError> {
    let now = Utc::now();
    let login_max = state.config.rate_limit.login_max;
    let register_max = state.config.rate_limit.register_max;

    let mut entries = collect_entries(&state.login_rate_limit, RateLimitScope::Login, login_max, now);
    entries.extend(collect_entries(
        &state.register_rate_limit,
        RateLimitScope::Register,
        register_max,
        now,
    ));

    // Sort: blocked first, then highest count, then most recent window.
    entries.sort_by(|a, b| {
        b.blocked
            .cmp(&a.blocked)
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| b.window_start.cmp(&a.window_start))
    });

    ok(RateLimitListResponse {
        entries,
        window_minutes: WINDOW_MINUTES,
        login_max,
        register_max,
    })
}

#[utoipa::path(
    post,
    path = "/api/admin/rate-limit/reset",
    operation_id = "reset_rate_limit",
    tag = "rate-limit",
    request_body = RateLimitResetRequest,
    responses(
        (status = 200, description = "Number of entries cleared", body = RateLimitResetResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn reset_rate_limit(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<RateLimitResetRequest>,
) -> Result<Json<ApiResponse<RateLimitResetResponse>>, AppError> {
    let scopes: &[RateLimitScope] = match req.scope {
        Some(s) => match s {
            RateLimitScope::Login => &[RateLimitScope::Login],
            RateLimitScope::Register => &[RateLimitScope::Register],
        },
        None => &[RateLimitScope::Login, RateLimitScope::Register],
    };

    let mut cleared: u32 = 0;
    for scope in scopes {
        let map = match scope {
            RateLimitScope::Login => &state.login_rate_limit,
            RateLimitScope::Register => &state.register_rate_limit,
        };
        match req.ip.as_deref() {
            Some(ip) if !ip.is_empty() => {
                if map.remove(ip).is_some() {
                    cleared += 1;
                }
            }
            _ => {
                cleared += u32::try_from(map.len()).unwrap_or(u32::MAX);
                map.clear();
            }
        }
    }

    let detail = format!(
        "scope={} ip={} cleared={cleared}",
        req.scope.map(|s| s.as_str()).unwrap_or("all"),
        req.ip.as_deref().unwrap_or("*"),
    );
    let caller_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &user.user_id,
        "rate_limit.reset",
        Some(&detail),
        &caller_ip,
    )
    .await;

    ok(RateLimitResetResponse { cleared })
}
