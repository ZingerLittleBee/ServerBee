use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};

use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

async fn start_test_server_with_cap(max_servers: u32) -> (String, tempfile::TempDir) {
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
            max_servers,
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
        .expect("Failed to set journal mode");
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .expect("Failed to enable foreign keys");

    Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");

    AuthService::create_user(&db, "admin", "testpass", "admin")
        .await
        .expect("Failed to seed admin");

    let state = AppState::new(db, config)
        .await
        .expect("Failed to create AppState");
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().expect("Failed to read listener addr");
    let base_url = format!("http://{}", addr);

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

async fn start_test_server() -> (String, tempfile::TempDir) {
    start_test_server_with_cap(0).await
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
        .json(&json!({
            "username": "admin",
            "password": "testpass"
        }))
        .send()
        .await
        .expect("Login request failed");

    assert_eq!(resp.status(), 200, "Login should succeed");
}

#[tokio::test]
async fn post_servers_creates_pending_with_bound_enrollment() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": "vps-1"}))
        .send()
        .await
        .expect("create server request failed");
    assert_eq!(resp.status(), 200, "POST /api/servers should succeed");

    let body: Value = resp.json().await.expect("Failed to parse response");
    let data = &body["data"];
    assert!(data["server_id"].is_string(), "server_id must be a string");

    let enrollment = &data["enrollment"];
    assert!(enrollment["id"].is_string(), "enrollment.id must be a string");

    let code = enrollment["code"]
        .as_str()
        .expect("enrollment.code must be a string");
    assert!(
        code.len() >= 16,
        "enrollment code length should be >= 16, got {}",
        code.len()
    );

    let code_prefix = enrollment["code_prefix"]
        .as_str()
        .expect("enrollment.code_prefix must be a string");
    assert_eq!(&code[..8], code_prefix, "code prefix mismatch");

    let expires_at = enrollment["expires_at"]
        .as_str()
        .expect("expires_at must be a string");
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .expect("expires_at must parse as RFC3339");
}

#[tokio::test]
async fn post_servers_with_full_metadata_persists_tags_and_billing() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({
            "name": "vps-prod",
            "tags": ["db", "prod"],
            "remark": "primary",
            "public_remark": "edge",
            "price": 5.0,
            "currency": "USD",
            "billing_cycle": "monthly",
            "billing_start_day": 1,
            "traffic_limit": 1099511627776_i64,
            "traffic_limit_type": "sum"
        }))
        .send()
        .await
        .expect("create server request failed");
    assert_eq!(resp.status(), 200, "POST /api/servers should succeed");
    let body: Value = resp.json().await.expect("Failed to parse response");
    let server_id = body["data"]["server_id"]
        .as_str()
        .expect("server_id")
        .to_string();

    let get_resp = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("get server request failed");
    assert_eq!(get_resp.status(), 200);
    let get_body: Value = get_resp.json().await.expect("Failed to parse get response");
    let data = &get_body["data"];
    assert_eq!(data["price"], 5.0);
    assert_eq!(data["currency"], "USD");
    assert_eq!(data["billing_cycle"], "monthly");
    assert_eq!(data["billing_start_day"], 1);
    assert_eq!(data["traffic_limit"], 1099511627776_i64);
    assert_eq!(data["traffic_limit_type"], "sum");
    assert_eq!(data["remark"], "primary");
    assert_eq!(data["public_remark"], "edge");

    let tags_resp = client
        .get(format!("{}/api/servers/{}/tags", base_url, server_id))
        .send()
        .await
        .expect("get tags request failed");
    assert_eq!(tags_resp.status(), 200);
    let tags_body: Value = tags_resp.json().await.expect("Failed to parse tags");
    let mut tags: Vec<String> = tags_body["data"]
        .as_array()
        .expect("tags should be array")
        .iter()
        .map(|t| t.as_str().expect("tag should be string").to_string())
        .collect();
    tags.sort();
    assert_eq!(tags, vec!["db".to_string(), "prod".to_string()]);
}

#[tokio::test]
async fn post_servers_respects_max_servers_cap() {
    let (base_url, _tmp) = start_test_server_with_cap(1).await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let first = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": "first"}))
        .send()
        .await
        .expect("first create failed");
    assert_eq!(first.status(), 200, "first create should succeed");

    let second = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": "second"}))
        .send()
        .await
        .expect("second create failed");
    assert_eq!(
        second.status(),
        400,
        "second create should hit max_servers cap"
    );
}

#[tokio::test]
async fn get_server_returns_has_token_and_outstanding_enrollment() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": "vps-pending"}))
        .send()
        .await
        .expect("create server request failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("Failed to parse response");
    let server_id = body["data"]["server_id"]
        .as_str()
        .expect("server_id")
        .to_string();
    let returned_code_prefix = body["data"]["enrollment"]["code_prefix"]
        .as_str()
        .expect("code_prefix")
        .to_string();
    let returned_enrollment_id = body["data"]["enrollment"]["id"]
        .as_str()
        .expect("enrollment id")
        .to_string();

    let get_resp = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("get server request failed");
    assert_eq!(get_resp.status(), 200);
    let get_body: Value = get_resp.json().await.expect("Failed to parse get response");
    let data = &get_body["data"];

    assert_eq!(data["has_token"], false, "pending server must have has_token=false");

    let outstanding = &data["outstanding_enrollment"];
    assert!(
        outstanding.is_object(),
        "outstanding_enrollment must be present for pending server"
    );
    assert_eq!(outstanding["id"], returned_enrollment_id);
    assert_eq!(outstanding["code_prefix"], returned_code_prefix);
    assert!(outstanding["expires_at"].is_string());
    assert!(outstanding["created_at"].is_string());

    // Plaintext code must NOT leak through the GET endpoint.
    assert!(
        outstanding.get("code").is_none(),
        "GET response must not include plaintext code"
    );
}
