use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};

use serverbee_server::config::{AdminConfig, AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::service::config::ConfigService;
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

    AuthService::init_admin(&db, &config.admin)
        .await
        .expect("Failed to init admin");

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

async fn mint_enrollment_code(client: &reqwest::Client, base_url: &str) -> String {
    login_admin(client, base_url).await;
    let resp = client
        .post(format!("{}/api/agent/enrollments", base_url))
        .json(&json!({}))
        .send()
        .await
        .expect("Enrollment mint request failed");
    assert_eq!(resp.status(), 200, "Enrollment mint should succeed");
    let body: Value = resp
        .json()
        .await
        .expect("Failed to parse enrollment response");
    body["data"]["code"]
        .as_str()
        .expect("enrollment code missing")
        .to_string()
}

async fn register_agent(client: &reqwest::Client, base_url: &str) -> String {
    let code = mint_enrollment_code(client, base_url).await;
    let register_resp = client
        .post(format!("{}/api/agent/register", base_url))
        .header("Authorization", format!("Bearer {code}"))
        .send()
        .await
        .expect("Register request failed");

    assert_eq!(
        register_resp.status(),
        200,
        "Agent registration should succeed"
    );
    let register_body: Value = register_resp
        .json()
        .await
        .expect("Failed to parse register response");

    register_body["data"]["server_id"]
        .as_str()
        .expect("server_id missing")
        .to_string()
}

async fn configure_server_cost(client: &reqwest::Client, base_url: &str, server_id: &str) {
    let update_resp = client
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({
            "price": 5.0,
            "billing_cycle": "monthly",
            "currency": "USD",
            "billing_start_day": 1,
            "traffic_limit": 1_099_511_627_776_i64,
            "traffic_limit_type": "sum"
        }))
        .send()
        .await
        .expect("Update server failed");

    assert_eq!(update_resp.status(), 200, "Server update should succeed");
}

fn data_object(body: &Value) -> &serde_json::Map<String, Value> {
    body["data"].as_object().expect("data should be an object")
}

#[tokio::test]
async fn cost_overview_requires_auth_and_returns_configured_servers() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let unauth = client
        .get(format!("{}/api/cost/overview", base_url))
        .send()
        .await
        .expect("GET /api/cost/overview failed");
    assert_eq!(unauth.status(), 401);

    login_admin(&client, &base_url).await;
    let server_id = register_agent(&client, &base_url).await;
    configure_server_cost(&client, &base_url, &server_id).await;

    let resp = client
        .get(format!("{}/api/cost/overview", base_url))
        .send()
        .await
        .expect("GET /api/cost/overview failed");

    assert_eq!(resp.status(), 200);
    let body: Value = resp
        .json()
        .await
        .expect("Failed to parse overview response");
    let data = data_object(&body);
    let servers = data["servers"]
        .as_array()
        .expect("servers should be an array");
    let configured = servers
        .iter()
        .find(|server| server["server_id"] == server_id)
        .expect("configured server missing from cost overview");

    assert_eq!(configured["configured"], true);
    assert_eq!(configured["invalid_reason"], Value::Null);
    assert_eq!(configured["currency"], "USD");
    assert_eq!(configured["billing_cycle"], "monthly");
    assert!(configured["cost_per_day"].is_number());
    assert!(configured["cost_per_month_equivalent"].is_number());
    assert!(configured["cycle_cost_elapsed"].is_number());
    assert!(configured["value_score"].is_object());
}

#[tokio::test]
async fn server_cost_insights_returns_unconfigured_price_without_error() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let server_id = register_agent(&client, &base_url).await;

    let resp = client
        .get(format!(
            "{}/api/servers/{}/cost-insights",
            base_url, server_id
        ))
        .send()
        .await
        .expect("GET /api/servers/{id}/cost-insights failed");

    assert_eq!(resp.status(), 200);
    let body: Value = resp
        .json()
        .await
        .expect("Failed to parse insights response");
    let data = data_object(&body);

    assert_eq!(data["server_id"], server_id);
    assert_eq!(data["configured"], false);
    assert_eq!(data["invalid_reason"], "missing_price");
    assert_eq!(data["price"], Value::Null);
    assert!(!data.contains_key("token_hash"));
    assert!(!data.contains_key("token_prefix"));
}

#[tokio::test]
async fn server_cost_insights_unknown_server_returns_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!(
            "{}/api/servers/not-a-real-server/cost-insights",
            base_url
        ))
        .send()
        .await
        .expect("GET /api/servers/{id}/cost-insights failed");

    assert_eq!(resp.status(), 404);
    let body: Value = resp
        .json()
        .await
        .expect("Failed to parse cost insights error response");
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .contains("Server not found")
    );
}

#[tokio::test]
async fn traffic_overview_response_does_not_include_cost_fields() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let server_id = register_agent(&client, &base_url).await;
    configure_server_cost(&client, &base_url, &server_id).await;

    let resp = client
        .get(format!("{}/api/traffic/overview", base_url))
        .send()
        .await
        .expect("GET /api/traffic/overview failed");

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("Failed to parse traffic response");
    let servers = body["data"].as_array().expect("data should be an array");
    let server = servers
        .iter()
        .find(|server| server["server_id"] == server_id)
        .expect("server missing from traffic overview");
    let server = server
        .as_object()
        .expect("traffic overview item should be an object");

    for cost_key in [
        "configured",
        "invalid_reason",
        "currency",
        "cost_per_second",
        "cost_per_hour",
        "cost_per_day",
        "cost_per_month_equivalent",
        "cycle_cost_elapsed",
        "cycle_burn_percent",
        "value_score",
    ] {
        assert!(
            !server.contains_key(cost_key),
            "traffic overview should not include {cost_key}"
        );
    }
}
