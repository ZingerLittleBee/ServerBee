use std::io::Write;
use std::time::Duration;

use chrono::Utc;
use reqwest::Client;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::entity::widget_module::{self, SourceType};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::service::widget_module::extractor::extract_manifest;
use serverbee_server::state::AppState;

struct TestServerContext {
    base_url: String,
    db: DatabaseConnection,
    _tmp: tempfile::TempDir,
}

/// Start an in-process test server and return a context exposing both the
/// base URL and the underlying sea-orm connection so tests can seed rows
/// directly.
async fn start_test_server_with_db() -> TestServerContext {
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

    db.execute_unprepared("PRAGMA journal_mode=WAL")
        .await
        .expect("Failed to enable WAL");
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .expect("Failed to enable foreign keys");

    Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");

    AuthService::create_user(&db, "admin", "testpass", "admin")
        .await
        .expect("Failed to seed admin");

    let state_db = db.clone();
    let state = AppState::new(state_db, config)
        .await
        .expect("Failed to create AppState");
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    TestServerContext {
        base_url,
        db,
        _tmp: tmp,
    }
}

fn http_client() -> Client {
    Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}

async fn login_admin(client: &Client, base_url: &str) {
    let resp = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&json!({ "username": "admin", "password": "testpass" }))
        .send()
        .await
        .expect("Login request failed");
    assert_eq!(resp.status(), 200, "Login should succeed");
}

async fn seed_module(db: &DatabaseConnection, id: &str) {
    let code = format!(
        r#"/**
 * @serverbee-widget {{
 *   "id": "{id}",
 *   "version": "1.0.0",
 *   "name": "Foo",
 *   "category": "Real-time",
 *   "sizing": {{ "defaultW": 3, "defaultH": 3, "minW": 2, "minH": 2, "strategy": "aspect-square" }},
 *   "sdkVersion": "^0.1.0"
 * }}
 */
export default {{}};
"#
    );
    let manifest = extract_manifest(&code).expect("manifest");
    let sha = sha256_hex(code.as_bytes());

    widget_module::ActiveModel {
        id: Set(id.to_string()),
        version: Set("1.0.0".into()),
        source_type: Set(SourceType::Upload),
        source_url: Set(None),
        bundled_by_theme_id: Set(None),
        manifest_json: Set(serde_json::to_string(&manifest).unwrap()),
        code_sha256: Set(sha),
        entry_path: Set("index.js".into()),
        package_blob: Set(Some(code.into_bytes())),
        installed_by: Set(None),
        installed_at: Set(Utc::now()),
        enabled: Set(true),
    }
    .insert(db)
    .await
    .expect("Failed to seed widget_module row");
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[tokio::test]
async fn list_returns_seeded_module() {
    let ctx = start_test_server_with_db().await;
    seed_module(&ctx.db, "com.test.foo").await;

    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    let res = client
        .get(format!("{}/api/widget-modules", ctx.base_url))
        .send()
        .await
        .expect("GET /api/widget-modules failed");
    assert_eq!(res.status(), 200, "list endpoint should return 200");

    let body: serde_json::Value = res.json().await.expect("invalid JSON body");
    let list = body["data"].as_array().expect("data should be an array");
    assert!(
        list.iter().any(|m| m["id"] == "com.test.foo"),
        "seeded module not found in list: {body}"
    );
}

#[tokio::test]
async fn serve_asset_returns_entry_bytes() {
    let ctx = start_test_server_with_db().await;
    seed_module(&ctx.db, "com.test.foo").await;

    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    let res = client
        .get(format!(
            "{}/api/widget-modules/com.test.foo/index.js",
            ctx.base_url
        ))
        .send()
        .await
        .expect("GET asset failed");
    assert_eq!(res.status(), 200);

    let ct = res
        .headers()
        .get("content-type")
        .expect("content-type header missing")
        .to_str()
        .expect("invalid content-type")
        .to_string();
    assert!(ct.contains("javascript"), "unexpected content-type: {ct}");
    assert!(
        res.headers().contains_key("etag"),
        "etag header should be present"
    );

    let body = res.text().await.expect("body text");
    assert!(
        body.contains("@serverbee-widget"),
        "asset body should contain the original JSDoc block"
    );
}

#[tokio::test]
async fn serve_asset_rejects_path_traversal() {
    let ctx = start_test_server_with_db().await;
    seed_module(&ctx.db, "com.test.foo").await;

    // Authenticate first and capture the session cookie so the raw TCP
    // request below isn't rejected by the auth middleware (which would
    // produce a 401 and mask whatever the path-traversal handling actually
    // does).
    let client = http_client();
    let login_resp = client
        .post(format!("{}/api/auth/login", ctx.base_url))
        .json(&json!({ "username": "admin", "password": "testpass" }))
        .send()
        .await
        .expect("login request failed");
    assert_eq!(login_resp.status(), 200);
    let set_cookie = login_resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("Set-Cookie missing")
        .to_str()
        .expect("invalid cookie")
        .to_string();
    let cookie_pair = set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .trim()
        .to_string();

    // Reqwest / the `url` crate normalize `..` segments client-side, and even
    // percent-encoded `%2E%2E` gets canonicalized because `.` is unreserved.
    // Craft a raw HTTP/1.1 request over a plain TCP socket so the literal
    // `../secret` reaches the server.
    let host_port = ctx
        .base_url
        .strip_prefix("http://")
        .expect("base_url prefix")
        .to_string();
    let path = "/api/widget-modules/com.test.foo/../secret";

    let mut stream = TcpStream::connect(&host_port)
        .await
        .expect("failed to connect to test server");
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host_port}\r\nCookie: {cookie_pair}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .await
        .expect("write request");

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .expect("read response");
    let response = String::from_utf8_lossy(&buf);
    let status_line = response.lines().next().unwrap_or("");
    assert!(
        status_line.starts_with("HTTP/1.1 4"),
        "path traversal should be rejected with 4xx, got status line: {status_line}"
    );
}

