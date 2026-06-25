//! Integration coverage for the embedded-SPA static file handler, the public
//! `/api/about` version endpoint, and the admin-only `/api/settings` family
//! (backup happy-path, restore validation arms, update roundtrip, auth gates).
//!
//! These branches are not exercised by the existing `router_content_admin.rs`
//! settings tests, which only cover GET/PUT roundtrip, the >16-byte non-SQLite
//! restore rejection, and the backup/GET auth gates. Here we add:
//!   * static_files::spa_handler — index fallback, asset/runtime/other cache
//!     branches, and unknown-route SPA fallback.
//!   * about::get_about — the public version endpoint (unauthenticated + admin).
//!   * setting::create_backup — admin happy path (VACUUM INTO + audit side
//!     effect), asserting the octet-stream/attachment response is a real SQLite
//!     file, plus the unauthenticated gate.
//!   * setting::restore_backup — the `body.len() < 16` too-small arm (distinct
//!     from the magic-byte arm) and member/unauthenticated gates.
//!   * setting::update_settings — all-null body roundtrip and PUT auth gates.
mod common;

use common::{http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ===========================================================================
// static_files::spa_handler — embedded SPA bundle
// ===========================================================================

/// Helper: fetch a path with a fresh client that does NOT follow redirects, so
/// we observe the handler's own status directly. The SPA handler always returns
/// 200 with a body (no redirects), but disabling redirects keeps assertions
/// honest if that ever changes.
fn no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("build no-redirect client")
}

#[tokio::test]
async fn spa_root_returns_index_html() {
    // GET / has no matching embedded path (path == ""), so the handler falls
    // back to index.html: 200 + text/html content-type.
    let (base_url, _tmp) = start_test_server().await;
    let client = no_redirect_client();

    let resp = client.get(format!("{}/", base_url)).send().await.unwrap();
    assert_eq!(resp.status(), 200, "root should serve the SPA index");
    let ctype = resp
        .headers()
        .get("content-type")
        .expect("content-type header present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        ctype.starts_with("text/html"),
        "index should be served as HTML, got: {ctype}"
    );
    // The else-branch cache policy (not assets/, not runtime/) is a short max-age.
    let cache = resp
        .headers()
        .get("cache-control")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    assert!(
        cache.contains("max-age=60"),
        "index.html should use the short-cache else branch, got: {cache:?}"
    );
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("<!doctype html>") || body.contains("<html"),
        "served body should be the SPA HTML document"
    );
}

#[tokio::test]
async fn spa_unknown_route_falls_back_to_index() {
    // An unknown deep path (no embedded file) must fall back to index.html so
    // TanStack Router can do client-side routing: 200 + HTML.
    let (base_url, _tmp) = start_test_server().await;
    let client = no_redirect_client();

    let resp = client
        .get(format!(
            "{}/this/route/only/exists/in/the/spa/router",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "unknown SPA route should fall back to index, not 404"
    );
    let ctype = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        ctype.starts_with("text/html"),
        "SPA fallback should serve HTML, got: {ctype}"
    );
}

#[tokio::test]
async fn spa_runtime_asset_uses_no_cache_branch() {
    // A known runtime shim file exists in the embedded bundle and exercises the
    // `runtime/` no-cache cache-control branch.
    let (base_url, _tmp) = start_test_server().await;
    let client = no_redirect_client();

    let resp = client
        .get(format!("{}/runtime/react.js", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "runtime shim should be served directly");
    let cache = resp
        .headers()
        .get("cache-control")
        .expect("cache-control present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        cache.contains("no-cache") && cache.contains("no-store"),
        "runtime files must disable caching, got: {cache}"
    );
    // mime_guess maps .js to a javascript content-type.
    let ctype = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        ctype.contains("javascript"),
        "react.js should be served as JavaScript, got: {ctype}"
    );
}

