use std::sync::Arc;

use axum::Json;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::entity::{api_key, session, user};
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
        };
        Ok(current_user
            .is_some_and(|user| !user.must_change_password && user.role == self.user.role))
    }
}

/// Resolve the authenticated connection (if any) from a request's headers.
///
/// Tries, in order: session cookie, `X-API-Key` header, `Bearer` token. The
/// first credential that validates decides the identity and lifetime; later
/// rungs are only consulted when earlier ones are absent or invalid. Returns
/// `None` when no credential validates.
///
/// This is the single, shared credential policy. HTTP middleware, public
/// routes with optional auth, and every WebSocket handler resolve through it,
/// so precedence, session-source, and expiry semantics never drift between
/// copies.
pub async fn resolve_connection(
    headers: &HeaderMap,
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

    None
}

/// A non-web session has a fixed lifetime (no sliding renewal), so its expiry
/// is a fact the transport must enforce; web sessions yield `None`.
fn mobile_expiry(session: &crate::entity::session::Model) -> Option<chrono::DateTime<chrono::Utc>> {
    (session.source != "web").then_some(session.expires_at)
}

/// Resolve the authenticated user for a WebSocket upgrade.
///
/// Same credential policy as [`resolve_connection`], plus the WS-specific
/// rule: a user flagged `must_change_password` is rejected outright, because
/// the onboarding flow cannot be completed over a WebSocket.
pub async fn resolve_ws_connection(
    headers: &HeaderMap,
    state: &AppState,
) -> Option<AuthenticatedConnection> {
    resolve_connection(headers, state)
        .await
        .filter(|conn| !conn.user.must_change_password)
}

/// Resolve the authenticated user (if any) from a request's headers.
///
/// Identity-only view of [`resolve_connection`] for callers that do not care
/// about connection lifetime (HTTP middleware, optional-auth public routes).
pub async fn resolve_optional_user(headers: &HeaderMap, state: &AppState) -> Option<CurrentUser> {
    resolve_connection(headers, state).await.map(|c| c.user)
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let current_user = resolve_optional_user(req.headers(), &state).await;

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
        assert_eq!(
            extract_session_cookie(&headers),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_session_cookie_only() {
        let headers = headers_with("cookie", "session_token=tok42");
        assert_eq!(
            extract_session_cookie(&headers),
            Some("tok42".to_string())
        );
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
            let token =
                seed_session(&db, &user.id, "web", Utc::now() + chrono::Duration::hours(1)).await;
            let state = AppState::new(db, AppConfig::default()).await.unwrap();

            let headers = headers_with(
                "cookie",
                &format!("session_token={token}"),
            );
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
            let token =
                seed_session(&db, &user.id, "web", Utc::now() + chrono::Duration::hours(1)).await;
            let state = AppState::new(db, AppConfig::default()).await.unwrap();

            let headers = headers_with("cookie", &format!("session_token={token}"));
            // HTTP resolution still authenticates (middleware whitelists the
            // onboarding routes); the WS view rejects outright.
            assert!(resolve_connection(&headers, &state).await.is_some());
            assert!(resolve_ws_connection(&headers, &state).await.is_none());
        }
    }
}