#[tokio::test]
async fn list_includes_builtin_hello_world() {
    let ctx = start_test_server_with_db().await;
    serverbee_server::service::widget_module::builtin::register_all(&ctx.db)
        .await
        .expect("register builtin widgets");

    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    let res = client
        .get(format!("{}/api/widget-modules", ctx.base_url))
        .send()
        .await
        .expect("GET /api/widget-modules failed");
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("invalid JSON");
    let list = body["data"].as_array().expect("data should be array");
    assert!(
        list.iter().any(|m| m["id"] == "com.serverbee.hello-world"),
        "expected hello-world in list, got {body:#?}"
    );
}

#[tokio::test]
async fn install_single_file_via_multipart_and_uninstall() {
    let ctx = start_test_server_with_db().await;

    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    let code = r#"/**
 * @serverbee-widget {
 *   "id": "com.test.uploaded",
 *   "version": "1.0.0",
 *   "name": "Uploaded",
 *   "category": "Real-time",
 *   "sizing": { "defaultW": 2, "defaultH": 2, "minW": 1, "minH": 1, "strategy": "free" },
 *   "sdkVersion": "^0.1.0"
 * }
 */
export default {};"#;

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(code.as_bytes().to_vec()).file_name("uploaded.js"),
    );

    let res = client
        .post(format!("{}/api/widget-modules", ctx.base_url))
        .multipart(form)
        .send()
        .await
        .expect("install request failed");
    assert_eq!(res.status(), 200, "install should return 200");
    let body: serde_json::Value = res.json().await.expect("invalid JSON");
    assert_eq!(body["data"]["id"], "com.test.uploaded");
    assert_eq!(body["data"]["version"], "1.0.0");

    let res2 = client
        .get(format!("{}/api/widget-modules", ctx.base_url))
        .send()
        .await
        .expect("list request failed");
    assert_eq!(res2.status(), 200);
    let list: serde_json::Value = res2.json().await.expect("invalid JSON");
    assert!(
        list["data"]
            .as_array()
            .expect("data array")
            .iter()
            .any(|m| m["id"] == "com.test.uploaded"),
        "uploaded module should appear in list: {list}"
    );

    let res3 = client
        .delete(format!(
            "{}/api/widget-modules/com.test.uploaded",
            ctx.base_url
        ))
        .send()
        .await
        .expect("delete request failed");
    assert_eq!(res3.status(), 204, "uninstall should return 204");

    let res4 = client
        .get(format!("{}/api/widget-modules", ctx.base_url))
        .send()
        .await
        .expect("post-delete list failed");
    let list2: serde_json::Value = res4.json().await.expect("invalid JSON");
    assert!(
        !list2["data"]
            .as_array()
            .expect("data array")
            .iter()
            .any(|m| m["id"] == "com.test.uploaded"),
        "module should be gone after uninstall: {list2}"
    );
}

