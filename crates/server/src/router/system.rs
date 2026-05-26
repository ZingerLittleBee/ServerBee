/// `/__system/*` reserved routes (spec § 6.4).
///
/// These endpoints are always served by the default SPA routing layer and are
/// never shadowed by a custom theme package. They clear browser cookies used by
/// the theme-precedence logic (spec § 6.5).
use std::sync::Arc;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;

use crate::router::static_files::{FORCE_DEFAULT_COOKIE, PREVIEW_COOKIE};
use crate::state::AppState;

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/clear-recovery", post(clear_recovery))
        .route("/clear-preview", post(clear_preview))
}

/// Build a cookie-clear `Set-Cookie` header value for the given cookie name.
///
/// Cookie names in this module are fixed ASCII identifiers, so `from_str`
/// cannot fail in practice. The defensive log + fallback keeps a hypothetical
/// future regression observable instead of silently returning a header value
/// the browser can't parse.
fn clear_cookie_header(name: &'static str) -> HeaderValue {
    let value = format!("{name}=; Path=/; Max-Age=0; SameSite=Strict");
    HeaderValue::from_str(&value).unwrap_or_else(|e| {
        tracing::warn!(
            error = %e,
            cookie = %name,
            "failed to construct clear Set-Cookie header; sending empty value"
        );
        HeaderValue::from_static("")
    })
}

/// Clear the `sb_force_default` recovery cookie.
///
/// Called by the "Exit recovery" button in the default SPA when an admin is
/// done with the `?theme=default` recovery mode and wants to return to the
/// active custom theme.
async fn clear_recovery() -> Response {
    let mut r = StatusCode::NO_CONTENT.into_response();
    r.headers_mut()
        .append(header::SET_COOKIE, clear_cookie_header(FORCE_DEFAULT_COOKIE));
    r
}

/// Clear the `sb_preview_theme` preview cookie.
///
/// Called by the "Exit preview" button injected into the preview theme HTML.
/// After clearing, the browser reloads and sees the default SPA (or active theme
/// if no recovery cookie is present).
async fn clear_preview() -> Response {
    let mut r = StatusCode::NO_CONTENT.into_response();
    r.headers_mut()
        .append(header::SET_COOKIE, clear_cookie_header(PREVIEW_COOKIE));
    r
}
