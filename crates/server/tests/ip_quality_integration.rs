/// Integration tests for the IP Quality REST API (Tasks 12 & 13).
///
/// Each test spins up a fresh in-memory server (random port) with migrations
/// applied, seeds an admin user, and exercises the endpoints over real HTTP.
use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

// ---------------------------------------------------------------------------
// Shared helpers (mirrors the pattern from integration.rs)
// ---------------------------------------------------------------------------

async fn start_test_server() -> (String, tempfile::TempDir) {
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

    let db_path = format!("{}/test.db", data_dir);
    let db_url = format!("sqlite://{}?mode=rwc", db_path);
    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(5);
    opt.sqlx_logging(false);

    let db = Database::connect(opt)
        .await
        .expect("Failed to connect to test database");

    db.execute_unprepared("PRAGMA journal_mode=WAL")
        .await
        .unwrap();
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .unwrap();

    Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");

    AuthService::create_user(&db, "admin", "testpass", "admin")
        .await
        .expect("Failed to seed admin");

    // Also seed a member user so we can test non-admin paths.
    AuthService::create_user(&db, "member", "memberpass", "member")
        .await
        .expect("Failed to seed member");

    let state = AppState::new(db, config)
        .await
        .expect("Failed to create AppState");
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    (base_url, tmp)
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}

async fn login_admin(client: &reqwest::Client, base_url: &str) {
    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "admin", "password": "testpass" }))
        .send()
        .await
        .expect("Login request failed");
    assert_eq!(resp.status(), 200, "Admin login should succeed");
}

async fn login_member(client: &reqwest::Client, base_url: &str) {
    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "member", "password": "memberpass" }))
        .send()
        .await
        .expect("Login request failed");
    assert_eq!(resp.status(), 200, "Member login should succeed");
}

async fn register_agent(client: &reqwest::Client, base_url: &str) -> (String, String) {
    // Create a pending server (this is the new flow that also mints the
    // enrollment code as part of the same response).
    let enroll_resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "ip-quality-test" }))
        .send()
        .await
        .expect("Create-server request failed");
    assert_eq!(enroll_resp.status(), 200);
    let enroll_body: serde_json::Value = enroll_resp.json().await.unwrap();
    let code = enroll_body["data"]["enrollment"]["code"]
        .as_str()
        .unwrap()
        .to_string();

    // Register
    let register_resp = client
        .post(format!("{}/api/agent/register", base_url))
        .header("Authorization", format!("Bearer {code}"))
        .send()
        .await
        .expect("Register request failed");
    assert_eq!(register_resp.status(), 200);
    let register_body: serde_json::Value = register_resp.json().await.unwrap();
    let server_id = register_body["data"]["server_id"]
        .as_str()
        .unwrap()
        .to_string();
    let token = register_body["data"]["token"].as_str().unwrap().to_string();
    (server_id, token)
}

// ---------------------------------------------------------------------------
// Task 12: Catalog endpoints
// ---------------------------------------------------------------------------

/// GET /api/ip-quality/services — should return the 9 seeded built-in services.
#[tokio::test]
async fn test_ip_quality_list_services_returns_nine() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/ip-quality/services", base_url))
        .send()
        .await
        .expect("GET /api/ip-quality/services failed");

    assert_eq!(resp.status(), 200, "list services should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    let services = body["data"].as_array().expect("data should be an array");
    assert_eq!(
        services.len(),
        9,
        "should return 9 seeded built-in services"
    );
    assert!(
        services.iter().all(|s| s["is_builtin"].as_bool() == Some(true)),
        "all seeded services should be built-in"
    );
}

/// POST /api/ip-quality/services as non-admin (member) → 403.
#[tokio::test]
async fn test_ip_quality_create_service_non_admin_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_member(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/ip-quality/services", base_url))
        .json(&json!({
            "name": "Test Service",
            "category": "other",
            "popularity": 10,
            "url": "https://example.com/test",
            "method": "GET",
            "headers": [],
            "timeout_ms": 5000,
            "rules": [{"kind": "status_equals", "code": 200, "result": "unlocked"}]
        }))
        .send()
        .await
        .expect("POST /api/ip-quality/services failed");

    assert_eq!(
        resp.status(),
        403,
        "non-admin should be forbidden from creating services"
    );
}

/// POST /api/ip-quality/services as admin with a valid custom service → 200.
#[tokio::test]
async fn test_ip_quality_create_service_admin_success() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/ip-quality/services", base_url))
        .json(&json!({
            "name": "My Custom Unlock",
            "category": "other",
            "popularity": 50,
            "url": "https://example.com/unlock",
            "method": "GET",
            "headers": [],
            "timeout_ms": 5000,
            "rules": [{"kind": "status_equals", "code": 200, "result": "unlocked"}]
        }))
        .send()
        .await
        .expect("POST /api/ip-quality/services failed");

    assert_eq!(resp.status(), 200, "admin create service should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["name"], "My Custom Unlock");
    assert_eq!(body["data"]["is_builtin"], false);
    assert!(
        body["data"]["key"]
            .as_str()
            .unwrap()
            .starts_with("custom_"),
        "key should have custom_ prefix"
    );

    // Verify list now returns 10
    let list_resp = client
        .get(format!("{}/api/ip-quality/services", base_url))
        .send()
        .await
        .unwrap();
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    assert_eq!(
        list_body["data"].as_array().unwrap().len(),
        10,
        "should have 9 built-ins + 1 custom"
    );
}

