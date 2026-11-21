use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use axum::Json;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::{api_key, oauth_account, session, user};
use crate::error::AppError;
use crate::service::audit::AuditService;
use crate::service::auth::AuthService;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub must_change_password: bool,
}

impl From<crate::entity::user::Model> for CurrentUser {
    fn from(user: crate::entity::user::Model) -> Self {
        Self {
            user_id: user.id,
            username: user.username,
            role: user.role,
            must_change_password: user.must_change_password,
        }
    }
}

/// 403 response with a distinct machine-readable code so the frontend can
/// reliably detect the forced-password-change state. Deliberately NOT routed
/// through `AppError` (whose Forbidden code is always "FORBIDDEN").
fn must_change_password_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": {
                "code": "MUST_CHANGE_PASSWORD",
                "message": "Password change required before continuing"
            }
        })),
    )
        .into_response()
}

/// Paths (already `/api`-stripped by `.nest("/api", ...)`) that a flagged user
/// may still reach so they can complete onboarding.
fn is_onboarding_whitelisted(method: &axum::http::Method, path: &str) -> bool {
    matches!(
        (method.as_str(), path),
        ("GET", "/auth/me") | ("POST", "/auth/onboarding") | ("POST", "/auth/logout")
    )
}

/// Identity plus lifetime facts for an authenticated connection.
///
/// Long-lived transports (WebSockets) need more than the user: they must know
/// when a fixed-lifetime credential expires so they can close the connection
/// mid-stream. Short-lived HTTP requests simply ignore `mobile_expires`.
#[derive(Debug, Clone)]
pub struct AuthenticatedConnection {
    pub user: CurrentUser,
    /// Fixed expiry of the authenticating session when it is a non-web
    /// (mobile) session, whatever header carried it. Web sessions renew on
    /// use (sliding expiry) and API keys never expire, so both yield `None`.
    pub mobile_expires: Option<chrono::DateTime<chrono::Utc>>,
    pub credential: ConnectionCredential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionCredential {
    Session { id: String },
    ApiKey { id: String },
    Tailscale { user_id: String },
}

impl AuthenticatedConnection {
    /// Re-check the persisted credential and user policy for a long-lived
    /// connection. Role changes deliberately invalidate the lease so a socket
    /// cannot retain privileges captured during its handshake.
    pub async fn lease_is_valid(&self, state: &AppState) -> Result<bool, sea_orm::DbErr> {
        let current_user = match &self.credential {
            ConnectionCredential::Session { id } => session::Entity::find_by_id(id)
                .filter(session::Column::ExpiresAt.gt(chrono::Utc::now()))
                .find_also_related(user::Entity)
                .one(&state.db)
                .await?
                .and_then(|(session, user)| {
                    (session.user_id == self.user.user_id)
                        .then_some(user)
                        .flatten()
                }),
            ConnectionCredential::ApiKey { id } => api_key::Entity::find_by_id(id)
                .find_also_related(user::Entity)
                .one(&state.db)
                .await?
                .and_then(|(key, user)| {
                    (key.user_id == self.user.user_id).then_some(user).flatten()
                }),
            ConnectionCredential::Tailscale { user_id } => {
                let auth_still_enabled = state
                    .tailscale_control
                    .effective_config(&state.db)
                    .await
                    .map(|cfg| cfg.auth_enabled)
                    .unwrap_or(false);
                if !auth_still_enabled || user_id != &self.user.user_id {
                    None
                } else {
                    user::Entity::find_by_id(user_id).one(&state.db).await?
                }
            }
        };
        Ok(current_user
            .is_some_and(|user| !user.must_change_password && user.role == self.user.role))
    }
}

/// Resolve the authenticated connection (if any) from a request's headers
/// without peer TCP address context (fails closed for Tailscale loopback auth).
pub async fn resolve_connection(
    headers: &HeaderMap,
    state: &AppState,
) -> Option<AuthenticatedConnection> {
    resolve_connection_with_peer(headers, None, state).await
}

/// Resolve the authenticated connection (if any) from a request's headers and
/// optional TCP peer socket address.
///
/// Tries, in order: session cookie, `X-API-Key` header, `Bearer` token, and
/// (when `tailscale.auth_enabled` is true on a strict loopback connection
/// without `X-Forwarded-For` / `Forwarded` headers) `Tailscale-User-Login`.
/// The first credential that validates decides the identity and lifetime; later
/// rungs are only consulted when earlier ones are absent or invalid.
pub async fn resolve_connection_with_peer(
    headers: &HeaderMap,
    peer_addr: Option<SocketAddr>,
    state: &AppState,
) -> Option<AuthenticatedConnection> {
    let session_ttl = state.config.auth.session_ttl;

    // Try session cookie
    if let Some(token) = extract_session_cookie(headers)
        && let Some((user, session)) = AuthService::validate_session(&state.db, &token, session_ttl)
            .await
            .ok()
            .flatten()
    {
        return Some(AuthenticatedConnection {
            user: CurrentUser::from(user),
            mobile_expires: mobile_expiry(&session),
            credential: ConnectionCredential::Session { id: session.id },
        });
    }

    // Try API key header
    if let Some(key) = extract_api_key(headers)
        && let Some((user, api_key)) = AuthService::validate_api_key_with_model(&state.db, &key)
            .await
            .ok()
            .flatten()
    {
        return Some(AuthenticatedConnection {
            user: CurrentUser::from(user),
            mobile_expires: None,
            credential: ConnectionCredential::ApiKey { id: api_key.id },
        });
    }

    // Try Bearer token
    if let Some(token) = extract_bearer_token(headers)
        && let Some((user, session)) = AuthService::validate_session(&state.db, &token, session_ttl)
            .await
            .ok()
            .flatten()
    {
        return Some(AuthenticatedConnection {
            user: CurrentUser::from(user),
            mobile_expires: mobile_expiry(&session),
            credential: ConnectionCredential::Session { id: session.id },
        });
    }

    // Try Tailscale Serve loopback identity header
    if let Some(conn) = resolve_tailscale_serve_connection(headers, peer_addr, state).await {
        return Some(conn);
    }

    None
}

/// Returns `true` iff `ip` is strictly IPv4 `127.0.0.1` or IPv6 `::1`
/// (including dual-stack IPv4-mapped `::ffff:127.0.0.1`).
pub(crate) fn is_strict_loopback_peer(ip: IpAddr) -> bool {
    matches!(
        ip.to_canonical(),
        IpAddr::V4(Ipv4Addr::LOCALHOST) | IpAddr::V6(Ipv6Addr::LOCALHOST)
    )
}

pub(crate) fn extract_tailscale_login(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("tailscale-user-login")?.to_str().ok()?.trim();
    if raw.is_empty() || raw.len() > 254 || raw.chars().any(char::is_control) {
        return None;
    }
    Some(raw.to_string())
}

async fn resolve_tailscale_serve_connection(
    headers: &HeaderMap,
    peer_addr: Option<SocketAddr>,
    state: &AppState,
) -> Option<AuthenticatedConnection> {
    let login = extract_tailscale_login(headers)?;

    // SECURITY INVARIANT: Never trust `Tailscale-User-Login` unless the TCP
    // peer is strictly loopback (`127.0.0.1` or `::1`) AND neither
    // `X-Forwarded-For` nor `Forwarded` is present.
    let peer = peer_addr?;
    if !is_strict_loopback_peer(peer.ip()) {
        tracing::warn!(
            peer = %peer,
            "Rejected Tailscale-User-Login header from non-loopback peer"
        );
        return None;
    }
    if headers.contains_key("x-forwarded-for") || headers.contains_key("forwarded") {
        tracing::warn!(
            peer = %peer,
            "Rejected Tailscale-User-Login header when X-Forwarded-For/Forwarded is present"
        );
        return None;
    }

    let effective = state
        .tailscale_control
        .effective_config(&state.db)
        .await
        .ok()?;
    if !effective.auth_enabled {
        return None;
    }

    if !crate::service::tailscale_control::verify_tailscale_whois_if_configured(
        &effective.socket_path,
        peer,
        &login,
    )
    .await
    {
        tracing::warn!(
            peer = %peer,
            login = %login,
            "Rejected Tailscale-User-Login header after tailscaled whois cross-check failed"
        );
        return None;
    }

    let user = match_or_provision_tailscale_user(
        &state.db,
        &login,
        &effective.auto_provision_role,
        &peer.ip().to_string(),
    )
    .await?;
    let user_id = user.id.clone();

    Some(AuthenticatedConnection {
        user: CurrentUser::from(user),
        mobile_expires: None,
        credential: ConnectionCredential::Tailscale { user_id },
    })
}

async fn match_or_provision_tailscale_user(
    db: &DatabaseConnection,
    login: &str,
    auto_provision_role: &str,
    audit_ip: &str,
) -> Option<user::Model> {
    let login_trimmed = login.trim();
    if login_trimmed.is_empty() {
        return None;
    }

    // 1. Exact username match.
    if let Some(exact) = user::Entity::find()
        .filter(user::Column::Username.eq(login_trimmed))
        .one(db)
        .await
        .ok()
        .flatten()
    {
        return Some(exact);
    }

    let all_users = user::Entity::find().all(db).await.ok()?;

    // Case-insensitive full-login username match if unique.
    let ci_full: Vec<&user::Model> = all_users
        .iter()
        .filter(|u| u.username.eq_ignore_ascii_case(login_trimmed))
        .collect();
    if ci_full.len() == 1 {
        return Some(ci_full[0].clone());
    }

    // 2. Linked OAuth account match (email case-insensitive or provider_user_id exact) if unique.
    if let Ok(accounts) = oauth_account::Entity::find().all(db).await {
        let matched_user_ids: HashSet<String> = accounts
            .into_iter()
            .filter(|acc| {
                acc.provider_user_id == login_trimmed
                    || acc
                        .email
                        .as_deref()
                        .is_some_and(|e| e.trim().eq_ignore_ascii_case(login_trimmed))
            })
            .map(|acc| acc.user_id)
            .collect();
        if matched_user_ids.len() == 1
            && let Some(uid) = matched_user_ids.into_iter().next()
            && let Some(found) = all_users.iter().find(|u| u.id == uid)
        {
            return Some(found.clone());
        }
    }

    // 3. Unique local-part match when login is an email/domain handle (`local@domain`).
    if let Some((local_part, _domain)) = login_trimmed.split_once('@') {
        let local_part = local_part.trim();
        if !local_part.is_empty() {
            let local_matches: Vec<&user::Model> = all_users
                .iter()
                .filter(|u| {
                    u.username.eq_ignore_ascii_case(local_part)
                        || u.username
                            .split_once('@')
                            .is_some_and(|(ulocal, _)| ulocal.eq_ignore_ascii_case(local_part))
                })
                .collect();
            if local_matches.len() == 1 {
                return Some(local_matches[0].clone());
            }
        }
    }

    // 4. Optional member-only auto-provisioning (NEVER auto-provision `admin`).
    let role = auto_provision_role.trim();
    if role.is_empty() {
        return None;
    }
    if role != "member" {
        tracing::warn!(
            configured_role = %role,
            "Refusing Tailscale auto-provisioning for non-member role"
        );
        return None;
    }

    let provisioned_username = login_trimmed.to_ascii_lowercase();
    let random_password = AuthService::generate_session_token();
    match AuthService::create_user(db, &provisioned_username, &random_password, "member").await {
        Ok(created) => {
            let detail = serde_json::json!({
                "username": created.username,
                "role": "member",
                "tailscale_login": login_trimmed,
            })
            .to_string();
            let _ = AuditService::log(
                db,
                &created.id,
                "tailscale.auto_provision",
                Some(&detail),
                audit_ip,
            )
            .await;
            Some(created)
        }
        Err(AppError::Conflict(_)) => user::Entity::find()
            .filter(user::Column::Username.eq(&provisioned_username))
            .one(db)
            .await
            .ok()
            .flatten()
            .filter(|u| u.role == "member"),
        Err(error) => {
            tracing::error!(
                "Failed to auto-provision Tailscale member '{provisioned_username}': {error}"
            );
            None
        }
    }
}

/// A non-web session has a fixed lifetime (no sliding renewal), so its expiry
/// is a fact the transport must enforce; web sessions yield `None`.
fn mobile_expiry(session: &crate::entity::session::Model) -> Option<chrono::DateTime<chrono::Utc>> {
    (session.source != "web").then_some(session.expires_at)
}

/// Resolve the authenticated user for a WebSocket upgrade without peer address.
pub async fn resolve_ws_connection(
    headers: &HeaderMap,
    state: &AppState,
) -> Option<AuthenticatedConnection> {
    resolve_ws_connection_with_peer(headers, None, state).await
}

/// Resolve the authenticated user for a WebSocket upgrade with optional peer
/// socket address.
///
/// Same credential policy as [`resolve_connection_with_peer`], plus the
/// WS-specific rule: a user flagged `must_change_password` is rejected
/// outright, because the onboarding flow cannot be completed over a WebSocket.
pub async fn resolve_ws_connection_with_peer(
    headers: &HeaderMap,
    peer_addr: Option<SocketAddr>,
    state: &AppState,
) -> Option<AuthenticatedConnection> {
    resolve_connection_with_peer(headers, peer_addr, state)
        .await
        .filter(|conn| !conn.user.must_change_password)
}

/// Resolve the authenticated user (if any) from a request's headers.
///
/// Identity-only view of [`resolve_connection`] for callers that do not care
/// about connection lifetime (HTTP middleware, optional-auth public routes).
#[allow(dead_code)]
pub async fn resolve_optional_user(headers: &HeaderMap, state: &AppState) -> Option<CurrentUser> {
    resolve_connection(headers, state).await.map(|c| c.user)
}

pub async fn resolve_optional_user_with_peer(
    headers: &HeaderMap,
    peer_addr: Option<SocketAddr>,
    state: &AppState,
) -> Option<CurrentUser> {
    resolve_connection_with_peer(headers, peer_addr, state)
        .await
        .map(|c| c.user)
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let peer_addr = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);
    let current_user = resolve_optional_user_with_peer(req.headers(), peer_addr, &state).await;

