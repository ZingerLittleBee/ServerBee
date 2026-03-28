use std::sync::Arc;

use axum::http::HeaderMap;
use axum::{extract::State, routing::post, Json, Router};
use serde::Deserialize;

use crate::error::{ApiResponse, AppError};
use crate::service::mobile_auth::{MobileAuthService, MobileTokenResponse};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mobile/auth/login", post(login))
        .route("/mobile/auth/refresh", post(refresh))
        .route("/mobile/auth/logout", post(logout))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct LoginRequest {
    username: String,
    password: String,
    totp_code: Option<String>,
    installation_id: String,
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/login",
    request_body = LoginRequest,
    responses((status = 200, body = ApiResponse<MobileTokenResponse>)),
    tag = "Mobile Auth"
)]
async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Json<ApiResponse<MobileTokenResponse>>, AppError> {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if !state.check_login_rate(&ip) {
        return Err(AppError::TooManyRequests("Too many login attempts".to_string()));
    }

    let result = MobileAuthService::login(
        &state.db,
        &state.jwt,
        &req.username,
        &req.password,
        req.totp_code.as_deref(),
        &req.installation_id,
        state.config.mobile.refresh_token_ttl,
        &ip,
        &user_agent,
    )
    .await?;

    Ok(Json(ApiResponse { data: result }))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct RefreshRequest {
    refresh_token: String,
    installation_id: String,
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/refresh",
    request_body = RefreshRequest,
    responses((status = 200, body = ApiResponse<MobileTokenResponse>)),
    tag = "Mobile Auth"
)]
async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<ApiResponse<MobileTokenResponse>>, AppError> {
    let result = MobileAuthService::refresh(
        &state.db,
        &state.jwt,
        &req.refresh_token,
        &req.installation_id,
        state.config.mobile.refresh_token_ttl,
    )
    .await?;

    Ok(Json(ApiResponse { data: result }))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct LogoutRequest {
    refresh_token: String,
    installation_id: String,
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/logout",
    request_body = LogoutRequest,
    responses((status = 200)),
    tag = "Mobile Auth"
)]
async fn logout(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LogoutRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    MobileAuthService::logout(&state.db, &req.refresh_token, &req.installation_id).await?;
    Ok(Json(ApiResponse { data: () }))
}