/// DELETE /api/ip-quality/services/:id on a built-in → 400.
#[tokio::test]
async fn test_ip_quality_delete_builtin_service_is_rejected() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Get the first built-in service id
    let list_resp = client
        .get(format!("{}/api/ip-quality/services", base_url))
        .send()
        .await
        .unwrap();
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let builtin_id = list_body["data"][0]["id"]
        .as_str()
        .expect("built-in id missing");

    let del_resp = client
        .delete(format!(
            "{}/api/ip-quality/services/{}",
            base_url, builtin_id
        ))
        .send()
        .await
        .expect("DELETE /api/ip-quality/services/{id} failed");

    assert_eq!(
        del_resp.status(),
        400,
        "deleting a built-in service should return 400"
    );
}

/// PUT /api/ip-quality/services/:id — update a custom service.
#[tokio::test]
async fn test_ip_quality_update_custom_service() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create a custom service
    let create_resp = client
        .post(format!("{}/api/ip-quality/services", base_url))
        .json(&json!({
            "name": "Original",
            "category": "other",
            "popularity": 5,
            "url": "https://example.com/orig",
            "method": "GET",
            "headers": [],
            "timeout_ms": 3000,
            "rules": [{"kind": "status_equals", "code": 200, "result": "unlocked"}]
        }))
        .send()
        .await
        .unwrap();
    let create_body: serde_json::Value = create_resp.json().await.unwrap();
    let id = create_body["data"]["id"].as_str().unwrap().to_string();

    // Update: disable it
    let update_resp = client
        .put(format!("{}/api/ip-quality/services/{}", base_url, id))
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .expect("PUT /api/ip-quality/services/{id} failed");

    assert_eq!(update_resp.status(), 200, "update should succeed");
    let update_body: serde_json::Value = update_resp.json().await.unwrap();
    assert_eq!(
        update_body["data"]["enabled"], false,
        "enabled should be false after update"
    );
}

/// PUT /api/ip-quality/services/:id as non-admin (member) → 403.
#[tokio::test]
async fn test_ip_quality_update_service_non_admin_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin_client = http_client();
    login_admin(&admin_client, &base_url).await;

    // Get a built-in service id (any service id works for the guard check —
    // the request is rejected by the middleware before reaching the handler).
    let list_resp = admin_client
        .get(format!("{}/api/ip-quality/services", base_url))
        .send()
        .await
        .unwrap();
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let service_id = list_body["data"][0]["id"].as_str().unwrap().to_string();

    let member_client = http_client();
    login_member(&member_client, &base_url).await;

    let resp = member_client
        .put(format!(
            "{}/api/ip-quality/services/{}",
            base_url, service_id
        ))
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .expect("PUT /api/ip-quality/services/{id} failed");

    assert_eq!(
        resp.status(),
        403,
        "non-admin should be forbidden from updating services"
    );
}

/// DELETE /api/ip-quality/services/:id as non-admin (member) → 403.
#[tokio::test]
async fn test_ip_quality_delete_service_non_admin_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin_client = http_client();
    login_admin(&admin_client, &base_url).await;

    // Get a built-in service id (any service id works for the guard check —
    // the request is rejected by the middleware before reaching the handler).
    let list_resp = admin_client
        .get(format!("{}/api/ip-quality/services", base_url))
        .send()
        .await
        .unwrap();
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let service_id = list_body["data"][0]["id"].as_str().unwrap().to_string();

    let member_client = http_client();
    login_member(&member_client, &base_url).await;

    let resp = member_client
        .delete(format!(
            "{}/api/ip-quality/services/{}",
            base_url, service_id
        ))
        .send()
        .await
        .expect("DELETE /api/ip-quality/services/{id} failed");

    assert_eq!(
        resp.status(),
        403,
        "non-admin should be forbidden from deleting services"
    );
}

/// DELETE /api/ip-quality/services/:id on a custom service → 200.
#[tokio::test]
async fn test_ip_quality_delete_custom_service_success() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create a custom service
    let create_resp = client
        .post(format!("{}/api/ip-quality/services", base_url))
        .json(&json!({
            "name": "ToDelete",
            "category": "other",
            "popularity": 1,
            "url": "https://example.com/del",
            "method": "GET",
            "headers": [],
            "timeout_ms": 5000,
            "rules": [{"kind": "status_equals", "code": 200, "result": "unlocked"}]
        }))
        .send()
        .await
        .unwrap();
    let create_body: serde_json::Value = create_resp.json().await.unwrap();
    let id = create_body["data"]["id"].as_str().unwrap().to_string();

    let del_resp = client
        .delete(format!("{}/api/ip-quality/services/{}", base_url, id))
        .send()
        .await
        .expect("DELETE /api/ip-quality/services/{id} failed");

    assert_eq!(del_resp.status(), 200, "delete custom service should succeed");

    // Verify list is back to 9
    let list_resp = client
        .get(format!("{}/api/ip-quality/services", base_url))
        .send()
        .await
        .unwrap();
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    assert_eq!(
        list_body["data"].as_array().unwrap().len(),
        9,
        "should be back to 9 after deleting custom"
    );
}