    match current_user {
        Some(user) => {
            if user.must_change_password
                && !is_onboarding_whitelisted(req.method(), req.uri().path())
            {
                return must_change_password_response();
            }
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// Middleware that requires the authenticated user to have the "admin" role.
/// Must be applied AFTER `auth_middleware`.
pub async fn require_admin(req: Request, next: Next) -> Response {
    let is_admin = req
        .extensions()
        .get::<CurrentUser>()
        .map(|u| u.role == "admin")
        .unwrap_or(false);

    if !is_admin {
        return StatusCode::FORBIDDEN.into_response();
    }

    next.run(req).await
}

/// Extract the `session_token` value from the Cookie header, if present.
pub(crate) fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie.strip_prefix("session_token=").map(|v| v.to_string())
        })
}

/// Extract the raw API key from the `X-API-Key` header, if present.
pub(crate) fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")?
        .to_str()
        .ok()
        .map(|s| s.to_string())
}

/// Extract the bearer token from the Authorization header, if present.
pub(crate) fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(name: &'static str, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(name, HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn test_extract_session_cookie_valid() {
        let headers = headers_with("cookie", "session_token=abc123; other=val");
        assert_eq!(extract_session_cookie(&headers), Some("abc123".to_string()));
    }

    #[test]
    fn test_extract_session_cookie_only() {
        let headers = headers_with("cookie", "session_token=tok42");
        assert_eq!(extract_session_cookie(&headers), Some("tok42".to_string()));
    }

    #[test]
    fn test_extract_session_cookie_missing() {
        let headers = headers_with("cookie", "other=val; foo=bar");
        assert_eq!(extract_session_cookie(&headers), None);
    }

    #[test]
    fn test_extract_session_cookie_no_header() {
        let headers = HeaderMap::new();
        assert_eq!(extract_session_cookie(&headers), None);
    }

    #[test]
    fn test_extract_api_key_valid() {
        let headers = headers_with("x-api-key", "serverbee_abc123def456");
        assert_eq!(
            extract_api_key(&headers),
            Some("serverbee_abc123def456".to_string())
        );
    }

    #[test]
    fn test_extract_api_key_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract_api_key(&headers), None);
    }

    #[test]
    fn test_extract_bearer_token_valid() {
        let headers = headers_with("authorization", "Bearer my_token_123");
        assert_eq!(
            extract_bearer_token(&headers),
            Some("my_token_123".to_string())
        );
    }

    #[test]
    fn test_extract_bearer_token_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract_bearer_token(&headers), None);
    }

    #[test]
    fn test_extract_bearer_token_wrong_scheme() {
        let headers = headers_with("authorization", "Basic dXNlcjpwYXNz");
        assert_eq!(extract_bearer_token(&headers), None);
    }

    #[test]
    fn test_onboarding_whitelist() {
        use axum::http::Method;
        assert!(is_onboarding_whitelisted(&Method::GET, "/auth/me"));
        assert!(is_onboarding_whitelisted(&Method::POST, "/auth/onboarding"));
        assert!(is_onboarding_whitelisted(&Method::POST, "/auth/logout"));
        assert!(!is_onboarding_whitelisted(&Method::POST, "/auth/me"));
        assert!(!is_onboarding_whitelisted(&Method::GET, "/servers"));
        assert!(!is_onboarding_whitelisted(&Method::GET, "/api/auth/me"));
    }

    // ── resolve_connection / resolve_ws_connection (shared credential policy) ──

    mod resolve {
        use super::*;
        use crate::config::AppConfig;
        use crate::entity::{session, user};
        use crate::service::auth::AuthService;
        use crate::test_utils::setup_test_db;
        use chrono::Utc;
        use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

        /// Seed a session row and return its plaintext token.
        async fn seed_session(
            db: &DatabaseConnection,
            user_id: &str,
            source: &str,
            expires_at: chrono::DateTime<Utc>,
        ) -> String {
            let token = AuthService::generate_session_token();
            session::ActiveModel {
                id: Set(uuid::Uuid::new_v4().to_string()),
                user_id: Set(user_id.to_string()),
                token: Set(AuthService::hash_session_token(&token)),
                ip: Set("127.0.0.1".into()),
                user_agent: Set("test".into()),
                expires_at: Set(expires_at),
                created_at: Set(Utc::now()),
                source: Set(source.to_string()),
                mobile_session_id: Set(None),
            }
            .insert(db)
            .await
            .expect("seed session");
            token
        }

        #[tokio::test]
        async fn cookie_takes_precedence_over_bearer() {
            let (db, _tmp) = setup_test_db().await;
            let cookie_user = AuthService::create_user(&db, "cookie-user", "pass1234", "admin")
                .await
                .unwrap();
            let bearer_user = AuthService::create_user(&db, "bearer-user", "pass1234", "member")
                .await
                .unwrap();
            let far = Utc::now() + chrono::Duration::hours(1);
            let cookie_tok = seed_session(&db, &cookie_user.id, "web", far).await;
            let bearer_tok = seed_session(&db, &bearer_user.id, "web", far).await;
            let state = AppState::new(db, AppConfig::default()).await.unwrap();

            let mut headers = HeaderMap::new();
            headers.insert(
                "cookie",
                HeaderValue::from_str(&format!("session_token={cookie_tok}")).unwrap(),
            );
            headers.insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {bearer_tok}")).unwrap(),
            );

            let conn = resolve_connection(&headers, &state).await.unwrap();
            assert_eq!(conn.user.username, "cookie-user");
        }

        #[tokio::test]
        async fn mobile_bearer_session_carries_fixed_expiry() {
            let (db, _tmp) = setup_test_db().await;
            let user = AuthService::create_user(&db, "mob", "pass1234", "member")
                .await
                .unwrap();
            let fixed = Utc::now() + chrono::Duration::hours(24);
            let token = seed_session(&db, &user.id, "mobile", fixed).await;
            let state = AppState::new(db, AppConfig::default()).await.unwrap();

            let headers = headers_with("authorization", &format!("Bearer {token}"));
            let conn = resolve_connection(&headers, &state).await.unwrap();
            assert_eq!(
                conn.mobile_expires.map(|e| e.timestamp()),
                Some(fixed.timestamp()),
                "non-web session must expose its fixed expiry"
            );
        }

        #[tokio::test]
        async fn web_session_has_no_fixed_expiry() {
            let (db, _tmp) = setup_test_db().await;
            let user = AuthService::create_user(&db, "webby", "pass1234", "member")
                .await
                .unwrap();
            let token = seed_session(
                &db,
                &user.id,
                "web",
                Utc::now() + chrono::Duration::hours(1),
            )
            .await;
            let state = AppState::new(db, AppConfig::default()).await.unwrap();

            let headers = headers_with("cookie", &format!("session_token={token}"));
            let conn = resolve_connection(&headers, &state).await.unwrap();
            assert!(conn.mobile_expires.is_none());
        }

        #[tokio::test]
        async fn ws_rejects_must_change_password_user() {
            let (db, _tmp) = setup_test_db().await;
            let user = AuthService::create_user(&db, "flagged", "pass1234", "admin")
                .await
                .unwrap();
            let mut active: user::ActiveModel = user.clone().into();
            active.must_change_password = Set(true);
            active.update(&db).await.unwrap();
            let token = seed_session(
                &db,
                &user.id,
                "web",
                Utc::now() + chrono::Duration::hours(1),
            )
            .await;
            let state = AppState::new(db, AppConfig::default()).await.unwrap();

            let headers = headers_with("cookie", &format!("session_token={token}"));
            // HTTP resolution still authenticates (middleware whitelists the
            // onboarding routes); the WS view rejects outright.
            assert!(resolve_connection(&headers, &state).await.is_some());
            assert!(resolve_ws_connection(&headers, &state).await.is_none());
        }

        #[tokio::test]
        async fn tailscale_serve_auth_enforces_loopback_no_xff_and_auth_enabled() {
            let (db, _tmp) = setup_test_db().await;
            AuthService::create_user(&db, "alice@example.com", "pass1234", "admin")
                .await
                .unwrap();

            // 1. Default config (auth_enabled = false): rejected even from loopback.
            let state_disabled = AppState::new(db.clone(), AppConfig::default())
                .await
                .unwrap();
            let ts_headers = headers_with("tailscale-user-login", "alice@example.com");
            let loopback_v4: SocketAddr = "127.0.0.1:41234".parse().unwrap();
            let loopback_v6: SocketAddr = "[::1]:41234".parse().unwrap();
            let external_peer: SocketAddr = "203.0.113.10:41234".parse().unwrap();
            let cgnat_peer: SocketAddr = "100.64.0.5:41234".parse().unwrap();

            assert!(
                resolve_connection_with_peer(&ts_headers, Some(loopback_v4), &state_disabled)
                    .await
                    .is_none()
            );

            // 2. Enable auth_enabled: loopback v4 and v6 succeed; missing peer or non-loopback fail.
            let mut enabled_cfg = AppConfig::default();
            enabled_cfg.tailscale.auth_enabled = true;
            let state = AppState::new(db, enabled_cfg).await.unwrap();

            let conn_v4 = resolve_connection_with_peer(&ts_headers, Some(loopback_v4), &state)
                .await
                .expect("loopback v4 should authenticate");
            assert_eq!(conn_v4.user.username, "alice@example.com");
            assert_eq!(conn_v4.user.role, "admin");
            assert!(conn_v4.lease_is_valid(&state).await.unwrap());

            assert!(
                resolve_connection_with_peer(&ts_headers, Some(loopback_v6), &state)
                    .await
                    .is_some(),
                "loopback v6 should authenticate"
            );
            assert!(
                resolve_connection_with_peer(&ts_headers, None, &state)
                    .await
                    .is_none(),
                "missing ConnectInfo must fail closed"
            );
            assert!(
                resolve_connection_with_peer(&ts_headers, Some(external_peer), &state)
                    .await
                    .is_none(),
                "public TCP peer must be rejected"
            );
            assert!(
                resolve_connection_with_peer(&ts_headers, Some(cgnat_peer), &state)
                    .await
                    .is_none(),
                "non-loopback CGNAT TCP peer must be rejected"
            );

            // 3. Presence of X-Forwarded-For or Forwarded rejects even on loopback!
            let mut with_xff = ts_headers.clone();
            with_xff.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4"));
            assert!(
                resolve_connection_with_peer(&with_xff, Some(loopback_v4), &state)
                    .await
                    .is_none()
            );

            let mut with_forwarded = ts_headers.clone();
            with_forwarded.insert("forwarded", HeaderValue::from_static("for=1.2.3.4"));
            assert!(
                resolve_connection_with_peer(&with_forwarded, Some(loopback_v4), &state)
                    .await
                    .is_none()
            );
        }
    }
}
