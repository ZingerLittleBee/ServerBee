use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, State};
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};

use crate::router::utils::extract_client_ip;

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::service::audit::AuditService;
use crate::service::auth::{AuthService, LoginParams};
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    username: String,
    password: String,
    totp_code: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    user_id: String,
    username: String,
    role: String,
    must_change_password: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    user_id: String,
    username: String,
    role: String,
    must_change_password: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateApiKeyRequest {
    name: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApiKeyResponse {
    id: String,
    name: String,
    key_prefix: String,
    created_at: String,
    key: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    old_password: String,
    new_password: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OnboardingRequest {
    new_password: String,
    new_username: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TotpSetupResponse {
    secret: String,
    otpauth_url: String,
    qr_code_base64: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TotpVerifyRequest {
    code: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TotpDisableRequest {
    password: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TotpStatusResponse {
    enabled: bool,
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
        .route("/auth/onboarding", post(onboarding))
        // 2FA
        .route("/auth/2fa/setup", post(totp_setup))
        .route("/auth/2fa/enable", post(totp_enable))
        .route("/auth/2fa/disable", post(totp_disable))
        .route("/auth/2fa/status", get(totp_status))
        // OAuth accounts management
        .route("/auth/oauth/accounts", get(list_oauth_accounts))
        .route("/auth/oauth/accounts/{id}", delete(unlink_oauth_account))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 422, description = "2FA code required (code: 2fa_required)"),
    )
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req_headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<ApiResponse<LoginResponse>>), AppError> {
    if body.username.is_empty() || body.password.is_empty() {
        return Err(AppError::Validation(
            "Username and password are required".to_string(),
        ));
    }

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let user_agent = extract_user_agent(&req_headers);

    // Rate limiting
    if !state.check_login_rate(&ip) {
        // Record the lockout so brute-force activity leaves a durable trail
        // (the rate-limit counter itself is in-memory and lost on restart).
        let _ = AuditService::log(
            &state.db,
            "anonymous",
            "login_rate_limited",
            Some(&format!("username={}", body.username)),
            &ip,
        )
        .await;
        return Err(AppError::TooManyRequests(
            "Too many login attempts. Please try again later.".to_string(),
        ));
    }

    let (session, user) = match AuthService::login(
        &state.db,
        LoginParams {
            username: &body.username,
            password: &body.password,
            totp_code: body.totp_code.as_deref(),
            ip: &ip,
            user_agent: &user_agent,
            session_ttl: state.config.auth.session_ttl,
        },
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            // Audit the failed attempt (best-effort). The attempted username
            // and client IP are recorded; the submitted password never is.
            let _ = AuditService::log(
                &state.db,
                "anonymous",
                "login_failed",
                Some(&format!("username={}", body.username)),
                &ip,
            )
            .await;
            return Err(e);
        }
    };

    let secure_flag = if state.config.auth.secure_cookie {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "session_token={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}{}",
        session.token, state.config.auth.session_ttl, secure_flag
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        cookie
            .parse()
            .map_err(|_| AppError::Internal("Failed to set cookie".to_string()))?,
    );

    // Audit log (best-effort, don't fail login on audit error)
    let _ = AuditService::log(&state.db, &user.id, "login", None, &ip).await;

    let response = ApiResponse {
        data: LoginResponse {
            user_id: user.id,
            username: user.username,
            role: user.role,
            must_change_password: user.must_change_password,
        },
    };

    Ok((headers, Json(response)))
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    tag = "auth",
    responses(
        (status = 200, description = "Logout successful"),
    ),
    security(("session_cookie" = []))
)]
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<ApiResponse<&'static str>>), AppError> {
    if let Some(cookie_header) = headers.get("cookie")
        && let Ok(cookies) = cookie_header.to_str()
    {
        for cookie in cookies.split(';') {
            let cookie = cookie.trim();
            if let Some(token) = cookie.strip_prefix("session_token=") {
                AuthService::logout(&state.db, token).await?;
            }
        }
    }

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

#[utoipa::path(
    get,
    path = "/api/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current user info", body = MeResponse),
        (status = 401, description = "Unauthorized"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn me(
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<MeResponse>>, AppError> {
    ok(MeResponse {
        user_id: current_user.user_id,
        username: current_user.username,
        role: current_user.role,
        must_change_password: current_user.must_change_password,
    })
}

#[utoipa::path(
    post,
    path = "/api/auth/api-keys",
    tag = "auth",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 200, description = "API key created", body = ApiKeyResponse),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn create_api_key(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req_headers: HeaderMap,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<ApiResponse<ApiKeyResponse>>, AppError> {
    if body.name.is_empty() {
        return Err(AppError::Validation("Name is required".to_string()));
    }

    let (model, plaintext_key) =
        AuthService::create_api_key(&state.db, &current_user.user_id, &body.name).await?;

    // Audit: API keys are full credentials, so their lifecycle is security-sensitive.
    let caller_ip =
        extract_client_ip(&ConnectInfo(addr), &req_headers, &state.config.server.trusted_proxies)
            .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "api_key.create",
        Some(&format!(
            "id={} name={} prefix={}",
            model.id, model.name, model.key_prefix
        )),
        &caller_ip,
    )
    .await;

    ok(ApiKeyResponse {
        id: model.id,
        name: model.name,
        key_prefix: model.key_prefix,
        created_at: model.created_at.to_rfc3339(),
        key: Some(plaintext_key),
    })
}

#[utoipa::path(
    get,
    path = "/api/auth/api-keys",
    tag = "auth",
    responses(
        (status = 200, description = "List of API keys", body = Vec<ApiKeyResponse>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_api_keys(
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

#[utoipa::path(
    delete,
    path = "/api/auth/api-keys/{id}",
    tag = "auth",
    params(("id" = String, Path, description = "API key ID")),
    responses(
        (status = 200, description = "API key deleted"),
        (status = 404, description = "API key not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn delete_api_key(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req_headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    AuthService::delete_api_key(&state.db, &id, &current_user.user_id).await?;

    // Audit: revoking an API key is security-sensitive.
    let caller_ip =
        extract_client_ip(&ConnectInfo(addr), &req_headers, &state.config.server.trusted_proxies)
            .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "api_key.delete",
        Some(&format!("id={id}")),
        &caller_ip,
    )
    .await;

    ok("ok")
}

#[utoipa::path(
    put,
    path = "/api/auth/password",
    tag = "auth",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn change_password(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    req_headers: HeaderMap,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    if body.new_password.is_empty() {
        return Err(AppError::Validation("New password is required".to_string()));
    }

    // Preserve the caller's current session (cookie or bearer) while revoking
    // any other sessions the user may have, so the change can't be undone by a
    // previously stolen session. An API-key authenticated caller has no session
    // token here, so `current_token` is None and all of the user's sessions are
    // revoked — the API key itself is unaffected.
    let current_token = extract_session_cookie_token(&req_headers)
        .or_else(|| extract_bearer_token_value(&req_headers));
    AuthService::change_password(
        &state.db,
        &current_user.user_id,
        &body.old_password,
        &body.new_password,
        current_token.as_deref(),
    )
    .await?;

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "change_password",
        None,
        &ip,
    )
    .await;

    ok("ok")
}

#[utoipa::path(
    post,
    path = "/api/auth/onboarding",
    tag = "auth",
    request_body = OnboardingRequest,
    responses(
        (status = 200, description = "Onboarding complete"),
        (status = 403, description = "Onboarding not required / forbidden"),
        (status = 409, description = "Username already taken"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn onboarding(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    req_headers: HeaderMap,
    Json(body): Json<OnboardingRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    AuthService::complete_onboarding(
        &state.db,
        &current_user.user_id,
        &body.new_password,
        body.new_username.as_deref(),
    )
    .await?;

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(&state.db, &current_user.user_id, "onboarding", None, &ip).await;

    ok("ok")
}

// ── 2FA (TOTP) Endpoints ──

#[utoipa::path(
    post,
    path = "/api/auth/2fa/setup",
    tag = "2fa",
    responses(
        (status = 200, description = "TOTP setup data", body = TotpSetupResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn totp_setup(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<TotpSetupResponse>>, AppError> {
    let (secret, url, qr) = AuthService::generate_totp_secret(&current_user.username)?;

    // Store the secret server-side, keyed by user_id (10 min TTL enforced on read)
    state.pending_totp.insert(
        current_user.user_id.clone(),
        crate::state::PendingTotp {
            secret: secret.clone(),
            created_at: chrono::Utc::now(),
        },
    );

    ok(TotpSetupResponse {
        secret,
        otpauth_url: url,
        qr_code_base64: qr,
    })
}

#[utoipa::path(
    post,
    path = "/api/auth/2fa/enable",
    tag = "2fa",
    request_body = TotpVerifyRequest,
    responses(
        (status = 200, description = "2FA enabled"),
        (status = 401, description = "Invalid TOTP code"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn totp_enable(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    req_headers: HeaderMap,
    Json(body): Json<TotpVerifyRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    // Retrieve the server-stored secret for this user
    let pending = state
        .pending_totp
        .remove(&current_user.user_id)
        .ok_or_else(|| {
            AppError::BadRequest(
                "No pending 2FA setup. Call /api/auth/2fa/setup first.".to_string(),
            )
        })?;

    let (_, pending_totp) = pending;

    // Check TTL (10 minutes)
    if chrono::Utc::now() - pending_totp.created_at > chrono::Duration::minutes(10) {
        return Err(AppError::BadRequest(
            "2FA setup expired. Please start again.".to_string(),
        ));
    }

    // Verify the code against the server-stored secret
    if !AuthService::verify_totp(&pending_totp.secret, &body.code)? {
        // Re-insert so user can retry
        state.pending_totp.insert(
            current_user.user_id.clone(),
            crate::state::PendingTotp {
                secret: pending_totp.secret,
                created_at: pending_totp.created_at,
            },
        );
        return Err(AppError::Unauthorized);
    }

    AuthService::enable_2fa(&state.db, &current_user.user_id, &pending_totp.secret).await?;

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(&state.db, &current_user.user_id, "2fa_enable", None, &ip).await;

    ok("ok")
}

#[utoipa::path(
    post,
    path = "/api/auth/2fa/disable",
    tag = "2fa",
    request_body = TotpDisableRequest,
    responses(
        (status = 200, description = "2FA disabled"),
        (status = 400, description = "Invalid password"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn totp_disable(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    req_headers: HeaderMap,
    Json(body): Json<TotpDisableRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    // Verify password before disabling
    let user = crate::entity::user::Entity::find_by_id(&current_user.user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound("User not found".to_string()))?;

    if !AuthService::verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::BadRequest("Password is incorrect".to_string()));
    }

    AuthService::disable_2fa(&state.db, &current_user.user_id).await?;

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &req_headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(&state.db, &current_user.user_id, "2fa_disable", None, &ip).await;

    ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/auth/2fa/status",
    tag = "2fa",
    responses(
        (status = 200, description = "2FA status", body = TotpStatusResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn totp_status(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<TotpStatusResponse>>, AppError> {
    let enabled = AuthService::has_2fa(&state.db, &current_user.user_id).await?;
    ok(TotpStatusResponse { enabled })
}

// ── OAuth Account Management ──

#[utoipa::path(
    get,
    path = "/api/auth/oauth/accounts",
    tag = "oauth",
    responses(
        (status = 200, description = "List linked OAuth accounts", body = Vec<crate::entity::oauth_account::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_oauth_accounts(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<ApiResponse<Vec<crate::entity::oauth_account::Model>>>, AppError> {
    let accounts =
        crate::service::oauth::OAuthService::list_accounts(&state.db, &current_user.user_id)
            .await?;
    ok(accounts)
}

#[utoipa::path(
    delete,
    path = "/api/auth/oauth/accounts/{id}",
    tag = "oauth",
    params(("id" = String, Path, description = "OAuth account ID")),
    responses(
        (status = 200, description = "OAuth account unlinked"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn unlink_oauth_account(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    crate::service::oauth::OAuthService::unlink_account(&state.db, &id, &current_user.user_id)
        .await?;
    ok("ok")
}

// ── Helpers ──

/// Extract the User-Agent from request headers.
fn extract_user_agent(headers: &HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}

/// Extract the `session_token` value from the Cookie header, if present.
fn extract_session_cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            cookie
                .trim()
                .strip_prefix("session_token=")
                .map(|v| v.to_string())
        })
}

/// Extract the bearer token from the Authorization header, if present.
fn extract_bearer_token_value(headers: &HeaderMap) -> Option<String> {
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
        ConnectInfo("198.51.100.7:5555".parse().unwrap())
    }

    #[tokio::test]
    async fn login_with_wrong_password_is_audited_without_leaking_password() {
        let (db, _tmp) = setup_test_db().await;
        AuthService::create_user(&db, "alice", "correct-horse", "member")
            .await
            .unwrap();
        let state = AppState::new(db.clone(), AppConfig::default()).await.unwrap();

        let result = login(
            State(state.clone()),
            conn(),
            HeaderMap::new(),
            Json(LoginRequest {
                username: "alice".to_string(),
                password: "wrong-password".to_string(),
                totp_code: None,
            }),
        )
        .await;
        assert!(result.is_err(), "login with a wrong password must fail");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "login_failed"
                && l.detail.as_deref().is_some_and(|d| d.contains("alice"))),
            "expected a login_failed audit row naming the username, got: {logs:?}"
        );
        assert!(
            logs.iter().all(|l| !l
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("wrong-password")),
            "audit detail must never contain the attempted password"
        );
    }

    #[tokio::test]
    async fn rate_limited_login_is_audited() {
        let (db, _tmp) = setup_test_db().await;
        let mut config = AppConfig::default();
        // Force the very first attempt from an IP to be rejected as rate-limited.
        config.rate_limit.login_max = 0;
        let state = AppState::new(db.clone(), config).await.unwrap();

        let result = login(
            State(state.clone()),
            conn(),
            HeaderMap::new(),
            Json(LoginRequest {
                username: "mallory".to_string(),
                password: "whatever".to_string(),
                totp_code: None,
            }),
        )
        .await;
        assert!(result.is_err(), "login must be rejected when rate-limited");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "login_rate_limited"
                && l.detail.as_deref().is_some_and(|d| d.contains("mallory"))),
            "expected a login_rate_limited audit row, got: {logs:?}"
        );
    }
}
