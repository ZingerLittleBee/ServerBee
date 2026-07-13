use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ApiResponse, AppError, ok};
use crate::router::utils::extract_client_ip;
use crate::service::agent_authority::{
    ClaimAgent, ClaimError, EnrollmentCode, ProposedRunToken, RequestSource,
};
use crate::service::upgrade_release::LatestAgentVersionResponse;
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    proposed_run_token: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    server_id: String,
}

/// Public routes for Agent enrollment (Bearer auth is checked by the handler).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/register", post(register))
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/latest-version", get(latest_version))
}

#[utoipa::path(
    get,
    path = "/api/agent/latest-version",
    tag = "agent",
    responses(
        (status = 200, description = "Latest agent release metadata", body = LatestAgentVersionResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn latest_version(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<LatestAgentVersionResponse>>, AppError> {
    ok(state.upgrade_release_service.latest().await)
}

#[utoipa::path(
    post,
    path = "/api/agent/register",
    tag = "agent",
    request_body = RegisterRequest,
    responses(
        (status = 200, description = "Agent claimed the bound Server authority", body = RegisterResponse),
        (status = 400, description = "Missing or invalid Agent-proposed run token"),
        (status = 401, description = "Enrollment claim rejected"),
    ),
    security(("bearer_token" = []))
)]
async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<ApiResponse<RegisterResponse>>, AppError> {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    if !state.check_register_rate(&ip) {
        return Err(AppError::TooManyRequests(
            "Too many registration attempts, please try later".to_string(),
        ));
    }

    let code = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)
        .and_then(|value| EnrollmentCode::parse(value).map_err(|_| AppError::Unauthorized))?;
    let proposed_run_token =
        ProposedRunToken::parse(body.proposed_run_token).map_err(AppError::BadRequest)?;
    let source = RequestSource::parse("agent:register")
        .map_err(|error| AppError::Internal(format!("invalid request source: {error}")))?;

    let receipt = state
        .agent_authority
        .claim(ClaimAgent {
            code,
            proposed_run_token,
            source,
            remote_addr: Some(ip),
        })
        .await
        .map_err(|error| match error {
            ClaimError::Rejected => AppError::Unauthorized,
            ClaimError::Store(error) => error,
        })?;

    ok(RegisterResponse {
        server_id: receipt.server_id.into_inner(),
    })
}