#[tokio::test]
async fn spa_root_known_asset_uses_short_cache_branch() {
    // A root-level non-asset/non-runtime known file (favicon.ico) exercises the
    // else cache branch (short max-age) AND the "embedded file matched" path
    // (distinct from the index fallback).
    let (base_url, _tmp) = start_test_server().await;
    let client = no_redirect_client();

    let resp = client
        .get(format!("{}/favicon.ico", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "favicon should be served from the bundle");
    let cache = resp
        .headers()
        .get("cache-control")
        .expect("cache-control present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        cache.contains("max-age=60"),
        "root non-hashed asset should use the short-cache else branch, got: {cache}"
    );
    // Confirm it is NOT the immutable assets/ policy (proves branch selection).
    assert!(
        !cache.contains("immutable"),
        "favicon must not get the immutable assets/ cache policy"
    );
}

#[tokio::test]
async fn spa_hashed_asset_uses_immutable_cache_branch() {
    // The `assets/` immutable cache branch. Asset filenames are content-hashed
    // and change on rebuild, so we discover a real /assets/* URL by parsing the
    // served index.html instead of hard-coding a hash — keeping the test stable.
    let (base_url, _tmp) = start_test_server().await;
    let client = no_redirect_client();

    let index = client
        .get(format!("{}/", base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // Pull the first "/assets/..." reference out of the HTML (src=/href=).
    let asset_path = extract_first_assets_path(&index)
        .expect("index.html should reference at least one /assets/* file");

    let resp = client
        .get(format!("{}{}", base_url, asset_path))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "hashed asset {asset_path} should be served"
    );
    let cache = resp
        .headers()
        .get("cache-control")
        .expect("cache-control present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        cache.contains("immutable") && cache.contains("max-age=31536000"),
        "assets/ files must use the immutable long-cache branch, got: {cache}"
    );
}

/// Find the first `/assets/...` path referenced in an HTML document.
fn extract_first_assets_path(html: &str) -> Option<String> {
    let needle = "/assets/";
    let start = html.find(needle)?;
    // Walk forward until a character that cannot be part of a URL path.
    let tail = &html[start..];
    let end = tail
        .find(|c: char| c == '"' || c == '\'' || c == ' ' || c == '>' || c == ')')
        .unwrap_or(tail.len());
    Some(tail[..end].to_string())
}

// ===========================================================================
// about::get_about — public version endpoint
// ===========================================================================

#[tokio::test]
async fn about_returns_version_unauthenticated() {
    // GET /api/about is mounted on the public router — no auth required — and
    // returns the crate version string.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/about", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "about endpoint is public");
    let body: Value = resp.json().await.unwrap();
    let version = body["data"]["version"]
        .as_str()
        .expect("version field present");
    assert!(
        !version.is_empty(),
        "version should be the non-empty CARGO_PKG_VERSION"
    );
    // Matches the compiled crate version exactly.
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "about version must equal the server crate version"
    );
}

#[tokio::test]
async fn about_accessible_to_admin_too() {
    // The public route stays reachable when authenticated (no regression where
    // a session somehow blocks a public route).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/about", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["version"].is_string());
}

// ===========================================================================
// setting::create_backup — admin happy path + auth gate
// ===========================================================================

#[tokio::test]
async fn settings_backup_admin_returns_sqlite_file() {
    // Admin POST /api/settings/backup runs VACUUM INTO, streams the resulting
    // file back as an octet-stream attachment, and writes an audit row. We
    // verify the response headers and that the bytes are a real SQLite DB.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/settings/backup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "admin backup should succeed");

    let ctype = resp
        .headers()
        .get("content-type")
        .expect("content-type present")
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        ctype, "application/octet-stream",
        "backup is returned as a binary download"
    );

    let disposition = resp
        .headers()
        .get("content-disposition")
        .expect("content-disposition present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        disposition.contains("attachment") && disposition.contains("serverbee_backup_"),
        "backup must be an attachment with the expected filename prefix, got: {disposition}"
    );

    let bytes = resp.bytes().await.unwrap();
    assert!(
        bytes.len() >= 16,
        "backup file should be a non-trivial SQLite database"
    );
    // SQLite file format magic header.
    assert_eq!(
        &bytes[..16],
        b"SQLite format 3\0",
        "downloaded backup must start with the SQLite magic header"
    );
}

