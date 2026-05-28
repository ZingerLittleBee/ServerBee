use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{ConnectInfo, DefaultBodyLimit, Extension, Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post},
};
use futures_util::StreamExt;
use serde::Deserialize;

use crate::{
    error::{ApiResponse, AppError, ok},
    middleware::auth::CurrentUser,
    router::utils::extract_client_ip,
    service::{
        audit::AuditService,
        widget_module::{
            WidgetModuleService,
            service::{InstalledFrom, WidgetModuleListEntry},
        },
    },
    state::AppState,
};

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/widget-modules", get(list_modules))
        .route("/widget-modules/{id}/{*asset_path}", get(serve_asset))
}

pub fn write_router() -> Router<Arc<AppState>> {
    // Apply a per-route body cap so this endpoint cannot be used as a memory
    // amplifier even before our MAX_MODULE_BYTES check runs. We add a small
    // headroom over MAX_MODULE_BYTES for multipart envelope overhead.
    Router::new()
        .route("/widget-modules", post(install_widget_module))
        .route("/widget-modules/{id}", delete(uninstall_module))
        .layer(DefaultBodyLimit::max(MAX_MODULE_BYTES + 65_536))
}

#[utoipa::path(
    get,
    path = "/api/widget-modules",
    tag = "widget-modules",
    responses(
        (status = 200, description = "List installed widget modules", body = Vec<WidgetModuleListEntry>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_modules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<WidgetModuleListEntry>>>, AppError> {
    let modules = WidgetModuleService::list(&state.db).await?;
    ok(modules)
}

#[utoipa::path(
    get,
    path = "/api/widget-modules/{id}/{asset_path}",
    tag = "widget-modules",
    params(
        ("id" = String, Path, description = "Module ID"),
        ("asset_path" = String, Path, description = "Asset path within the package"),
    ),
    responses(
        (status = 200, description = "Asset bytes"),
        (status = 404, description = "Module or asset not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn serve_asset(
    State(state): State<Arc<AppState>>,
    Path((id, asset_path)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let served = WidgetModuleService::serve_asset(&state.db, &id, &asset_path).await?;
    let etag_suffix_len = 8.min(served.code_sha256.len());
    let etag = format!(
        "\"{}-{}\"",
        served.version,
        &served.code_sha256[..etag_suffix_len]
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&served.mime).map_err(|e| AppError::Internal(e.to_string()))?,
    );
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&etag).map_err(|e| AppError::Internal(e.to_string()))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400, immutable"),
    );
    Ok((StatusCode::OK, headers, served.bytes))
}

#[derive(Debug, Deserialize)]
pub struct InstallQuery {
    pub url: Option<String>,
}

/// Max accepted module size (1 MiB) — applies to both URL fetch and multipart upload.
const MAX_MODULE_BYTES: usize = 1_048_576;

/// SSRF guard: rejects an IP that falls into any reserved / private / loopback /
/// link-local / multicast / documentation range. Cloud metadata (169.254.169.254),
/// CGNAT (100.64.0.0/10), and IPv6 ULA/link-local/v4-mapped are explicitly covered.
/// Returns `true` only for addresses that we consider safe to dial from the server.
pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_ipv4(v4),
        IpAddr::V6(v6) => is_public_ipv6(v6),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    if ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
    {
        return false;
    }
    let [a, b, c, _] = ip.octets();
    // 100.64.0.0/10 CGNAT
    if a == 100 && (b & 0b1100_0000) == 0b0100_0000 {
        return false;
    }
    // 192.0.0.0/24 IETF (and 192.0.2.0/24 documentation already filtered by is_documentation)
    if a == 192 && b == 0 && c == 0 {
        return false;
    }
    // 192.88.99.0/24 (6to4 anycast, deprecated)
    if a == 192 && b == 88 && c == 99 {
        return false;
    }
    // 198.18.0.0/15 benchmarking
    if a == 198 && (b == 18 || b == 19) {
        return false;
    }
    // 240.0.0.0/4 reserved for future use (covers 255.255.255.255 too)
    if a >= 240 {
        return false;
    }
    true
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return false;
    }
    let segments = ip.segments();
    // fc00::/7 unique-local
    if (segments[0] & 0xfe00) == 0xfc00 {
        return false;
    }
    // fe80::/10 link-local
    if (segments[0] & 0xffc0) == 0xfe80 {
        return false;
    }
    // ::ffff:0:0/96 IPv4-mapped — check octets 0..=9 are zero and 10..=11 are 0xff
    let octets = ip.octets();
    if octets[..10] == [0u8; 10] && octets[10] == 0xff && octets[11] == 0xff {
        // Recurse on the embedded v4 address.
        let v4 = Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15]);
        return is_public_ipv4(v4);
    }
    // 2001:db8::/32 documentation
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return false;
    }
    // 100::/64 discard prefix
    if segments[0] == 0x0100 && segments[1] == 0 && segments[2] == 0 && segments[3] == 0 {
        return false;
    }
    true
}

