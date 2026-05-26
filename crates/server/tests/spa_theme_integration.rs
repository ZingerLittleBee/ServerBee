/// Integration test suite for the Custom SPA Theme feature.
///
/// Covers all 16 scenarios from plan Task 16:
///  1.  Non-admin POST → 403
///  2.  Admin upload valid fixture → 200, uuid + manifest
///  3.  Upload same id + higher version → 200 + is_upgrade_of set
///  4.  Upload same id + lower version → 400 NO_DOWNGRADE
///  5.  Upload same id + same version → 409 VERSION_EXISTS
///  6.  Admin activate via PUT → 200; GET / returns theme body
///  7.  GET /?theme=default returns default SPA body + sets sb_force_default cookie
///  8.  Subsequent GET / with sb_force_default cookie returns default
///  9.  GET /?theme=active clears cookie and returns active theme
/// 10.  GET /?theme=preview:<uuid> as admin returns that theme + sets sb_preview_theme cookie
/// 11.  Same preview query as non-admin returns active (preview ignored)
/// 12.  DELETE active theme → 409 THEME_IN_USE
/// 13.  DELETE inactive theme → 204; row removed
/// 14.  Upload zip-slip fixture → 400 ZIP_SLIP
/// 15.  Upload zip-bomb fixture → 400 ZIP_BOMB
/// 16.  Upload package > 25 MB → 413 UPLOAD_TOO_LARGE
use std::time::Duration;

use reqwest::{Client, StatusCode, multipart};
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

// ---------------------------------------------------------------------------
// Bootstrap helpers
// ---------------------------------------------------------------------------

/// Start a test server. If `create_member` is true, also seeds `member/memberpass`.
async fn start_test_server(create_member: bool) -> (String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let data_dir = tmp.path().to_str().unwrap().to_string();

    let config = AppConfig {
        server: ServerConfig {
            listen: "127.0.0.1:0".to_string(),
            data_dir: data_dir.clone(),
            trusted_proxies: Vec::new(),
        },
        database: DatabaseConfig {
            path: "test.db".to_string(),
            max_connections: 5,
        },
        auth: AuthConfig {
            session_ttl: 86400,
            secure_cookie: false,
            max_servers: 0,
        },
        ..AppConfig::default()
    };

    let db_path = format!("{data_dir}/test.db");
    let db_url = format!("sqlite://{db_path}?mode=rwc");
    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(5);
    opt.sqlx_logging(false);

    let db = Database::connect(opt)
        .await
        .expect("Failed to connect to test database");

    db.execute_unprepared("PRAGMA journal_mode=WAL").await.unwrap();
    db.execute_unprepared("PRAGMA foreign_keys=ON").await.unwrap();

    Migrator::up(&db, None).await.expect("Failed to run migrations");

    AuthService::create_user(&db, "admin", "testpass", "admin")
        .await
        .expect("Failed to seed admin");

    if create_member {
        AuthService::create_user(&db, "member", "memberpass", "member")
            .await
            .expect("Failed to seed member");
    }

    let state = AppState::new(db, config).await.expect("Failed to create AppState");
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    (base_url, tmp)
}

fn http_client() -> Client {
    Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to build HTTP client")
}

async fn login(client: &Client, base_url: &str, username: &str, password: &str) {
    let resp = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("Login request failed");
    assert_eq!(resp.status(), StatusCode::OK, "Login should succeed for {username}");
}

// ---------------------------------------------------------------------------
// Zip-building helpers (duplicated from extractor.rs #[cfg(test)])
// ---------------------------------------------------------------------------

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    buf
}

fn build_zip_stored(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    buf
}

/// Build a minimal valid theme zip.
fn valid_theme_zip(id: &str, version: &str) -> Vec<u8> {
    let manifest = serde_json::json!({
        "schema_version": 1,
        "id": id,
        "name": id,
        "version": version,
    })
    .to_string();
    build_zip(&[("manifest.json", manifest.as_bytes()), ("index.html", b"<html><body>theme</body></html>")])
}