#[tokio::test]
async fn settings_backup_unauthenticated_401() {
    // No auth -> 401 before reaching the admin handler.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/settings/backup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ===========================================================================
// setting::restore_backup — validation arms + auth gates
// ===========================================================================

#[tokio::test]
async fn settings_restore_too_small_body_422() {
    // A body shorter than 16 bytes hits the FIRST validation arm
    // (`body.len() < 16` -> "too small"), distinct from the magic-byte arm
    // already covered by the >16-byte non-SQLite test elsewhere.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/settings/restore", base_url))
        .body(b"short".to_vec()) // 5 bytes < 16
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        422,
        "an undersized restore body must be rejected as Validation"
    );
}

#[tokio::test]
async fn settings_restore_empty_body_422() {
    // An empty body is also < 16 bytes -> the too-small validation arm.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/settings/restore", base_url))
        .body(Vec::<u8>::new())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn settings_restore_bad_magic_16plus_bytes_422() {
    // Exactly the boundary: a >=16-byte body that is NOT a SQLite file passes
    // the length check but fails the magic-byte check -> Validation 422. This
    // pins the SECOND validation arm with a body crafted to be exactly 16 bytes
    // of the wrong magic (boundary case the broad existing test does not pin).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/settings/restore", base_url))
        .body(b"NOT-SQLITE-MAGIC".to_vec()) // exactly 16 bytes, wrong magic
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn settings_restore_member_forbidden() {
    // POST /api/settings/restore is admin-only -> member gets 403 before the
    // body is ever inspected.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "restore_member", "member").await;

    let resp = member
        .post(format!("{}/api/settings/restore", base_url))
        .body(b"this body is long enough to pass the length check".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn settings_restore_unauthenticated_401() {
    // No auth -> 401 on /api/settings/restore.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/settings/restore", base_url))
        .body(b"this body is long enough to pass the length check".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ===========================================================================
// setting::update_settings — body variants + auth gates
// ===========================================================================

#[tokio::test]
async fn settings_update_all_null_roundtrips() {
    // PUT with an all-null/empty object is valid (every field is Option) and is
    // echoed back, persisting the (empty) settings blob.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/settings", base_url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "an empty settings object is valid");
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["site_name"].is_null(),
        "absent fields round-trip as null"
    );

    // Read back: still all null (no stale prior value bleeding through).
    let get = admin
        .get(format!("{}/api/settings", base_url))
        .send()
        .await
        .unwrap();
    let get_body: Value = get.json().await.unwrap();
    assert!(get_body["data"]["custom_js"].is_null());
}

#[tokio::test]
async fn settings_update_custom_js_persists() {
    // Exercise the custom_js field specifically (the existing roundtrip test
    // leaves it null), confirming the full SystemSettings shape serializes.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let put = admin
        .put(format!("{}/api/settings", base_url))
        .json(&json!({
            "site_name": "JS Site",
            "custom_js": "console.log('hi')"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200);

    let get = admin
        .get(format!("{}/api/settings", base_url))
        .send()
        .await
        .unwrap();
    let body: Value = get.json().await.unwrap();
    assert_eq!(body["data"]["custom_js"], "console.log('hi')");
    assert_eq!(body["data"]["site_name"], "JS Site");
}

#[tokio::test]
async fn settings_update_member_forbidden() {
    // PUT /api/settings is admin-only -> member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "settings_put_member", "member").await;

    let resp = member
        .put(format!("{}/api/settings", base_url))
        .json(&json!({ "site_name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn settings_update_unauthenticated_401() {
    // No auth -> 401 on PUT /api/settings.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .put(format!("{}/api/settings", base_url))
        .json(&json!({ "site_name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
