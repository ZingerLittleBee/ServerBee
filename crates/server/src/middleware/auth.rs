use std::sync::Arc;

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
            .map(|user| CurrentUser {
                user_id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
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
                    })
            } else {
                None
            }
        }
    };

    match current_user {
        Some(user) => {
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
            .header("x-api-key", "sb_abc123def456")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_api_key(&req), Some("sb_abc123def456".to_string()));
    }

    #[test]
    fn test_extract_api_key_missing() {
        let req = HttpRequest::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(extract_api_key(&req), None);
    }
}