#[tokio::test]
async fn install_rejects_invalid_manifest() {
    let ctx = start_test_server_with_db().await;

    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    // No @serverbee-widget block at all.
    let code = b"export default {};";
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(code.to_vec()).file_name("bad.js"),
    );

    let res = client
        .post(format!("{}/api/widget-modules", ctx.base_url))
        .multipart(form)
        .send()
        .await
        .expect("install request failed");
    assert_eq!(
        res.status(),
        400,
        "install without manifest block should 400"
    );
}

#[tokio::test]
async fn cannot_uninstall_builtin() {
    let ctx = start_test_server_with_db().await;
    serverbee_server::service::widget_module::builtin::register_all(&ctx.db)
        .await
        .expect("register builtin widgets");

    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    let res = client
        .delete(format!(
            "{}/api/widget-modules/com.serverbee.hello-world",
            ctx.base_url
        ))
        .send()
        .await
        .expect("delete request failed");
    assert!(
        res.status().is_client_error(),
        "uninstalling builtin should be a client error, got {}",
        res.status()
    );
}

#[tokio::test]
async fn serve_builtin_asset_returns_js_bytes() {
    let ctx = start_test_server_with_db().await;
    serverbee_server::service::widget_module::builtin::register_all(&ctx.db)
        .await
        .expect("register builtin widgets");

    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    let res = client
        .get(format!(
            "{}/api/widget-modules/com.serverbee.hello-world/index.js",
            ctx.base_url
        ))
        .send()
        .await
        .expect("GET builtin asset failed");
    assert_eq!(res.status(), 200);
    let ct = res
        .headers()
        .get("content-type")
        .expect("content-type missing")
        .to_str()
        .expect("invalid content-type")
        .to_string();
    assert!(ct.contains("javascript"), "unexpected content-type: {ct}");
    let body = res.text().await.expect("body text");
    assert!(
        body.contains("@serverbee/widget-sdk") || body.contains("defineWidget"),
        "expected SDK import or defineWidget in builtin js, got: {}",
        &body[..body.len().min(200)]
    );
}

// ---------- zip collection install tests ----------

/// Build a single-file widget JS source with the given id baked into the
/// JSDoc manifest and the body so we can confirm it round-trips.
fn build_widget_js(id: &str) -> String {
    format!(
        r#"/**
 * @serverbee-widget {{
 *   "id": "{id}",
 *   "version": "1.0.0",
 *   "name": "Test {id}",
 *   "category": "Real-time",
 *   "sizing": {{ "defaultW": 3, "defaultH": 3, "minW": 2, "minH": 2, "strategy": "aspect-square" }},
 *   "sdkVersion": "^0.1.0"
 * }}
 */
export default {{ id: "{id}" }};
"#
    )
}

/// Build an in-memory zip collection bundle from a list of `(folder, id)`
/// pairs. Each pair becomes a file at `{folder}/index.js`, and a top-level
/// `collection.json` is added listing every entry.
fn build_collection_zip(widgets: &[(&str, &str)]) -> Vec<u8> {
    use zip::write::SimpleFileOptions;

    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zw = zip::ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let entries: Vec<serde_json::Value> = widgets
            .iter()
            .map(|(folder, _)| json!({ "entry": format!("{folder}/index.js") }))
            .collect();
        let manifest = json!({ "widgets": entries });
        zw.start_file("collection.json", opts).unwrap();
        zw.write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
            .unwrap();

        for (folder, id) in widgets {
            zw.start_file(format!("{folder}/index.js"), opts).unwrap();
            zw.write_all(build_widget_js(id).as_bytes()).unwrap();
        }

        zw.finish().unwrap();
    }
    buf
}

