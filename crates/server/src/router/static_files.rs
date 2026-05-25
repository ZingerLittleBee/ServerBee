use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use rand::rngs::OsRng;
use rust_embed::Embed;
use serde::Deserialize;

use crate::service::spa_theme::LoadedTheme;
use crate::state::AppState;

#[derive(Embed)]
#[folder = "../../apps/web/dist"]
struct Assets;

const FORCE_DEFAULT_COOKIE: &str = "sb_force_default";
const PREVIEW_COOKIE: &str = "sb_preview_theme";
/// Lifetime of the preview cookie in seconds — kept in sync with the
/// `Max-Age` value we set on `sb_preview_theme`. Also drives the banner
/// countdown timer's expiry epoch.
const PREVIEW_MAX_AGE_SECS: u64 = 900;

#[derive(Debug, Deserialize)]
pub struct ThemeQuery {
    theme: Option<String>,
}

/// Decision the handler made about which source to serve.
enum Source {
    Default,
    Active(LoadedTheme),
    Preview { uuid: String, theme: Option<LoadedTheme> },
}

pub async fn theme_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ThemeQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Resolve the authenticated user (if any) — the fallback lives outside the auth
    // middleware layer, so we call resolve_optional_user directly.
    let user = crate::middleware::auth::resolve_optional_user(&headers, &state).await;
    let is_admin = user.as_ref().map(|u| u.role == "admin").unwrap_or(false);

    // Parse cookies inline (no extra deps).
    let cookie_str = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let cookies: Vec<(&str, &str)> = cookie_str
        .split(';')
        .filter_map(|c| {
            let mut it = c.trim().splitn(2, '=');
            Some((it.next()?, it.next().unwrap_or("")))
        })
        .collect();
    let has_force_default = cookies.iter().any(|(k, _)| *k == FORCE_DEFAULT_COOKIE);
    let preview_cookie = cookies
        .iter()
        .find(|(k, _)| *k == PREVIEW_COOKIE)
        .map(|(_, v)| *v);

    // Cookie(s) to set, stored as individual header value strings.
    let mut set_cookies: Vec<String> = Vec::new();

    // Load the active theme once to avoid multiple arc_swap loads.
    // Cloning is cheap: LoadedTheme contains Arc'd Bytes values.
    let active: Option<LoadedTheme> = {
        let guard = state.active_spa_theme.load();
        (**guard).clone()
    };

    // Precedence per spec § 6.5
    let source: Source = match q.theme.as_deref() {
        // 1. ?theme=default → serve default SPA, set recovery cookie
        Some("default") => {
            set_cookies.push(format!(
                "{FORCE_DEFAULT_COOKIE}=1; Path=/; Max-Age=3600; SameSite=Strict"
            ));
            Source::Default
        }

        // 2. ?theme=preview:<uuid> AND admin → preview theme, set preview cookie
        Some(t) if t.starts_with("preview:") && is_admin => {
            let uuid = t.trim_start_matches("preview:").to_string();
            set_cookies.push(format!(
                "{PREVIEW_COOKIE}={uuid}; Path=/; Max-Age={PREVIEW_MAX_AGE_SECS}; SameSite=Strict"
            ));
            Source::Preview { uuid, theme: None }
        }

        // 3. ?theme=active → clear recovery+preview cookies, serve active (or default)
        Some("active") => {
            set_cookies.push(format!(
                "{FORCE_DEFAULT_COOKIE}=; Path=/; Max-Age=0; SameSite=Strict"
            ));
            set_cookies.push(format!(
                "{PREVIEW_COOKIE}=; Path=/; Max-Age=0; SameSite=Strict"
            ));
            if let Some(loaded) = active {
                Source::Active(loaded)
            } else {
                Source::Default
            }
        }

        // 4–7. No query param: cookie / active / default fallthrough
        _ => {
            if has_force_default {
                // 4. Recovery cookie present → serve default
                Source::Default
            } else if let Some(uuid) = preview_cookie.filter(|_| is_admin) {
                // 5. Preview cookie present AND admin → preview that theme
                Source::Preview {
                    uuid: uuid.to_string(),
                    theme: None,
                }
            } else if let Some(loaded) = active {
                // 6. Active theme set → serve it
                Source::Active(loaded)
            } else {
                // 7. Nothing else → default SPA
                Source::Default
            }
        }
    };

    // For Preview sources, load the theme on demand (may differ from active).
    let source = match source {
        Source::Preview { uuid, .. } => {
            let loaded = load_preview_on_demand(&state, &uuid).await;
            Source::Preview { uuid, theme: loaded }
        }
        other => other,
    };

    let resp = match &source {
        Source::Default => serve_default(path),
        Source::Active(theme) => serve_theme(path, theme, false),
        Source::Preview { uuid: _, theme: Some(theme) } => serve_theme(path, theme, true),
        // Preview requested but theme not found — fall back to default.
        Source::Preview { uuid: _, theme: None } => serve_default(path),
    };

    // Append Set-Cookie headers (each as a separate header value per RFC 6265 §3).
    if set_cookies.is_empty() {
        resp
    } else {
        let mut r = resp;
        for cookie in set_cookies {
            if let Ok(v) = HeaderValue::from_str(&cookie) {
                r.headers_mut().append(header::SET_COOKIE, v);
            }
        }
        r
    }
}