/// Resolves the URL's host to its IP addresses and rejects if ANY resolved
/// address lives in a reserved range. We bias toward false-negatives (refuse
/// the fetch) because the cost of a wrong allow is "talked to internal box";
/// the cost of a wrong reject is "user must self-host the module".
async fn enforce_url_safety(url: &str) -> Result<(), AppError> {
    let parsed =
        url::Url::parse(url).map_err(|e| AppError::BadRequest(format!("bad url: {e}")))?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::BadRequest("url must be http(s)".into()));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::BadRequest("url missing host".into()))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| AppError::BadRequest("url missing port".into()))?;

    // If the host is a literal IP, check it directly without DNS.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err(AppError::BadRequest(
                "private/loopback/reserved address rejected".into(),
            ));
        }
        return Ok(());
    }

    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| AppError::BadRequest(format!("dns lookup failed: {e}")))?;
    let mut any = false;
    for sa in addrs {
        any = true;
        if !is_public_ip(sa.ip()) {
            return Err(AppError::BadRequest(
                "host resolves to private/reserved address".into(),
            ));
        }
    }
    if !any {
        return Err(AppError::BadRequest(
            "host resolved to no addresses".into(),
        ));
    }
    Ok(())
}

#[utoipa::path(
    post,
    path = "/api/widget-modules",
    tag = "widget-modules",
    params(
        ("url" = Option<String>, Query, description = "HTTPS URL to fetch the widget bundle from. Accepts either a single `.js` file or a `.zip` collection bundle."),
    ),
    request_body(
        content_type = "multipart/form-data",
        description = "Alternatively, upload the widget bundle in a `file` field. Accepts either a single `.js` file or a `.zip` collection bundle.",
    ),
    responses(
        (
            status = 200,
            description = "Installed (or upgraded) widget module(s). For a single `.js` file the response is `{ data: { id, version } }`. For a `.zip` collection it is `{ data: [{ id, version }, ...] }` — one entry per widget in the collection.",
        ),
        (status = 400, description = "Bad URL, unsupported source, or invalid manifest"),
        (status = 409, description = "Module id conflicts with an existing install of a different source type"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn install_widget_module(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(user): Extension<CurrentUser>,
    Query(q): Query<InstallQuery>,
    headers: HeaderMap,
    multipart: Option<Multipart>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let user_id = user.user_id.parse::<i64>().ok();
    let client_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    let (bytes, from) = if let Some(url) = q.url {
        enforce_url_safety(&url).await?;

        // Disable redirects: a 3xx Location could re-enter a private address
        // after the initial public-IP check passed.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| AppError::Internal(format!("http client: {e}")))?;
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::BadRequest(format!("fetch: {e}")))?;
        if resp.status().is_redirection() {
            return Err(AppError::BadRequest("redirects not allowed".into()));
        }
        if !resp.status().is_success() {
            return Err(AppError::BadRequest(format!(
                "fetch {}: {}",
                url,
                resp.status()
            )));
        }

        // Stream the body with a running total so we abort early on oversize
        // responses instead of buffering an unbounded payload.
        let mut total = 0usize;
        let mut buf = Vec::with_capacity(64 * 1024);
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| AppError::Internal(format!("read body: {e}")))?;
            total += chunk.len();
            if total > MAX_MODULE_BYTES {
                return Err(AppError::BadRequest("module too large (>1MB)".into()));
            }
            buf.extend_from_slice(&chunk);
        }
        (buf, InstalledFrom::Url(url))
    } else if let Some(mut mp) = multipart {
        let mut bytes_opt: Option<Vec<u8>> = None;
        let mut name_opt: Option<String> = None;
        while let Some(field) = mp
            .next_field()
            .await
            .map_err(|e| AppError::BadRequest(format!("multipart: {e}")))?
        {
            if field.name() == Some("file") {
                name_opt = field.file_name().map(|s| s.to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("multipart body: {e}")))?;
                if data.len() > MAX_MODULE_BYTES {
                    return Err(AppError::BadRequest("module too large (>1MB)".into()));
                }
                bytes_opt = Some(data.to_vec());
                break;
            }
        }
        let bytes =
            bytes_opt.ok_or_else(|| AppError::BadRequest("missing 'file' part".into()))?;
        (
            bytes,
            InstalledFrom::Upload(name_opt.unwrap_or_else(|| "upload.js".into())),
        )
    } else {
        return Err(AppError::BadRequest(
            "provide ?url=... or multipart file".into(),
        ));
    };

    if bytes.starts_with(b"PK\x03\x04") {
        let rows = WidgetModuleService::install_collection_from_zip(
            &state.db, bytes, from, user_id,
        )
        .await?;
        let payload: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| serde_json::json!({ "id": r.id, "version": r.version }))
            .collect();
        // Best-effort audit log per installed widget — failures here must
        // never poison the install response.
        for row in rows.iter() {
            let detail = serde_json::json!({
                "id": row.id,
                "version": row.version,
                "source_type": format!("{:?}", row.source_type),
                "source_url": row.source_url,
                "code_sha256": row.code_sha256,
            })
            .to_string();
            let _ = AuditService::log(
                &state.db,
                &user.user_id,
                "widget_module.install",
                Some(&detail),
                &client_ip,
            )
            .await;
        }
        ok(serde_json::Value::Array(payload))
    } else {
        let row =
            WidgetModuleService::install_single_file(&state.db, bytes, from, user_id).await?;
        let detail = serde_json::json!({
            "id": row.id,
            "version": row.version,
            "source_type": format!("{:?}", row.source_type),
            "source_url": row.source_url,
            "code_sha256": row.code_sha256,
        })
        .to_string();
        let _ = AuditService::log(
            &state.db,
            &user.user_id,
            "widget_module.install",
            Some(&detail),
            &client_ip,
        )
        .await;
        ok(serde_json::json!({ "id": row.id, "version": row.version }))
    }
}

