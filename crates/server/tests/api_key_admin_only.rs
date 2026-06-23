/// Regression test: minting an API key is admin-only.
///
/// An API key is a permanent, non-expiring credential bound to the caller's
/// role. Allowing any authenticated user (incl. a leaked short-lived mobile
/// Bearer token) to mint one turns a transient credential into standing
/// persistence. `POST /api/auth/api-keys` therefore lives behind `require_admin`;
/// a member must be rejected with 403 while an admin still succeeds.
use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

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

async fn login(client: &reqwest::Client, base_url: &str, username: &str, password: &str) {
    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("Login request failed");
    assert_eq!(resp.status(), 200, "{username} login should succeed");
}

/// A member must NOT be able to mint an API key.
#[tokio::test]
async fn member_cannot_create_api_key() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login(&client, &base_url, "member", "memberpass").await;

    let resp = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "member-attempt" }))
        .send()
        .await
        .expect("POST /api/auth/api-keys failed");

    assert_eq!(
        resp.status(),
        403,
        "a member must be forbidden from minting an API key"
    );
}

/// An admin must still be able to mint an API key.
#[tokio::test]
async fn admin_can_create_api_key() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login(&client, &base_url, "admin", "testpass").await;

    let resp = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "admin-key" }))
        .send()
        .await
        .expect("POST /api/auth/api-keys failed");

    assert_eq!(resp.status(), 200, "admin API key creation should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse api key response");
    assert!(
        body["data"]["key"].as_str().is_some_and(|k| !k.is_empty()),
        "response must carry the plaintext key"
    );
}
