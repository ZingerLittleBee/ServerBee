use std::sync::Arc;

use axum::Json;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

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

/// Resolve the authenticated user (if any) from a request's headers.
///
/// Tries, in order: session cookie, `X-API-Key` header, `Bearer` token.
/// Returns `None` when no credential is present or none validates.
///
/// This is the single, shared credential-parsing routine. Both
/// `auth_middleware` (which enforces auth) and public routes that need
/// optional auth (e.g. the public status page's IP masking) call it, so the
/// security-sensitive parsing logic never drifts between copies.
pub async fn resolve_optional_user(
    headers: &HeaderMap,
    state: &AppState,
) -> Option<CurrentUser> {
    // Try session cookie
    if let Some(token) = extract_session_cookie(headers)
        && let Some((user, _session)) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl)
                .await
                .ok()
                .flatten()
    {
        return Some(CurrentUser::from(user));
    }

    // Try API key header
    if let Some(key) = extract_api_key(headers)
        && let Some(user) = AuthService::validate_api_key(&state.db, &key)
            .await
            .ok()
            .flatten()
    {
        return Some(CurrentUser::from(user));
    }

    // Try Bearer token
    if let Some(token) = extract_bearer_token(headers)
        && let Some((user, _session)) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl)
                .await
                .ok()
                .flatten()
    {
        return Some(CurrentUser::from(user));
    }

    None
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

fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
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

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")?
        .to_str()
        .ok()
        .map(|s| s.to_string())
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
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
}