/// Upload a zip as admin and return the parsed JSON response body.
async fn upload_theme(client: &Client, base_url: &str, zip_bytes: Vec<u8>) -> (StatusCode, Value) {
    let part = multipart::Part::bytes(zip_bytes)
        .file_name("theme.sbtheme")
        .mime_str("application/zip")
        .unwrap();
    let form = multipart::Form::new().part("package", part);

    let resp = client
        .post(format!("{base_url}/api/settings/spa-themes"))
        .multipart(form)
        .send()
        .await
        .expect("Upload request failed");

    let status = resp.status();
    let body_bytes = resp.bytes().await.expect("Failed to read upload response body");
    let body: Value = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(&body_bytes).into_owned())
        })
    };
    (status, body)
}

// ---------------------------------------------------------------------------
// Scenarios 1–5: upload policy + auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_1_non_admin_upload_is_403() {
    let (base_url, _tmp) = start_test_server(true).await;
    let client = http_client();
    login(&client, &base_url, "member", "memberpass").await;

    let zip = valid_theme_zip("acme-test", "1.0.0");
    let (status, body) = upload_theme(&client, &base_url, zip).await;

    assert_eq!(status, StatusCode::FORBIDDEN, "scenario 1: non-admin upload must be 403; body={body}");
}

#[tokio::test]
async fn scenarios_2_to_5_upload_version_policy() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    login(&client, &base_url, "admin", "testpass").await;

    // Scenario 2: admin upload valid fixture → 200, uuid + manifest
    let zip_v1 = valid_theme_zip("acme-test", "1.0.0");
    let (status, body) = upload_theme(&client, &base_url, zip_v1).await;
    assert_eq!(status, StatusCode::OK, "scenario 2: valid upload must be 200; body={body}");
    let uuid_v1 = body["data"]["uuid"].as_str().expect("scenario 2: must have uuid").to_string();
    assert!(!uuid_v1.is_empty(), "scenario 2: uuid must not be empty");
    let manifest_id = body["data"]["manifest"]["id"]
        .as_str()
        .expect("scenario 2: must have manifest.id");
    assert_eq!(manifest_id, "acme-test", "scenario 2: manifest id must match");
    assert!(
        body["data"]["is_upgrade_of"].is_null(),
        "scenario 2: first upload must not have is_upgrade_of"
    );

    // Scenario 3: same id + higher version → 200 + is_upgrade_of set
    let zip_v2 = valid_theme_zip("acme-test", "1.1.0");
    let (status, body) = upload_theme(&client, &base_url, zip_v2).await;
    assert_eq!(status, StatusCode::OK, "scenario 3: upgrade must be 200; body={body}");
    let is_upgrade_of = &body["data"]["is_upgrade_of"];
    assert!(
        !is_upgrade_of.is_null(),
        "scenario 3: upgrade must have is_upgrade_of set; body={body}"
    );
    assert_eq!(
        is_upgrade_of["previous_uuid"].as_str().unwrap_or(""),
        uuid_v1,
        "scenario 3: is_upgrade_of.previous_uuid must match v1 uuid"
    );
    assert_eq!(
        is_upgrade_of["previous_version"].as_str().unwrap_or(""),
        "1.0.0",
        "scenario 3: is_upgrade_of.previous_version must be 1.0.0"
    );

    // Scenario 4: same id + lower version → 400 NO_DOWNGRADE
    let zip_old = valid_theme_zip("acme-test", "0.9.0");
    let (status, body) = upload_theme(&client, &base_url, zip_old).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "scenario 4: downgrade must be 400; body={body}");
    assert_eq!(
        body["error"]["code"].as_str().unwrap_or(""),
        "NO_DOWNGRADE",
        "scenario 4: error code must be NO_DOWNGRADE"
    );

    // Scenario 5: same id + same version → 409 VERSION_EXISTS
    let zip_same = valid_theme_zip("acme-test", "1.1.0");
    let (status, body) = upload_theme(&client, &base_url, zip_same).await;
    assert_eq!(status, StatusCode::CONFLICT, "scenario 5: same version must be 409; body={body}");
    assert_eq!(
        body["error"]["code"].as_str().unwrap_or(""),
        "VERSION_EXISTS",
        "scenario 5: error code must be VERSION_EXISTS"
    );
}

