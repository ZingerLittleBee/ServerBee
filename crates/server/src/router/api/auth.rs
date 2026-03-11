use std::sync::Arc;

use axum::extract::{Extension, State};
use axum::http::header::SET_COOKIE;
use axum::http::HeaderMap;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ok, ApiResponse, AppError};
use crate::middleware::auth::CurrentUser;
use crate::service::auth::AuthService;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    user_id: String,
    username: String,
    role: String,
}

#[derive(Debug, Serialize)]
struct MeResponse {
    user_id: String,
    username: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
}

#[derive(Debug, Serialize)]
struct ApiKeyResponse {
    id: String,
    name: String,
    key_prefix: String,
    created_at: String,
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    old_password: String,
    new_password: String,
}

/// Public routes (no auth required).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new().route("/auth/login", post(login))
}

/// Protected routes (auth required).
pub fn protected_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/api-keys", post(create_api_key))
        .route("/auth/api-keys", get(list_api_keys))
        .route("/auth/api-keys/{id}", delete(delete_api_key))
        .route("/auth/password", put(change_password))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<ApiResponse<LoginResponse>>), AppError> {
    if body.username.is_empty() || body.password.is_empty() {
        return Err(AppError::Validation(
            "Username and password are required".to_string(),
        ));
    }

    let (session, user) = AuthService::login(
        &state.db,
        &body.username,
        &body.password,
        "unknown",
        "unknown",
        state.config.auth.session_ttl,
    )
    .await?;

    let cookie = format!(
        "session_token={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400",
        session.token
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        cookie
            .parse()
            .map_err(|_| AppError::Internal("Failed to set cookie".to_string()))?,
    );

    let response = ApiResponse {
        data: LoginResponse {
            user_id: user.id,
            username: user.username,
            role: user.role,
        },
    };

    Ok((headers, Json(response)))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<ApiResponse<&'static str>>), AppError> {
    // Extract session token from cookie
    if let Some(cookie_header) = headers.get("cookie") {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("session_token=") {
                    AuthService::logout(&state.db, token).await?;
                }
            }
        }
    }

    // Clear the cookie
    let clear_cookie = "session_token=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        clear_cookie
            .parse()
            .map_err(|_| AppError::Internal("Failed to clear cookie".to_string()))?,
    );

    Ok((response_headers, Json(ApiResponse { data: "ok" })))
}

async fn me(
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<MeResponse>>, AppError> {
    ok(MeResponse {
        user_id: current_user.user_id,
        username: current_user.username,
        role: current_user.role,
    })
}

async fn create_api_key(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<ApiResponse<ApiKeyResponse>>, AppError> {
    if body.name.is_empty() {
        return Err(AppError::Validation("Name is required".to_string()));
    }

    let (model, plaintext_key) =
        AuthService::create_api_key(&state.db, &current_user.user_id, &body.name).await?;

    ok(ApiKeyResponse {
        id: model.id,
        name: model.name,
        key_prefix: model.key_prefix,
        created_at: model.created_at.to_rfc3339(),
        key: Some(plaintext_key),
    })
}

async fn list_api_keys(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<Vec<ApiKeyResponse>>>, AppError> {
    let keys = AuthService::list_api_keys(&state.db, &current_user.user_id).await?;

    let response: Vec<ApiKeyResponse> = keys
        .into_iter()
        .map(|k| ApiKeyResponse {
            id: k.id,
            name: k.name,
            key_prefix: k.key_prefix,
            created_at: k.created_at.to_rfc3339(),
            key: None,
        })
        .collect();

    ok(response)
}

async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    AuthService::delete_api_key(&state.db, &id, &current_user.user_id).await?;
    ok("ok")
}

async fn change_password(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    if body.new_password.is_empty() {
        return Err(AppError::Validation(
            "New password is required".to_string(),
        ));
    }

    AuthService::change_password(
        &state.db,
        &current_user.user_id,
        &body.old_password,
        &body.new_password,
    )
    .await?;

    ok("ok")
}