// ---------------------------------------------------------------------------
// Task 13: Settings, overview, detail, check, events
// ---------------------------------------------------------------------------

/// GET /api/ip-quality/settings — returns the default interval.
#[tokio::test]
async fn test_ip_quality_get_settings_default() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/ip-quality/settings", base_url))
        .send()
        .await
        .expect("GET /api/ip-quality/settings failed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["check_interval_hours"], 12,
        "default interval should be 12"
    );
}

/// PUT /api/ip-quality/settings (admin) — updates interval.
#[tokio::test]
async fn test_ip_quality_put_settings_admin() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .put(format!("{}/api/ip-quality/settings", base_url))
        .json(&json!({ "check_interval_hours": 6 }))
        .send()
        .await
        .expect("PUT /api/ip-quality/settings failed");

    assert_eq!(resp.status(), 200, "PUT settings should succeed for admin");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["check_interval_hours"], 6);

    // Read back and verify
    let get_resp = client
        .get(format!("{}/api/ip-quality/settings", base_url))
        .send()
        .await
        .unwrap();
    let get_body: serde_json::Value = get_resp.json().await.unwrap();
    assert_eq!(get_body["data"]["check_interval_hours"], 6);
}

/// PUT /api/ip-quality/settings as non-admin → 403.
#[tokio::test]
async fn test_ip_quality_put_settings_non_admin_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_member(&client, &base_url).await;

    let resp = client
        .put(format!("{}/api/ip-quality/settings", base_url))
        .json(&json!({ "check_interval_hours": 6 }))
        .send()
        .await
        .expect("PUT /api/ip-quality/settings failed");

    assert_eq!(resp.status(), 403, "non-admin PUT settings should return 403");
}

/// GET /api/ip-quality/overview — returns array (may be empty).
#[tokio::test]
async fn test_ip_quality_get_overview() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/ip-quality/overview", base_url))
        .send()
        .await
        .expect("GET /api/ip-quality/overview failed");

    assert_eq!(resp.status(), 200, "overview should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    // No agent is registered in this test, so the overview has no server rows.
    // An empty array is the expected, valid response — the assertion only
    // verifies the endpoint returns a well-formed array shape.
    assert!(
        body["data"].is_array(),
        "data should be an array"
    );
}

/// GET /api/ip-quality/servers/:id — returns server summary.
#[tokio::test]
async fn test_ip_quality_get_server_summary() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Register an agent to get a real server ID
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/ip-quality/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("GET /api/ip-quality/servers/{id} failed");

    assert_eq!(resp.status(), 200, "server summary should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["server_id"], server_id);
    assert!(
        body["data"]["unlock_results"].is_array(),
        "unlock_results should be an array"
    );
}

/// GET /api/ip-quality/events — returns event list (may be empty).
#[tokio::test]
async fn test_ip_quality_get_events() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, _token) = register_agent(&client, &base_url).await;

    let resp = client
        .get(format!(
            "{}/api/ip-quality/events?server_id={}&limit=50",
            base_url, server_id
        ))
        .send()
        .await
        .expect("GET /api/ip-quality/events failed");

    assert_eq!(resp.status(), 200, "events should succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"].is_array(), "data should be an array");
}

/// POST /api/ip-quality/servers/:id/check — when server is offline → 404.
/// This is the "offline path" which we can always test without a live WS agent.
#[tokio::test]
async fn test_ip_quality_check_server_offline_returns_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Register an agent (so the server_id exists in DB) but don't connect via WS
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let resp = client
        .post(format!(
            "{}/api/ip-quality/servers/{}/check",
            base_url, server_id
        ))
        .send()
        .await
        .expect("POST /api/ip-quality/servers/{id}/check failed");

    // Agent is not connected via WS, so agent_manager has no sender → 404
    assert_eq!(
        resp.status(),
        404,
        "check on offline server should return 404"
    );
}

/// POST /api/ip-quality/servers/:id/check as non-admin → 403.
#[tokio::test]
async fn test_ip_quality_check_non_admin_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin_client = http_client();
    login_admin(&admin_client, &base_url).await;

    let (server_id, _token) = register_agent(&admin_client, &base_url).await;

    let member_client = http_client();
    login_member(&member_client, &base_url).await;

    let resp = member_client
        .post(format!(
            "{}/api/ip-quality/servers/{}/check",
            base_url, server_id
        ))
        .send()
        .await
        .expect("POST /api/ip-quality/servers/{id}/check failed");

    assert_eq!(
        resp.status(),
        403,
        "non-admin check should return 403"
    );
}