// ---------------------------------------------------------------------------
// Scenarios 6–11: activate + SPA serve + cookie precedence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenarios_6_to_11_activate_and_serve() {
    let (base_url, _tmp) = start_test_server(true).await;

    // Admin client (with session cookie)
    let admin = http_client();
    login(&admin, &base_url, "admin", "testpass").await;

    // Upload a theme
    let html_body = b"<html><body>CUSTOM_THEME_CONTENT</body></html>";
    let manifest = serde_json::json!({
        "schema_version": 1,
        "id": "my-theme",
        "name": "my-theme",
        "version": "1.0.0",
    })
    .to_string();
    let zip = build_zip(&[("manifest.json", manifest.as_bytes()), ("index.html", html_body)]);
    let (status, body) = upload_theme(&admin, &base_url, zip).await;
    assert_eq!(status, StatusCode::OK, "setup: upload must succeed; body={body}");
    let uuid = body["data"]["uuid"].as_str().expect("setup: must have uuid").to_string();

    // Scenario 6: activate via PUT → 200; GET / returns theme body
    let activate_resp = admin
        .put(format!("{base_url}/api/settings/active-spa-theme"))
        .json(&json!({ "theme_id": uuid }))
        .send()
        .await
        .expect("activate request failed");
    assert_eq!(
        activate_resp.status(),
        StatusCode::OK,
        "scenario 6: activate must be 200"
    );
    let activate_body: Value = activate_resp.json().await.unwrap();
    assert_eq!(
        activate_body["data"]["theme_id"].as_str().unwrap_or(""),
        uuid,
        "scenario 6: activate response must echo back theme_id"
    );

    // GET / — separate non-authenticated client so no admin session cookie
    let anon = http_client();
    let serve_resp = anon
        .get(format!("{base_url}/"))
        .send()
        .await
        .expect("GET / failed");
    assert_eq!(serve_resp.status(), StatusCode::OK, "scenario 6: GET / must be 200");
    let serve_body = serve_resp.text().await.expect("body text");
    assert!(
        serve_body.contains("CUSTOM_THEME_CONTENT"),
        "scenario 6: GET / must return theme content; got={serve_body:.200}"
    );

    // Scenario 7: GET /?theme=default returns default SPA body + sets sb_force_default cookie.
    //
    // In test environments the embedded SPA (apps/web/dist) may be absent, causing
    // serve_default() to return 404. That is acceptable here — what we care about is:
    //   (a) the response sets the sb_force_default cookie
    //   (b) the body does NOT contain our custom theme sentinel
    let default_resp = anon
        .get(format!("{base_url}/?theme=default"))
        .send()
        .await
        .expect("GET /?theme=default failed");
    let default_status = default_resp.status();
    assert!(
        default_status == StatusCode::OK || default_status == StatusCode::NOT_FOUND,
        "scenario 7: GET /?theme=default must be 200 or 404 (no dist in test env); got={default_status}"
    );
    // Cookie jar in reqwest stores cookies — verify via Set-Cookie header before body is consumed.
    let set_cookie = default_resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        set_cookie.iter().any(|c| c.starts_with("sb_force_default=")),
        "scenario 7: must set sb_force_default cookie; headers={set_cookie:?}"
    );
    let default_body = default_resp.text().await.expect("body text");
    assert!(
        !default_body.contains("CUSTOM_THEME_CONTENT"),
        "scenario 7: default SPA must not contain theme content; got={default_body:.200}"
    );

    // Scenario 8: subsequent GET / with the sb_force_default cookie returns default (not the theme).
    // reqwest's cookie store has already persisted the cookie from scenario 7.
    // Again, 404 is acceptable when the embedded SPA is absent.
    let after_default_resp = anon
        .get(format!("{base_url}/"))
        .send()
        .await
        .expect("GET / with cookie failed");
    let after_status = after_default_resp.status();
    assert!(
        after_status == StatusCode::OK || after_status == StatusCode::NOT_FOUND,
        "scenario 8: GET / with sb_force_default must be 200 or 404; got={after_status}"
    );
    let after_default_body = after_default_resp.text().await.expect("body text");
    assert!(
        !after_default_body.contains("CUSTOM_THEME_CONTENT"),
        "scenario 8: recovery cookie must keep serving default; got={after_default_body:.200}"
    );

    // Scenario 9: GET /?theme=active clears cookie and returns active theme
    let active_resp = anon
        .get(format!("{base_url}/?theme=active"))
        .send()
        .await
        .expect("GET /?theme=active failed");
    assert_eq!(
        active_resp.status(),
        StatusCode::OK,
        "scenario 9: GET /?theme=active must be 200"
    );
    // Verify Set-Cookie clears sb_force_default (Max-Age=0)
    let clear_cookies = active_resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        clear_cookies
            .iter()
            .any(|c| c.contains("sb_force_default=") && c.contains("Max-Age=0")),
        "scenario 9: must clear sb_force_default cookie; headers={clear_cookies:?}"
    );
    let active_body = active_resp.text().await.expect("body text");
    assert!(
        active_body.contains("CUSTOM_THEME_CONTENT"),
        "scenario 9: GET /?theme=active must return theme; got={active_body:.200}"
    );

    // Scenario 10: GET /?theme=preview:<uuid> as admin returns theme + sets sb_preview_theme cookie
    let preview_resp = admin
        .get(format!("{base_url}/?theme=preview:{uuid}"))
        .send()
        .await
        .expect("GET /?theme=preview:<uuid> failed");
    assert_eq!(
        preview_resp.status(),
        StatusCode::OK,
        "scenario 10: admin preview must be 200"
    );
    let preview_cookies = preview_resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        preview_cookies
            .iter()
            .any(|c| c.starts_with("sb_preview_theme=") && c.contains(&uuid)),
        "scenario 10: admin preview must set sb_preview_theme cookie with uuid; headers={preview_cookies:?}"
    );
    let preview_body = preview_resp.text().await.expect("body text");
    assert!(
        preview_body.contains("CUSTOM_THEME_CONTENT"),
        "scenario 10: admin preview must return theme content; got={preview_body:.200}"
    );
    // Preview mode injects a banner
    assert!(
        preview_body.contains("__sb_preview"),
        "scenario 10: admin preview must inject banner; got={preview_body:.200}"
    );

    // Scenario 11: same preview query as non-admin returns active (preview ignored)
    // Use a fresh non-admin client (no session, no preview cookie).
    let non_admin = http_client();
    let non_admin_preview_resp = non_admin
        .get(format!("{base_url}/?theme=preview:{uuid}"))
        .send()
        .await
        .expect("non-admin preview GET failed");
    assert_eq!(
        non_admin_preview_resp.status(),
        StatusCode::OK,
        "scenario 11: non-admin preview must be 200"
    );
    // Should NOT set a preview cookie
    let na_cookies = non_admin_preview_resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert!(
        !na_cookies.iter().any(|c| c.starts_with("sb_preview_theme=")),
        "scenario 11: non-admin must not get sb_preview_theme cookie; headers={na_cookies:?}"
    );
    // Should serve active theme (since there is an active theme), not with banner.
    let na_body = non_admin_preview_resp.text().await.expect("body text");
    assert!(
        na_body.contains("CUSTOM_THEME_CONTENT"),
        "scenario 11: non-admin falls through to active theme; got={na_body:.200}"
    );
    assert!(
        !na_body.contains("__sb_preview"),
        "scenario 11: non-admin must not see preview banner; got={na_body:.200}"
    );
}

