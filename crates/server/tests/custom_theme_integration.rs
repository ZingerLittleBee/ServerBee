use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use serverbee_server::config::{AdminConfig, AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::service::config::ConfigService;
use serverbee_server::state::AppState;

const LIGHT_VALUE: &str = "oklch(0.5 0.1 180)";
const DARK_VALUE: &str = "oklch(0.3 0.1 180)";

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
            auto_discovery_key: "test-key".to_string(),
            secure_cookie: false,
            max_servers: 0,
        },
        admin: AdminConfig {
            username: "admin".to_string(),
            password: "testpass".to_string(),
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

    AuthService::init_admin(&db, &config.admin)
        .await
        .expect("Failed to init admin");

    if create_member {
        AuthService::create_user(&db, "member", "memberpass", "member")
            .await
            .expect("Failed to create member");
    }

    ConfigService::set(&db, "auto_discovery_key", "test-key")
        .await
        .expect("Failed to set auto_discovery_key");

    let state = AppState::new(db, config)
        .await
        .expect("Failed to create AppState");
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener
        .local_addr()
        .expect("Failed to read listener address");
    let base_url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .expect("Test server failed");
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    (base_url, tmp)
}

fn http_client() -> Client {
    Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}

async fn login(client: &Client, base_url: &str, username: &str, password: &str) {
    let resp = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&json!({
            "username": username,
            "password": password,
        }))
        .send()
        .await
        .expect("Login request failed");

    assert_eq!(resp.status(), StatusCode::OK, "Login should succeed");
}

async fn admin_login(client: &Client, base_url: &str) {
    login(client, base_url, "admin", "testpass").await;
}

async fn member_login(client: &Client, base_url: &str) {
    login(client, base_url, "member", "memberpass").await;
}