/// Load a theme from the DB on demand (used for preview of a non-active theme).
async fn load_preview_on_demand(state: &Arc<AppState>, uuid: &str) -> Option<LoadedTheme> {
    let row = crate::service::spa_theme::SpaThemeService::get(&state.db, uuid)
        .await
        .ok()?;
    crate::service::spa_theme::SpaThemeService::load_row(&row).ok()
}

/// Serve a file from the embedded default SPA (rust-embed).
fn serve_default(path: &str) -> Response {
    if let Some(file) = Assets::get(path) {
        return embedded_file_response(path, &file);
    }
    // SPA history routing fallback
    match Assets::get("index.html") {
        Some(file) => embedded_file_response("index.html", &file),
        None => (StatusCode::NOT_FOUND, "Frontend not embedded").into_response(),
    }
}

fn embedded_file_response(path: &str, file: &rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut builder = Response::builder().header(header::CONTENT_TYPE, mime.as_ref());
    if path.starts_with("assets/") {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    } else {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=60");
    }
    builder
        .body(Body::from(file.data.clone()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Generate a fresh CSP nonce (16 random bytes, URL-safe base64-encoded).
fn fresh_nonce() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Build the CSP header value. When `nonce` is `Some`, adds `'nonce-...'` to
/// `script-src` alongside `'unsafe-inline'` so the injected preview banner
/// script is covered by an explicit allowance in addition to the blanket
/// inline rule (spec § 6.6).
fn csp_header(nonce: Option<&str>) -> String {
    let script_src = match nonce {
        Some(n) => format!("script-src 'self' 'unsafe-inline' 'unsafe-eval' 'nonce-{n}'"),
        None => "script-src 'self' 'unsafe-inline' 'unsafe-eval'".to_string(),
    };
    format!(
        "default-src 'self'; \
         {script_src}; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; \
         font-src 'self' data:; \
         connect-src 'self'; \
         frame-ancestors 'none'; \
         base-uri 'self'; \
         form-action 'self'"
    )
}

/// Serve a file from a custom theme, with CSP headers and optional preview banner.
fn serve_theme(path: &str, theme: &LoadedTheme, inject_banner: bool) -> Response {
    let p = if path.is_empty() { theme.entry.as_str() } else { path };
    let (served_path, bytes) = if let Some(b) = theme.get(p) {
        (p.to_string(), b)
    } else if let Some(b) = theme.entry_html() {
        // SPA history routing fallback
        (theme.entry.clone(), b)
    } else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    let mime = mime_guess::from_path(&served_path).first_or_octet_stream();
    let is_html = mime.essence_str() == "text/html";

    // Banner is only injected for HTML responses in preview mode. The nonce is
    // produced fresh per response and threaded into both the CSP header and the
    // injected `<script>` tag.
    let (body_bytes, nonce) = if is_html && inject_banner {
        let nonce = fresh_nonce();
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            + PREVIEW_MAX_AGE_SECS;
        let injected = inject_preview_banner(&bytes, &nonce, expires_at);
        (injected, Some(nonce))
    } else {
        (bytes, None)
    };

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CONTENT_SECURITY_POLICY, csp_header(nonce.as_deref()))
        // Spec § 5.5 — theme responses use `same-origin` (the global tower-http
        // layer is configured with `if_not_present`, so this value wins).
        .header(header::REFERRER_POLICY, "same-origin");
    if served_path.starts_with("assets/") && !is_html {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    } else {
        builder = builder.header(header::CACHE_CONTROL, "no-cache");
    }
    builder
        .body(Body::from(body_bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Inject a preview-mode banner just before `</body>` in an HTML response.
///
/// The banner shows a countdown timer (`MM:SS`) driven by the `data-expires`
/// attribute on the container div; the inline script ticks once per second
/// and swaps to "Preview expired" on hit zero. Spec § 6.6.
///
/// The `<script>` tag carries the `nonce` attribute so the CSP `'nonce-...'`
/// rule covers it explicitly, in addition to `'unsafe-inline'`.
fn inject_preview_banner(html: &axum::body::Bytes, nonce: &str, expires_at: u64) -> axum::body::Bytes {
    // Pre-built CSS / structure pieces. The runtime parts (expires epoch and
    // nonce) are formatted in below.
    const STYLE: &str = "position:fixed;top:0;left:0;right:0;z-index:2147483647;\
        background:#fde68a;color:#111;padding:8px 12px;font:14px/1.4 sans-serif;\
        text-align:center;box-shadow:0 1px 4px rgba(0,0,0,.2)";
    const BTN_STYLE: &str = "margin-left:8px;padding:4px 10px;border:1px solid #333;\
        background:#fff;cursor:pointer";
    // Inline JS — keeps the banner under 1 KB total (spec § 6.6). Uses
    // `var` / function expressions (no ES6) for broad compatibility.
    const SCRIPT_BODY: &str = "(function(){\
        var d=document.getElementById('__sb_preview');\
        var t=document.getElementById('__sb_timer');\
        var b=document.getElementById('__sb_exit');\
        if(!d||!t||!b)return;\
        var exp=parseInt(d.getAttribute('data-expires'),10)||0;\
        function fmt(s){var m=Math.floor(s/60);var ss=s%60;\
            return m+':'+(ss<10?'0'+ss:ss);}\
        function tick(){\
            var now=Math.floor(Date.now()/1000);\
            var rem=exp-now;\
            if(rem<=0){t.textContent='expired';b.disabled=true;clearInterval(iv);return;}\
            t.textContent='expires in '+fmt(rem);\
        }\
        tick();var iv=setInterval(tick,1000);\
        b.onclick=function(){\
            fetch('/__system/clear-preview',{method:'POST',credentials:'include'})\
                .then(function(){location.reload()});\
        };\
    })();";

    let banner = format!(
        "<div id=\"__sb_preview\" data-expires=\"{expires_at}\" style=\"{STYLE}\">\
            Preview mode &middot; <span id=\"__sb_timer\">expires in --:--</span> &middot; \
            <button id=\"__sb_exit\" style=\"{BTN_STYLE}\">Exit preview</button>\
        </div>\
        <script nonce=\"{nonce}\">{SCRIPT_BODY}</script>"
    );

    let s = std::str::from_utf8(html).unwrap_or("");
    let lower = s.to_ascii_lowercase();
    let injected = match lower.rfind("</body>") {
        Some(i) => format!("{}{}{}", &s[..i], banner, &s[i..]),
        None => format!("{s}{banner}"),
    };
    axum::body::Bytes::from(injected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_banner_before_body_close() {
        let html = axum::body::Bytes::from("<html><body><h1>Hi</h1></body></html>");
        let out = inject_preview_banner(&html, "abc123", 1_700_000_900);
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.contains("__sb_preview"));
        assert!(s.contains("__sb_exit"));
        // Banner comes before </body>
        let banner_pos = s.find("__sb_preview").unwrap();
        let body_close_pos = s.find("</body>").unwrap();
        assert!(banner_pos < body_close_pos);
    }

    #[test]
    fn inject_banner_no_body_tag_appends_at_end() {
        let html = axum::body::Bytes::from("<html><p>No body tag</p></html>");
        let out = inject_preview_banner(&html, "xyz", 0);
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.contains("__sb_preview"));
        // Appended after the original content
        assert!(s.starts_with("<html><p>No body tag</p></html>"));
    }

    #[test]
    fn inject_banner_uses_last_body_close() {
        // Malformed HTML with two </body> tags — inject before the last one.
        let html = axum::body::Bytes::from("<body>A</body><body>B</body>");
        let out = inject_preview_banner(&html, "n", 0);
        let s = std::str::from_utf8(&out).unwrap();
        // rfind picks the last </body>
        assert!(s.ends_with("</body>"));
        let last_body = s.rfind("</body>").unwrap();
        let banner = s.rfind("__sb_preview").unwrap();
        assert!(banner < last_body);
    }

    #[test]
    fn inject_banner_includes_expiry_and_nonce() {
        let html = axum::body::Bytes::from("<body></body>");
        let out = inject_preview_banner(&html, "NONCE-XYZ", 1_700_000_999);
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.contains(r#"data-expires="1700000999""#));
        assert!(s.contains(r#"<script nonce="NONCE-XYZ">"#));
        assert!(s.contains("__sb_timer"));
        // Countdown logic markers
        assert!(s.contains("setInterval"));
        assert!(s.contains("Date.now"));
    }

    #[test]
    fn csp_header_includes_nonce_when_provided() {
        let csp = csp_header(Some("ABC123"));
        assert!(csp.contains("'nonce-ABC123'"));
        assert!(csp.contains("'unsafe-inline'"));
        assert!(csp.contains("'unsafe-eval'"));
    }

    #[test]
    fn csp_header_omits_nonce_when_none() {
        let csp = csp_header(None);
        assert!(!csp.contains("nonce-"));
        assert!(csp.contains("script-src 'self' 'unsafe-inline' 'unsafe-eval'"));
    }

    #[test]
    fn fresh_nonce_is_unique_and_url_safe() {
        let a = fresh_nonce();
        let b = fresh_nonce();
        assert_ne!(a, b);
        // URL-safe base64 alphabet only
        for c in a.chars() {
            assert!(c.is_ascii_alphanumeric() || c == '-' || c == '_');
        }
        // 16 bytes → ceil(16/3 * 4) = 22 chars (no padding)
        assert_eq!(a.len(), 22);
    }
}