#[utoipa::path(
    delete,
    path = "/api/widget-modules/{id}",
    tag = "widget-modules",
    params(("id" = String, Path, description = "Module ID")),
    responses(
        (status = 204, description = "Module uninstalled"),
        (status = 400, description = "Cannot uninstall builtin module"),
        (status = 404, description = "Module not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn uninstall_module(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let client_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    WidgetModuleService::uninstall(&state.db, &id).await?;
    let detail = serde_json::json!({ "id": id }).to_string();
    let _ = AuditService::log(
        &state.db,
        &user.user_id,
        "widget_module.uninstall",
        Some(&detail),
        &client_ip,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_loopback_and_private_rejected() {
        assert!(!is_public_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("10.1.2.3".parse().unwrap()));
        assert!(!is_public_ip("172.16.0.1".parse().unwrap()));
        assert!(!is_public_ip("192.168.1.1".parse().unwrap()));
        assert!(!is_public_ip("169.254.169.254".parse().unwrap()));
        assert!(!is_public_ip("100.64.0.1".parse().unwrap()));
        assert!(!is_public_ip("224.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("240.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("255.255.255.255".parse().unwrap()));
        assert!(!is_public_ip("198.18.0.1".parse().unwrap()));
        assert!(!is_public_ip("198.51.100.1".parse().unwrap()));
        assert!(!is_public_ip("203.0.113.1".parse().unwrap()));
        assert!(!is_public_ip("192.0.2.1".parse().unwrap()));
        assert!(!is_public_ip("0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn ipv4_public_accepted() {
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("1.1.1.1".parse().unwrap()));
        assert!(is_public_ip("203.0.114.1".parse().unwrap()));
    }

    #[test]
    fn ipv6_reserved_rejected() {
        assert!(!is_public_ip("::1".parse().unwrap()));
        assert!(!is_public_ip("::".parse().unwrap()));
        assert!(!is_public_ip("fc00::1".parse().unwrap()));
        assert!(!is_public_ip("fd12:3456::1".parse().unwrap()));
        assert!(!is_public_ip("fe80::1".parse().unwrap()));
        assert!(!is_public_ip("ff02::1".parse().unwrap())); // multicast
        assert!(!is_public_ip("2001:db8::1".parse().unwrap()));
        assert!(!is_public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("::ffff:169.254.169.254".parse().unwrap()));
        assert!(!is_public_ip("0100::1".parse().unwrap()));
    }

    #[test]
    fn ipv6_public_accepted() {
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
        assert!(is_public_ip("::ffff:8.8.8.8".parse().unwrap()));
    }
}