#[tokio::test]
async fn install_collection_zip_returns_array_of_widgets() {
    let ctx = start_test_server_with_db().await;
    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    let zip = build_collection_zip(&[
        ("weather", "com.test.weather"),
        ("clock", "com.test.clock"),
    ]);

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(zip).file_name("pack.zip"),
    );

    let res = client
        .post(format!("{}/api/widget-modules", ctx.base_url))
        .multipart(form)
        .send()
        .await
        .expect("install zip request failed");
    assert_eq!(res.status(), 200, "zip install should return 200");
    let body: serde_json::Value = res.json().await.expect("invalid JSON");
    let arr = body["data"].as_array().expect("data should be array");
    assert_eq!(arr.len(), 2);
    let ids: Vec<String> = arr
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&"com.test.weather".to_string()));
    assert!(ids.contains(&"com.test.clock".to_string()));

    // Each widget served at its own /{id}/index.js, resolving inside its zip folder.
    for id in ["com.test.weather", "com.test.clock"] {
        let res = client
            .get(format!("{}/api/widget-modules/{id}/index.js", ctx.base_url))
            .send()
            .await
            .expect("asset get failed");
        assert_eq!(res.status(), 200, "asset {id} should be 200");
        let body = res.text().await.expect("body text");
        assert!(
            body.contains(id),
            "expected body for {id} to contain its id, got: {}",
            &body[..body.len().min(160)]
        );
    }

    // List endpoint exposes both.
    let res = client
        .get(format!("{}/api/widget-modules", ctx.base_url))
        .send()
        .await
        .expect("list request failed");
    let listed: serde_json::Value = res.json().await.expect("invalid JSON");
    let listed_ids: Vec<String> = listed["data"]
        .as_array()
        .expect("array")
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    assert!(listed_ids.contains(&"com.test.weather".to_string()));
    assert!(listed_ids.contains(&"com.test.clock".to_string()));

    // Deleting one leaves the other intact.
    let del = client
        .delete(format!(
            "{}/api/widget-modules/com.test.weather",
            ctx.base_url
        ))
        .send()
        .await
        .expect("delete failed");
    assert_eq!(del.status(), 204);

    let res = client
        .get(format!(
            "{}/api/widget-modules/com.test.clock/index.js",
            ctx.base_url
        ))
        .send()
        .await
        .expect("asset get failed");
    assert_eq!(
        res.status(),
        200,
        "remaining widget should still be served after sibling is removed"
    );
}

#[tokio::test]
async fn install_zip_missing_collection_json_is_400() {
    let ctx = start_test_server_with_db().await;
    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    // Build a zip with just a widget file but no collection.json.
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zw = zip::ZipWriter::new(cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("weather/index.js", opts).unwrap();
        zw.write_all(build_widget_js("com.test.weather").as_bytes())
            .unwrap();
        zw.finish().unwrap();
    }

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(buf).file_name("bad.zip"),
    );
    let res = client
        .post(format!("{}/api/widget-modules", ctx.base_url))
        .multipart(form)
        .send()
        .await
        .expect("install request failed");
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn install_zip_duplicate_ids_is_400() {
    let ctx = start_test_server_with_db().await;
    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    // Two folders point to widgets that declare the same id.
    let zip = build_collection_zip(&[
        ("a", "com.test.dup"),
        ("b", "com.test.dup"),
    ]);

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(zip).file_name("dup.zip"),
    );
    let res = client
        .post(format!("{}/api/widget-modules", ctx.base_url))
        .multipart(form)
        .send()
        .await
        .expect("install request failed");
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn install_zip_entry_with_dotdot_is_400() {
    let ctx = start_test_server_with_db().await;
    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    // Manually craft a zip whose collection.json points outside the bundle.
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zw = zip::ZipWriter::new(cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        let manifest = json!({ "widgets": [{ "entry": "../escape.js" }] });
        zw.start_file("collection.json", opts).unwrap();
        zw.write_all(manifest.to_string().as_bytes()).unwrap();

        // A regular widget at a sibling location too so the zip itself is valid.
        zw.start_file("weather/index.js", opts).unwrap();
        zw.write_all(build_widget_js("com.test.weather").as_bytes())
            .unwrap();
        zw.finish().unwrap();
    }

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(buf).file_name("evil.zip"),
    );
    let res = client
        .post(format!("{}/api/widget-modules", ctx.base_url))
        .multipart(form)
        .send()
        .await
        .expect("install request failed");
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn install_zip_entry_missing_jsdoc_is_400() {
    let ctx = start_test_server_with_db().await;
    let client = http_client();
    login_admin(&client, &ctx.base_url).await;

    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zw = zip::ZipWriter::new(cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let manifest = json!({ "widgets": [{ "entry": "weather/index.js" }] });
        zw.start_file("collection.json", opts).unwrap();
        zw.write_all(manifest.to_string().as_bytes()).unwrap();
        // Body has no @serverbee-widget block.
        zw.start_file("weather/index.js", opts).unwrap();
        zw.write_all(b"export default {};").unwrap();
        zw.finish().unwrap();
    }

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(buf).file_name("missing.zip"),
    );
    let res = client
        .post(format!("{}/api/widget-modules", ctx.base_url))
        .multipart(form)
        .send()
        .await
        .expect("install request failed");
    assert_eq!(res.status(), 400);
}
