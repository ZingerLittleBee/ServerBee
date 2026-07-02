use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use rand::RngCore;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::api::auth::login_audit_detail;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::mobile_auth::{MobileAuthService, MobileLoginParams, MobileTokenResponse};
use crate::state::{AppState, PendingPair};

// ── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MobileLoginRequest {
    username: String,
    password: String,
    installation_id: String,
    device_name: String,
    totp_code: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MobileRefreshRequest {
    refresh_token: String,
    installation_id: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MobilePairRedeemRequest {
    code: String,
    installation_id: String,
    device_name: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobilePairCodeResponse {
    code: String,
    expires_in_secs: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PushRegisterRequest {
    /// The APNs device token obtained from the iOS device.
    pub device_token: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MobileDeviceResponse {
    id: String,
    device_name: String,
    installation_id: String,
    created_at: String,
    last_used_at: String,
}

// ── Routers ──────────────────────────────────────────────────────────────────

/// Public routes (no auth required).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mobile/auth/login", post(mobile_login))
        .route("/mobile/auth/refresh", post(mobile_refresh))
        .route("/mobile/auth/pair", post(mobile_pair_redeem))
}

/// Protected routes (auth required).
pub fn protected_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mobile/auth/logout", post(mobile_logout))
        .route("/mobile/auth/devices", get(list_devices))
        .route("/mobile/auth/devices/{id}", delete(revoke_device))
        .route("/mobile/pair", post(generate_pair_code))
        .route("/mobile/push/register", post(push_register))
        .route("/mobile/push/unregister", post(push_unregister))
}

// ── Handlers ─────────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/mobile/auth/login",
    tag = "mobile-auth",
    request_body = MobileLoginRequest,
    responses(
        (status = 200, description = "Mobile login successful", body = MobileTokenResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 422, description = "2FA code required (code: 2fa_required)"),
        (status = 429, description = "Too many login attempts"),
    )
)]
pub async fn mobile_login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req_headers: HeaderMap,
    Json(body): Json<MobileLoginRequest>,
) -> Result<Json<ApiResponse<MobileTokenResponse>>, AppError> {
    if body.username.is_empty() || body.password.is_empty() {
        return Err(AppError::Validation(
            "Username and password are required".to_string(),
        ));
    }
    if body.installation_id.is_empty() || body.device_name.is_empty() {
        return Err(AppError::Validation(
            "installation_id and device_name are required".to_string(),
        ));
    }

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let user_agent = extract_user_agent(&req_headers);

    // Rate limiting (shares the per-IP counter with the web login endpoint).
    if !state.check_login_rate(&ip) {
        let _ = AuditService::log(
            &state.db,
            "anonymous",
            "login_rate_limited",
            Some(&login_audit_detail(&body.username)),
            &ip,
        )
        .await;
        return Err(AppError::TooManyRequests(
            "Too many login attempts. Please try again later.".to_string(),
        ));
    }

    let response = match MobileAuthService::login(
        &state.db,
        &state.config.mobile,
        MobileLoginParams {
            username: &body.username,
            password: &body.password,
            totp_code: body.totp_code.as_deref(),
            installation_id: &body.installation_id,
            device_name: &body.device_name,
            ip: &ip,
            user_agent: &user_agent,
        },
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            // Mirror the web login audit so brute force cannot dodge the trail
            // by targeting the mobile endpoint. Only genuine credential
            // failures are recorded; the password is never included.
            if matches!(&e, AppError::Unauthorized) {
                let _ = AuditService::log(
                    &state.db,
                    "anonymous",
                    "login_failed",
                    Some(&login_audit_detail(&body.username)),
                    &ip,
                )
                .await;
            }
            return Err(e);
        }
    };

    ok(response)
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/refresh",
    tag = "mobile-auth",
    request_body = MobileRefreshRequest,
    responses(
        (status = 200, description = "Token refreshed", body = MobileTokenResponse),
        (status = 401, description = "Invalid or expired refresh token"),
    )
)]
pub async fn mobile_refresh(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req_headers: HeaderMap,
    Json(body): Json<MobileRefreshRequest>,
) -> Result<Json<ApiResponse<MobileTokenResponse>>, AppError> {
    if body.refresh_token.is_empty() || body.installation_id.is_empty() {
        return Err(AppError::Validation(
            "refresh_token and installation_id are required".to_string(),
        ));
    }

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let user_agent = extract_user_agent(&req_headers);

    let response = MobileAuthService::refresh(
        &state.db,
        &state.config.mobile,
        &body.refresh_token,
        &body.installation_id,
        &ip,
        &user_agent,
    )
    .await?;

    ok(response)
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/logout",
    tag = "mobile-auth",
    responses(
        (status = 200, description = "Logged out"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_token" = []))
)]
pub async fn mobile_logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    let token = extract_bearer(&headers).ok_or(AppError::Unauthorized)?;

    // Find the session row by token to get mobile_session_id
    let session = crate::entity::session::Entity::find()
        .filter(
            crate::entity::session::Column::Token
                .eq(crate::service::auth::AuthService::hash_session_token(&token)),
        )
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let mobile_session_id = session
        .mobile_session_id
        .ok_or_else(|| AppError::BadRequest("This session is not a mobile session".to_string()))?;

    MobileAuthService::logout(&state.db, &mobile_session_id).await?;

    ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/mobile/auth/devices",
    tag = "mobile-auth",
    responses(
        (status = 200, description = "List of mobile devices", body = Vec<MobileDeviceResponse>),
        (status = 401, description = "Unauthorized"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_devices(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<Vec<MobileDeviceResponse>>>, AppError> {
    let devices = MobileAuthService::list_devices(&state.db, &current_user.user_id).await?;

    let response: Vec<MobileDeviceResponse> = devices
        .into_iter()
        .map(|d| MobileDeviceResponse {
            id: d.id,
            device_name: d.device_name,
            installation_id: d.installation_id,
            created_at: d.created_at.to_rfc3339(),
            last_used_at: d.last_used_at.to_rfc3339(),
        })
        .collect();

    ok(response)
}

#[utoipa::path(
    delete,
    path = "/api/mobile/auth/devices/{id}",
    tag = "mobile-auth",
    params(("id" = String, Path, description = "Mobile session ID")),
    responses(
        (status = 200, description = "Device revoked"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Cannot revoke another user's device"),
        (status = 404, description = "Mobile session not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn revoke_device(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    MobileAuthService::revoke_device(&state.db, &id, &current_user.user_id).await?;
    ok("ok")
}

/// Pairing code TTL in seconds.
const PAIR_CODE_TTL_SECS: i64 = 300;

#[utoipa::path(
    post,
    path = "/api/mobile/pair",
    tag = "mobile-auth",
    responses(
        (status = 200, description = "Pairing code generated", body = MobilePairCodeResponse),
        (status = 401, description = "Unauthorized"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn generate_pair_code(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<MobilePairCodeResponse>>, AppError> {
    // Clean up expired codes (5 min TTL) and existing codes for this user
    let now = chrono::Utc::now();
    state.pending_pairs.retain(|_, v| {
        v.user_id != current_user.user_id && (now - v.created_at) < chrono::Duration::minutes(5)
    });

    // Generate a 32-byte random base64url code with sb_pair_ prefix
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let code = format!("sb_pair_{}", URL_SAFE_NO_PAD.encode(bytes));

    state.pending_pairs.insert(
        code.clone(),
        PendingPair {
            user_id: current_user.user_id.clone(),
            created_at: chrono::Utc::now(),
        },
    );

    ok(MobilePairCodeResponse {
        code,
        expires_in_secs: PAIR_CODE_TTL_SECS,
    })
}

#[utoipa::path(
    post,
    path = "/api/mobile/auth/pair",
    tag = "mobile-auth",
    request_body = MobilePairRedeemRequest,
    responses(
        (status = 200, description = "Pairing successful", body = MobileTokenResponse),
        (status = 400, description = "Invalid or expired pairing code"),
        (status = 422, description = "Validation error"),
    )
)]
pub async fn mobile_pair_redeem(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req_headers: HeaderMap,
    Json(body): Json<MobilePairRedeemRequest>,
) -> Result<Json<ApiResponse<MobileTokenResponse>>, AppError> {
    if body.code.is_empty() || body.installation_id.is_empty() || body.device_name.is_empty() {
        return Err(AppError::Validation(
            "code, installation_id, and device_name are required".to_string(),
        ));
    }

    // Look up and remove the pairing code
    let (_, pending) = state
        .pending_pairs
        .remove(&body.code)
        .ok_or_else(|| AppError::BadRequest("Invalid pairing code".to_string()))?;

    // Check 5-minute TTL
    if chrono::Utc::now() - pending.created_at > chrono::Duration::seconds(PAIR_CODE_TTL_SECS) {
        return Err(AppError::BadRequest("Pairing code has expired".to_string()));
    }

    // Fetch the user who generated this code
    let user = crate::entity::user::Entity::find_by_id(&pending.user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::BadRequest("User not found".to_string()))?;

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let user_agent = extract_user_agent(&req_headers);

    let response = MobileAuthService::login_for_user(
        &state.db,
        &state.config.mobile,
        &user,
        &body.installation_id,
        &body.device_name,
        &ip,
        &user_agent,
    )
    .await?;

    ok(response)
}

// ── Push token management ────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/mobile/push/register",
    tag = "mobile-auth",
    request_body = PushRegisterRequest,
    responses(
        (status = 200, description = "Device token registered"),
        (status = 401, description = "Unauthorized"),
        (status = 422, description = "Validation error"),
    ),
    security(("bearer_token" = []))
)]
pub async fn push_register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PushRegisterRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    if body.device_token.is_empty() {
        return Err(AppError::Validation("device_token is required".to_string()));
    }

    let token = extract_bearer(&headers).ok_or(AppError::Unauthorized)?;

    // Find the session by bearer token
    let session = crate::entity::session::Entity::find()
        .filter(
            crate::entity::session::Column::Token
                .eq(crate::service::auth::AuthService::hash_session_token(&token)),
        )
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let mobile_session_id = session
        .mobile_session_id
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("This session is not a mobile session".to_string()))?;

    // Look up the mobile session to get installation_id
    let mobile_session = crate::entity::mobile_session::Entity::find_by_id(mobile_session_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::BadRequest("Mobile session not found".to_string()))?;

    // Upsert: find by installation_id, update if exists, insert if not
    let existing = crate::entity::device_token::Entity::find()
        .filter(
            crate::entity::device_token::Column::InstallationId.eq(&mobile_session.installation_id),
        )
        .one(&state.db)
        .await?;

    let now = Utc::now();

    if let Some(existing) = existing {
        // installation_id is a client-supplied value and must not be treated
        // as an ownership boundary. A device_token row belongs to whichever
        // user registered it; legitimate device hand-off already deletes the
        // previous owner's row when their mobile session is invalidated
        // (see mobile_auth session cleanup). Reaching here with a different
        // user_id therefore means a caller is trying to overwrite another
        // user's push registration via a forged installation_id — reject it.
        if existing.user_id != session.user_id {
            tracing::warn!(
                "Rejected push_register hijack attempt: user {} tried to overwrite installation {} owned by user {}",
                session.user_id,
                mobile_session.installation_id,
                existing.user_id
            );
            return Err(AppError::Forbidden(
                "This device is registered to another account".to_string(),
            ));
        }
        let mut model: crate::entity::device_token::ActiveModel = existing.into();
        model.token = Set(body.device_token);
        model.mobile_session_id = Set(mobile_session_id.to_string());
        model.updated_at = Set(now);
        model.update(&state.db).await?;
    } else {
        let model = crate::entity::device_token::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            user_id: Set(session.user_id.clone()),
            mobile_session_id: Set(mobile_session_id.to_string()),
            installation_id: Set(mobile_session.installation_id.clone()),
            token: Set(body.device_token),
            created_at: Set(now),
            updated_at: Set(now),
        };
        model.insert(&state.db).await?;
    }

    ok("ok")
}

#[utoipa::path(
    post,
    path = "/api/mobile/push/unregister",
    tag = "mobile-auth",
    responses(
        (status = 200, description = "Device token unregistered"),
        (status = 401, description = "Unauthorized"),
    ),
    security(("bearer_token" = []))
)]
pub async fn push_unregister(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    let token = extract_bearer(&headers).ok_or(AppError::Unauthorized)?;

    let session = crate::entity::session::Entity::find()
        .filter(
            crate::entity::session::Column::Token
                .eq(crate::service::auth::AuthService::hash_session_token(&token)),
        )
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let mobile_session_id = session
        .mobile_session_id
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("This session is not a mobile session".to_string()))?;

    let mobile_session = crate::entity::mobile_session::Entity::find_by_id(mobile_session_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::BadRequest("Mobile session not found".to_string()))?;

    // Delete the device token for this installation, scoped to the caller's
    // own user. installation_id is a client-supplied value and must not be
    // treated as an ownership boundary: filtering by user_id as well prevents
    // a member from deleting another user's push registration by forging the
    // victim's installation_id.
    crate::entity::device_token::Entity::delete_many()
        .filter(
            crate::entity::device_token::Column::InstallationId.eq(&mobile_session.installation_id),
        )
        .filter(crate::entity::device_token::Column::UserId.eq(&session.user_id))
        .exec(&state.db)
        .await?;

    ok("ok")
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the User-Agent from request headers.
fn extract_user_agent(headers: &HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}