async fn request_json(
    client: &Client,
    method: Method,
    url: String,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = client.request(method, url);
    if let Some(body) = body {
        request = request.json(&body);
    }

    let resp = request.send().await.expect("Request failed");
    let status = resp.status();
    let bytes = resp.bytes().await.expect("Failed to read response body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };

    (status, body)
}

fn full_vars(value: &str) -> Value {
    let keys = [
        "background",
        "foreground",
        "card",
        "card-foreground",
        "popover",
        "popover-foreground",
        "primary",
        "primary-foreground",
        "secondary",
        "secondary-foreground",
        "muted",
        "muted-foreground",
        "accent",
        "accent-foreground",
        "destructive",
        "border",
        "input",
        "ring",
        "chart-1",
        "chart-2",
        "chart-3",
        "chart-4",
        "chart-5",
        "sidebar",
        "sidebar-foreground",
        "sidebar-primary",
        "sidebar-primary-foreground",
        "sidebar-accent",
        "sidebar-accent-foreground",
        "sidebar-border",
        "sidebar-ring",
    ];
    let mut m = serde_json::Map::new();
    for k in keys {
        m.insert(k.to_string(), Value::String(value.to_string()));
    }
    Value::Object(m)
}

fn theme_body(name: &str) -> Value {
    json!({
        "name": name,
        "description": "Integration test theme",
        "based_on": "default",
        "vars_light": full_vars(LIGHT_VALUE),
        "vars_dark": full_vars(DARK_VALUE),
    })
}

async fn create_theme(client: &Client, base_url: &str, name: &str) -> Value {
    let (status, body) = request_json(
        client,
        Method::POST,
        format!("{base_url}/api/settings/themes"),
        Some(theme_body(name)),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "Theme creation should succeed: {body}"
    );
    body["data"].clone()
}

#[tokio::test]
async fn admin_can_create_get_update_delete_theme() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    admin_login(&client, &base_url).await;

    let created = create_theme(&client, &base_url, "Ocean").await;
    let id = created["id"]
        .as_i64()
        .expect("Created theme should have id");

    let (get_status, fetched) = request_json(
        &client,
        Method::GET,
        format!("{base_url}/api/settings/themes/{id}"),
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["data"]["name"], "Ocean");

    let (list_status, listed) = request_json(
        &client,
        Method::GET,
        format!("{base_url}/api/settings/themes"),
        None,
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    let themes = listed["data"]
        .as_array()
        .expect("Theme list should be an array");
    assert!(
        themes.iter().any(|theme| theme["id"].as_i64() == Some(id)),
        "Theme list should contain created id: {listed}"
    );

    let (update_status, updated) = request_json(
        &client,
        Method::PUT,
        format!("{base_url}/api/settings/themes/{id}"),
        Some(theme_body("Ocean Updated")),
    )
    .await;
    assert_eq!(update_status, StatusCode::OK);
    assert_eq!(updated["data"]["name"], "Ocean Updated");

    let (duplicate_status, duplicated) = request_json(
        &client,
        Method::POST,
        format!("{base_url}/api/settings/themes/{id}/duplicate"),
        None,
    )
    .await;
    assert_eq!(duplicate_status, StatusCode::OK);
    let copied_name = duplicated["data"]["name"]
        .as_str()
        .expect("Copied theme should have a name");
    assert!(
        copied_name.ends_with("(copy)"),
        "Copied name should end with (copy): {copied_name}"
    );

    let (delete_status, deleted) = request_json(
        &client,
        Method::DELETE,
        format!("{base_url}/api/settings/themes/{id}"),
        None,
    )
    .await;
    assert_eq!(
        delete_status,
        StatusCode::OK,
        "Delete should succeed: {deleted}"
    );

    let (missing_status, _) = request_json(
        &client,
        Method::GET,
        format!("{base_url}/api/settings/themes/{id}"),
        None,
    )
    .await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn member_cannot_write_themes() {
    let (base_url, _tmp) = start_test_server(true).await;
    let client = http_client();
    member_login(&client, &base_url).await;

    let (status, _) = request_json(
        &client,
        Method::POST,
        format!("{base_url}/api/settings/themes"),
        Some(theme_body("Member Theme")),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rejects_missing_variable() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    admin_login(&client, &base_url).await;

    let mut body = theme_body("Broken Theme");
    body["vars_light"]
        .as_object_mut()
        .expect("vars_light should be an object")
        .remove("background");

    let (status, response) = request_json(
        &client,
        Method::POST,
        format!("{base_url}/api/settings/themes"),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Missing variable should be rejected: {response}"
    );
}

#[tokio::test]
async fn delete_blocked_when_theme_is_active() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    admin_login(&client, &base_url).await;

    let created = create_theme(&client, &base_url, "Active Ocean").await;
    let id = created["id"]
        .as_i64()
        .expect("Created theme should have id");

    let (activate_status, _) = request_json(
        &client,
        Method::PUT,
        format!("{base_url}/api/settings/active-theme"),
        Some(json!({ "ref": format!("custom:{id}") })),
    )
    .await;
    assert_eq!(activate_status, StatusCode::OK);

    let (blocked_status, blocked) = request_json(
        &client,
        Method::DELETE,
        format!("{base_url}/api/settings/themes/{id}"),
        None,
    )
    .await;
    assert_eq!(
        blocked_status,
        StatusCode::CONFLICT,
        "Active theme delete should be blocked: {blocked}"
    );

    let (reset_status, _) = request_json(
        &client,
        Method::PUT,
        format!("{base_url}/api/settings/active-theme"),
        Some(json!({ "ref": "preset:default" })),
    )
    .await;
    assert_eq!(reset_status, StatusCode::OK);

    let (delete_status, deleted) = request_json(
        &client,
        Method::DELETE,
        format!("{base_url}/api/settings/themes/{id}"),
        None,
    )
    .await;
    assert_eq!(
        delete_status,
        StatusCode::OK,
        "Delete should succeed: {deleted}"
    );
}

#[tokio::test]
async fn active_theme_returns_resolved_payload() {
    let (base_url, _tmp) = start_test_server(false).await;
    let client = http_client();
    admin_login(&client, &base_url).await;

    let created = create_theme(&client, &base_url, "Resolved Ocean").await;
    let id = created["id"]
        .as_i64()
        .expect("Created theme should have id");

    let (activate_status, _) = request_json(
        &client,
        Method::PUT,
        format!("{base_url}/api/settings/active-theme"),
        Some(json!({ "ref": format!("custom:{id}") })),
    )
    .await;
    assert_eq!(activate_status, StatusCode::OK);

    let (status, active) = request_json(
        &client,
        Method::GET,
        format!("{base_url}/api/settings/active-theme"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(active["data"]["ref"], format!("custom:{id}"));
    assert_eq!(active["data"]["theme"]["kind"], "custom");
    assert_eq!(active["data"]["theme"]["id"], id);
    assert_eq!(active["data"]["theme"]["name"], "Resolved Ocean");
    assert_eq!(
        active["data"]["theme"]["vars_light"]["background"],
        LIGHT_VALUE
    );
    assert_eq!(
        active["data"]["theme"]["vars_dark"]["background"],
        DARK_VALUE
    );
}