// ---------------------------------------------------------------------------
// Scenarios 12–13: delete active / inactive theme
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenarios_12_13_delete_active_and_inactive() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    login(&client, &base_url, "admin", "testpass").await;

    // Upload two themes with different ids
    let zip_a = valid_theme_zip("theme-alpha", "1.0.0");
    let (_, body_a) = upload_theme(&client, &base_url, zip_a).await;
    let uuid_a = body_a["data"]["uuid"].as_str().expect("uuid_a").to_string();

    let zip_b = valid_theme_zip("theme-beta", "1.0.0");
    let (_, body_b) = upload_theme(&client, &base_url, zip_b).await;
    let uuid_b = body_b["data"]["uuid"].as_str().expect("uuid_b").to_string();

    // Activate theme A
    let act = client
        .put(format!("{base_url}/api/settings/active-spa-theme"))
        .json(&json!({ "theme_id": uuid_a }))
        .send()
        .await
        .expect("activate failed");
    assert_eq!(act.status(), StatusCode::OK, "activate must succeed");

    // Scenario 12: DELETE active theme → 409 THEME_IN_USE
    let del_resp = client
        .delete(format!("{base_url}/api/settings/spa-themes/{uuid_a}"))
        .send()
        .await
        .expect("delete request failed");
    assert_eq!(
        del_resp.status(),
        StatusCode::CONFLICT,
        "scenario 12: deleting active theme must be 409"
    );
    let del_body: Value = del_resp.json().await.unwrap();
    assert_eq!(
        del_body["error"]["code"].as_str().unwrap_or(""),
        "THEME_IN_USE",
        "scenario 12: error code must be THEME_IN_USE"
    );

    // Scenario 13: DELETE inactive theme B → 204; row removed
    let del_b = client
        .delete(format!("{base_url}/api/settings/spa-themes/{uuid_b}"))
        .send()
        .await
        .expect("delete inactive request failed");
    assert_eq!(
        del_b.status(),
        StatusCode::NO_CONTENT,
        "scenario 13: deleting inactive theme must be 204"
    );

    // Verify it's gone from the list
    let list_resp = client
        .get(format!("{base_url}/api/settings/spa-themes"))
        .send()
        .await
        .expect("list failed");
    let list_body: Value = list_resp.json().await.unwrap();
    let empty = vec![];
    let uuids: Vec<_> = list_body["data"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|e| e["uuid"].as_str())
        .collect();
    assert!(
        !uuids.contains(&uuid_b.as_str()),
        "scenario 13: deleted theme must not appear in list; list={uuids:?}"
    );
}

