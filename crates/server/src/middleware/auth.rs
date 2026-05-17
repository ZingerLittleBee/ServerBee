use std::sync::Arc;

use axum::Json;
use axum::{
    extract::{Request, State},
    http::StatusCode,
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

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    // Try session cookie
    let current_user = if let Some(token) = extract_session_cookie(&req) {
        AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl)
            .await
            .ok()
            .flatten()
            .map(|(user, _session)| CurrentUser {
                user_id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
                must_change_password: user.must_change_password,
            })
    } else {
        None
    };

    // Try API key header if session not found
    let current_user = match current_user {
        Some(u) => Some(u),
        None => {
            if let Some(key) = extract_api_key(&req) {
                AuthService::validate_api_key(&state.db, &key)
                    .await
                    .ok()
                    .flatten()
                    .map(|user| CurrentUser {
                        user_id: user.id.clone(),
                        username: user.username.clone(),
                        role: user.role.clone(),
                        must_change_password: user.must_change_password,
                    })
            } else {
                None
            }
        }
    };

    // Try Bearer token if still not authenticated
    let current_user = match current_user {
        Some(u) => Some(u),
        None => {
            if let Some(token) = extract_bearer_token(&req) {
                AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl)
                    .await
                    .ok()
                    .flatten()
                    .map(|(user, _session)| CurrentUser {
                        user_id: user.id.clone(),
                        username: user.username.clone(),
                        role: user.role.clone(),
                        must_change_password: user.must_change_password,
                    })
            } else {
                None
            }
        }
    };

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

fn extract_session_cookie(req: &Request) -> Option<String> {
    req.headers()
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie.strip_prefix("session_token=").map(|v| v.to_string())
        })
}

fn extract_api_key(req: &Request) -> Option<String> {
    req.headers()
        .get("x-api-key")?
        .to_str()
        .ok()
        .map(|s| s.to_string())
}

fn extract_bearer_token(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;

    #[test]
    fn test_extract_session_cookie_valid() {
        let req = HttpRequest::builder()
            .header("cookie", "session_token=abc123; other=val")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(&req), Some("abc123".to_string()));
    }

    #[test]
    fn test_extract_session_cookie_only() {
        let req = HttpRequest::builder()
            .header("cookie", "session_token=tok42")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(&req), Some("tok42".to_string()));
    }

    #[test]
    fn test_extract_session_cookie_missing() {
        let req = HttpRequest::builder()
            .header("cookie", "other=val; foo=bar")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(&req), None);
    }

    #[test]
    fn test_extract_session_cookie_no_header() {
        let req = HttpRequest::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(&req), None);
    }

    #[test]
    fn test_extract_api_key_valid() {
        let req = HttpRequest::builder()
            .header("x-api-key", "serverbee_abc123def456")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(
            extract_api_key(&req),
            Some("serverbee_abc123def456".to_string())
        );
    }

    #[test]
    fn test_extract_api_key_missing() {
        let req = HttpRequest::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_api_key(&req), None);
    }

    #[test]
    fn test_extract_bearer_token_valid() {
        let req = HttpRequest::builder()
            .header("authorization", "Bearer my_token_123")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), Some("my_token_123".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_missing() {
        let req = HttpRequest::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), None);
    }

    #[test]
    fn test_extract_bearer_token_wrong_scheme() {
        let req = HttpRequest::builder()
            .header("authorization", "Basic dXNlcjpwYXNz")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), None);
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