/// Extract Bearer token from Authorization header.
fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::audit_log;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use sea_orm::EntityTrait;

    fn conn() -> ConnectInfo<SocketAddr> {
        ConnectInfo("198.51.100.9:7777".parse().unwrap())
    }

    fn req(username: &str, password: &str) -> MobileLoginRequest {
        MobileLoginRequest {
            username: username.to_string(),
            password: password.to_string(),
            installation_id: "inst-test".to_string(),
            device_name: "Test Device".to_string(),
            totp_code: None,
        }
    }

    #[tokio::test]
    async fn mobile_login_wrong_password_is_audited() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "dave", "correct-horse", "member")
            .await
            .unwrap();
        let state = AppState::new(db.clone(), AppConfig::default()).await.unwrap();

        let result = mobile_login(
            State(state.clone()),
            conn(),
            HeaderMap::new(),
            Json(req("dave", "wrong-password")),
        )
        .await;
        assert!(result.is_err(), "wrong password must fail");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "login_failed"
                && l.detail.as_deref().is_some_and(|d| d.contains("dave"))),
            "mobile login must leave a login_failed audit row, got: {logs:?}"
        );
        assert!(
            logs.iter().all(|l| !l
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("wrong-password")),
            "the submitted password must never be recorded"
        );
    }

    #[tokio::test]
    async fn mobile_login_rate_limited_is_audited() {
        let (db, _tmp) = setup_test_db().await;
        let mut config = AppConfig::default();
        config.rate_limit.login_max = 0;
        let state = AppState::new(db.clone(), config).await.unwrap();

        let result = mobile_login(
            State(state.clone()),
            conn(),
            HeaderMap::new(),
            Json(req("eve", "whatever")),
        )
        .await;
        assert!(result.is_err(), "must be rejected when rate-limited");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "login_rate_limited"
                && l.detail.as_deref().is_some_and(|d| d.contains("eve"))),
            "mobile rate-limited login must be audited, got: {logs:?}"
        );
    }
}