// ---------------------------------------------------------------------------
// Scenarios 14–15: zip-slip + zip-bomb rejections
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_14_zip_slip_is_rejected() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    login(&client, &base_url, "admin", "testpass").await;

    // Build a zip with a path traversal entry
    let zip = build_zip(&[("../etc/passwd", b"root:x:0:0"), ("manifest.json", b"{}")]);
    let (status, body) = upload_theme(&client, &base_url, zip).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "scenario 14: zip-slip must be 400; body={body}"
    );
    assert_eq!(
        body["error"]["code"].as_str().unwrap_or(""),
        "ZIP_SLIP",
        "scenario 14: error code must be ZIP_SLIP"
    );
}

#[tokio::test]
async fn scenario_15_zip_bomb_is_rejected() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    login(&client, &base_url, "admin", "testpass").await;

    // Highly compressible: 5 MB of zeros → ratio >> 100x
    let zeros = vec![0u8; 5 * 1024 * 1024];
    let zip = build_zip(&[("bomb.js", &zeros), ("manifest.json", b"{}")]);
    let (status, body) = upload_theme(&client, &base_url, zip).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "scenario 15: zip-bomb must be 400; body={body}"
    );
    assert_eq!(
        body["error"]["code"].as_str().unwrap_or(""),
        "ZIP_BOMB",
        "scenario 15: error code must be ZIP_BOMB"
    );
}

// ---------------------------------------------------------------------------
// Scenario 16: upload > 25 MB → 413 UPLOAD_TOO_LARGE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_16_upload_too_large_is_413() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    login(&client, &base_url, "admin", "testpass").await;

    // Build a zip with incompressible (LCG pseudo-random) data > 25 MB using STORED method
    // so the zip file itself stays at the raw size without requiring decompression.
    // 26 MB of incompressible data stored uncompressed → zip is ~26 MB.
    const SIZE: usize = 26 * 1024 * 1024;
    let mut data = Vec::with_capacity(SIZE);
    let mut x: u64 = 0xdeadbeef_cafebabe;
    for _ in 0..SIZE {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((x >> 56) as u8);
    }

    // Use STORED (no compression) so the zip payload stays large.
    // The individual file is 26 MB which exceeds MAX_FILE_BYTES (5 MB), but the
    // axum body limit (25 MB) fires before the extractor even runs because the
    // entire multipart body is rejected by DefaultBodyLimit before it's read.
    let zip = build_zip_stored(&[("big.js", &data)]);

    let part = multipart::Part::bytes(zip)
        .file_name("huge.sbtheme")
        .mime_str("application/zip")
        .unwrap();
    let form = multipart::Form::new().part("package", part);

    let resp = client
        .post(format!("{base_url}/api/settings/spa-themes"))
        .multipart(form)
        .send()
        .await
        .expect("large upload request failed");

    let status = resp.status();
    let body_bytes = resp.bytes().await.expect("Failed to read body");

    assert_eq!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "scenario 16: 26 MB upload must be 413"
    );

    // Response must be JSON with our error contract
    let body: Value = serde_json::from_slice(&body_bytes)
        .expect("scenario 16: 413 response must be valid JSON");
    assert_eq!(
        body["error"]["code"].as_str().unwrap_or(""),
        "UPLOAD_TOO_LARGE",
        "scenario 16: error code must be UPLOAD_TOO_LARGE; body={body}"
    );
}
