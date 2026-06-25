//! Shared integration-test harness for router-level tests.
//!
//! Files under `tests/` subdirectories are NOT compiled as their own test
//! binaries, so this module is included into each top-level test file via
//! `mod common;`. Each consumer uses only a subset of these helpers, hence the
//! crate-wide `dead_code` allowance.
#![allow(dead_code)]

use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};

use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

/// Start a test server in a temporary directory bound to a random port.
///
/// Returns `(base_url, temp_dir)`. Keep `temp_dir` alive for the duration of
/// the test — dropping it removes the on-disk SQLite database.
pub async fn start_test_server() -> (String, tempfile::TempDir) {
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

    // Seed a ready-to-use admin (password known, onboarding already done).
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

/// Build a reqwest client that stores cookies automatically (for session auth).
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}

/// Log in as the seeded admin; the session cookie is stored on `client`.
pub async fn login_admin(client: &reqwest::Client, base_url: &str) -> Value {
    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "admin", "password": "testpass" }))
        .send()
        .await
        .expect("Login request failed");
    assert_eq!(resp.status(), 200, "Login should succeed");
    resp.json::<Value>().await.expect("parse login response")
}

/// Create a second user with the given role and return an authenticated client
/// logged in as that user. Requires an already-authenticated admin client.
pub async fn login_as_new_user(
    admin: &reqwest::Client,
    base_url: &str,
    username: &str,
    role: &str,
) -> reqwest::Client {
    admin
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": username, "password": "memberpass", "role": role }))
        .send()
        .await
        .expect("create user failed");

    let client = http_client();
    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": username, "password": "memberpass" }))
        .send()
        .await
        .expect("member login failed");
    assert_eq!(resp.status(), 200, "member login should succeed");
    client
}

/// Create a pending server via the admin API and return its server id.
pub async fn create_server(client: &reqwest::Client, base_url: &str, name: &str) -> String {
    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": name }))
        .send()
        .await
        .expect("create server failed");
    assert_eq!(resp.status(), 200, "create server should succeed");
    let body: Value = resp.json().await.expect("parse create-server response");
    body["data"]["server_id"]
        .as_str()
        .expect("server id missing")
        .to_string()
}

// ── Mock-agent WebSocket harness ──────────────────────────────────────────
//
// These helpers let a test stand up a fake agent that connects over the agent
// WebSocket, completes the SystemInfo handshake, and then answers control-plane
// requests (file / docker / exec) the server forwards to it. They mirror the
// proven pattern in tests/integration.rs and tests/docker_integration.rs.

use futures_util::{SinkExt, StreamExt};
use std::time::Duration as StdDuration;
use tokio_tungstenite::tungstenite;

/// Write half of a connected mock-agent WebSocket.
pub type AgentSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
/// Read half of a connected mock-agent WebSocket.
pub type AgentReader = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Mint an enrollment code by creating a pending server as admin.
pub async fn mint_enrollment_code(client: &reqwest::Client, base_url: &str, name: &str) -> String {
    login_admin(client, base_url).await;
    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": name }))
        .send()
        .await
        .expect("create-server request failed");
    assert_eq!(resp.status(), 200, "create server should succeed");
    let body: Value = resp.json().await.expect("parse create-server response");
    body["data"]["enrollment"]["code"]
        .as_str()
        .expect("enrollment code missing")
        .to_string()
}

/// Register an agent (enrollment → register) and return `(server_id, token)`.
pub async fn register_agent(client: &reqwest::Client, base_url: &str) -> (String, String) {
    let code = mint_enrollment_code(client, base_url, "mock-agent-server").await;
    let resp = client
        .post(format!("{}/api/agent/register", base_url))
        .header("Authorization", format!("Bearer {code}"))
        .send()
        .await
        .expect("register request failed");
    assert_eq!(resp.status(), 200, "agent registration should succeed");
    let body: Value = resp.json().await.expect("parse register response");
    let server_id = body["data"]["server_id"].as_str().expect("server_id missing").to_string();
    let token = body["data"]["token"].as_str().expect("token missing").to_string();
    (server_id, token)
}

/// Open the agent WebSocket with the given token and return its split halves.
pub async fn connect_agent(base_url: &str, token: &str) -> (AgentSink, AgentReader) {
    let ws_url = format!("{}/api/agent/ws?token={}", base_url.replace("http://", "ws://"), token);
    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("agent WebSocket connection failed");
    ws_stream.split()
}

/// Receive the next text frame from the agent socket, parsed as JSON (5s timeout).
pub async fn recv_agent_text(reader: &mut AgentReader) -> Value {
    let message = tokio::time::timeout(StdDuration::from_secs(5), reader.next())
        .await
        .expect("timed out waiting for agent message")
        .expect("agent WebSocket stream ended")
        .expect("agent WebSocket read error");
    match message {
        tungstenite::Message::Text(text) => {
            serde_json::from_str(&text).expect("parse agent message")
        }
        other => panic!("expected Text message, got: {:?}", other),
    }
}

/// Complete the SystemInfo handshake with the given capability bitmask and wait
/// for the server's Ack. `agent_local_capabilities = None` lets the server apply
/// its default capability set.
pub async fn send_system_info(
    sink: &mut AgentSink,
    reader: &mut AgentReader,
    msg_id: &str,
    agent_local_capabilities: Option<u32>,
) {
    let system_info = json!({
        "type": "system_info",
        "msg_id": msg_id,
        "cpu_name": "Intel Xeon E5-2680 v4",
        "cpu_cores": 8,
        "cpu_arch": "x86_64",
        "os": "Ubuntu 22.04",
        "kernel_version": "5.15.0-100-generic",
        "mem_total": 16_000_000_000_i64,
        "swap_total": 4_000_000_000_i64,
        "disk_total": 100_000_000_000_i64,
        "ipv4": "1.2.3.4",
        "ipv6": null,
        "virtualization": "kvm",
        "agent_version": "0.1.0",
        "protocol_version": serverbee_common::constants::PROTOCOL_VERSION,
        "features": [],
        "agent_local_capabilities": agent_local_capabilities
    });
    sink.send(tungstenite::Message::Text(system_info.to_string().into()))
        .await
        .expect("send SystemInfo");
    loop {
        let msg = recv_agent_text(reader).await;
        if msg["type"] == "ack" {
            assert_eq!(msg["msg_id"], msg_id);
            break;
        }
    }
}
