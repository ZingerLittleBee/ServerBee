use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tokio_tungstenite::tungstenite;

use serverbee_server::config::{
    AdminConfig, AppConfig, AuthConfig, DatabaseConfig, ServerConfig,
};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::service::config::ConfigService;
use serverbee_server::state::AppState;

/// Start a test server in a temporary directory with a random port.
/// Returns `(base_url, temp_dir)` where `temp_dir` is kept alive for the
/// duration of the test (dropping it removes the temporary directory).
async fn start_test_server() -> (String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let data_dir = tmp.path().to_str().unwrap().to_string();

    let config = AppConfig {
        server: ServerConfig {
            listen: "127.0.0.1:0".to_string(),
            data_dir: data_dir.clone(),
        },
        database: DatabaseConfig {
            path: "test.db".to_string(),
            max_connections: 5,
        },
        auth: AuthConfig {
            session_ttl: 86400,
            auto_discovery_key: "test-key".to_string(),
            secure_cookie: false,
        },
        admin: AdminConfig {
            username: "admin".to_string(),
            password: "testpass".to_string(),
        },
        ..AppConfig::default()
    };

    // Connect to SQLite
    let db_path = format!("{}/test.db", data_dir);
    let db_url = format!("sqlite://{}?mode=rwc", db_path);
    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(5);
    opt.sqlx_logging(false);

    let db = Database::connect(opt)
        .await
        .expect("Failed to connect to test database");

    // SQLite pragmas
    db.execute_unprepared("PRAGMA journal_mode=WAL")
        .await
        .unwrap();
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .unwrap();

    // Run migrations
    Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");

    // Initialize admin user
    AuthService::init_admin(&db, &config.admin)
        .await
        .expect("Failed to init admin");

    // Persist the auto-discovery key
    ConfigService::set(&db, "auto_discovery_key", "test-key")
        .await
        .expect("Failed to set auto_discovery_key");

    // Build state and router
    let state = AppState::new(db, config);
    let app = create_router(state);

    // Bind to a random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    // Give the server a moment to start accepting connections
    tokio::time::sleep(Duration::from_millis(50)).await;

    (base_url, tmp)
}

/// Build a reqwest client that stores cookies automatically.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}

/// Login as admin and return the authenticated client (with session cookie).
async fn login_admin(client: &reqwest::Client, base_url: &str) -> serde_json::Value {
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
    resp.json::<serde_json::Value>()
        .await
        .expect("Failed to parse login response")
}

#[tokio::test]
async fn test_agent_register_connect_report() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    // ── Step 1: Register agent ──
    let register_resp = client
        .post(format!("{}/api/agent/register", base_url))
        .header("Authorization", "Bearer test-key")
        .send()
        .await
        .expect("Register request failed");

    assert_eq!(register_resp.status(), 200, "Agent registration should succeed");
    let register_body: serde_json::Value = register_resp
        .json()
        .await
        .expect("Failed to parse register response");

    let server_id = register_body["data"]["server_id"]
        .as_str()
        .expect("server_id missing");
    let token = register_body["data"]["token"]
        .as_str()
        .expect("token missing");

    assert!(!server_id.is_empty(), "server_id should not be empty");
    assert!(!token.is_empty(), "token should not be empty");

    // ── Step 2: Connect via WebSocket ──
    let ws_url = format!(
        "{}/api/agent/ws?token={}",
        base_url.replace("http://", "ws://"),
        token
    );
    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("WebSocket connection failed");

    let (mut ws_sink, mut ws_reader) = ws_stream.split();

    // Read Welcome message
    let welcome_msg = tokio::time::timeout(Duration::from_secs(5), ws_reader.next())
        .await
        .expect("Timeout waiting for Welcome")
        .expect("WebSocket stream ended")
        .expect("WebSocket read error");

    let welcome_text = match welcome_msg {
        tungstenite::Message::Text(t) => t.to_string(),
        other => panic!("Expected Text message, got: {:?}", other),
    };

    let welcome: serde_json::Value =
        serde_json::from_str(&welcome_text).expect("Failed to parse Welcome");
    assert_eq!(welcome["type"], "welcome");
    assert_eq!(welcome["server_id"], server_id);
    assert_eq!(welcome["protocol_version"], 1);

    // ── Step 3: Send SystemInfo ──
    let system_info = json!({
        "type": "system_info",
        "msg_id": "test-msg-1",
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
        "agent_version": "0.1.0"
    });

    ws_sink
        .send(tungstenite::Message::Text(system_info.to_string().into()))
        .await
        .expect("Failed to send SystemInfo");

    // Read the Ack for SystemInfo
    let ack_msg = tokio::time::timeout(Duration::from_secs(5), ws_reader.next())
        .await
        .expect("Timeout waiting for Ack")
        .expect("WebSocket stream ended")
        .expect("WebSocket read error");

    let ack_text = match ack_msg {
        tungstenite::Message::Text(t) => t.to_string(),
        other => panic!("Expected Text Ack, got: {:?}", other),
    };

    let ack: serde_json::Value =
        serde_json::from_str(&ack_text).expect("Failed to parse Ack");
    assert_eq!(ack["type"], "ack");
    assert_eq!(ack["msg_id"], "test-msg-1");

    // ── Step 4: Send Report ──
    let report = json!({
        "type": "report",
        "cpu": 45.5,
        "mem_used": 8_000_000_000_i64,
        "swap_used": 500_000_000_i64,
        "disk_used": 30_000_000_000_i64,
        "net_in_speed": 1_000_000_i64,
        "net_out_speed": 500_000_i64,
        "net_in_transfer": 10_000_000_000_i64,
        "net_out_transfer": 5_000_000_000_i64,
        "load1": 1.5,
        "load5": 1.2,
        "load15": 0.8,
        "tcp_conn": 42,
        "udp_conn": 5,
        "process_count": 120,
        "uptime": 86400_u64,
        "temperature": 55.0,
        "gpu": null
    });

    ws_sink
        .send(tungstenite::Message::Text(report.to_string().into()))
        .await
        .expect("Failed to send Report");

    // Small delay to let the server process the report
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── Step 5: Login as admin and verify ──
    let login_body = login_admin(&client, &base_url).await;
    assert_eq!(login_body["data"]["username"], "admin");

    // ── Step 6: GET /api/servers → verify server is listed ──
    let servers_resp = client
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .expect("GET /api/servers failed");

    assert_eq!(servers_resp.status(), 200);
    let servers_body: serde_json::Value = servers_resp
        .json()
        .await
        .expect("Failed to parse servers response");

    let servers = servers_body["data"]
        .as_array()
        .expect("data should be an array");
    assert!(
        servers.iter().any(|s| s["id"] == server_id),
        "Registered server should appear in /api/servers"
    );

    // ── Step 7: GET /api/servers/{server_id} → verify SystemInfo fields ──
    let server_resp = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("GET /api/servers/{id} failed");

    assert_eq!(server_resp.status(), 200);
    let server_body: serde_json::Value = server_resp
        .json()
        .await
        .expect("Failed to parse server detail response");

    let server_data = &server_body["data"];
    assert_eq!(server_data["cpu_name"], "Intel Xeon E5-2680 v4");
    assert_eq!(server_data["cpu_cores"], 8);
    assert_eq!(server_data["cpu_arch"], "x86_64");
    assert_eq!(server_data["os"], "Ubuntu 22.04");
    assert_eq!(server_data["kernel_version"], "5.15.0-100-generic");
    assert_eq!(server_data["virtualization"], "kvm");
    assert_eq!(server_data["agent_version"], "0.1.0");
    assert_eq!(server_data["ipv4"], "1.2.3.4");

    // Clean up: close the WS connection
    let _ = ws_sink.close().await;
}

#[tokio::test]
async fn test_backup_restore() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    // ── Step 1: Login as admin ──
    login_admin(&client, &base_url).await;

    // ── Step 2: Create a notification (to verify backup contains data) ──
    let create_resp = client
        .post(format!("{}/api/notifications", base_url))
        .json(&json!({
            "name": "Test Webhook",
            "notify_type": "webhook",
            "config_json": {
                "type": "webhook",
                "url": "https://example.com/hook",
                "method": "POST",
                "headers": {},
                "body_template": null
            },
            "enabled": true
        }))
        .send()
        .await
        .expect("Create notification failed");

    assert_eq!(create_resp.status(), 200, "Create notification should succeed");
    let create_body: serde_json::Value = create_resp
        .json()
        .await
        .expect("Failed to parse create notification response");

    let notification_id = create_body["data"]["id"]
        .as_str()
        .expect("notification id missing");

    // Verify the notification exists
    let list_resp = client
        .get(format!("{}/api/notifications", base_url))
        .send()
        .await
        .expect("List notifications failed");

    assert_eq!(list_resp.status(), 200);
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let notifications = list_body["data"].as_array().unwrap();
    assert_eq!(notifications.len(), 1, "Should have 1 notification");

    // ── Step 3: Create backup ──
    let backup_resp = client
        .post(format!("{}/api/settings/backup", base_url))
        .send()
        .await
        .expect("Backup request failed");

    assert_eq!(backup_resp.status(), 200, "Backup should succeed");

    let content_type = backup_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        content_type, "application/octet-stream",
        "Backup should return octet-stream"
    );

    let backup_bytes = backup_resp.bytes().await.expect("Failed to read backup bytes");
    assert!(backup_bytes.len() > 16, "Backup file should not be empty");
    assert_eq!(
        &backup_bytes[..16],
        b"SQLite format 3\0",
        "Backup should be a valid SQLite file"
    );

    // ── Step 4: Delete the notification ──
    let delete_resp = client
        .delete(format!("{}/api/notifications/{}", base_url, notification_id))
        .send()
        .await
        .expect("Delete notification failed");

    assert_eq!(delete_resp.status(), 200, "Delete should succeed");

    // Verify notification is gone
    let list_resp2 = client
        .get(format!("{}/api/notifications", base_url))
        .send()
        .await
        .expect("List notifications failed");

    assert_eq!(list_resp2.status(), 200);
    let list_body2: serde_json::Value = list_resp2.json().await.unwrap();
    let notifications2 = list_body2["data"].as_array().unwrap();
    assert!(
        notifications2.is_empty(),
        "Notifications should be empty after delete"
    );

    // ── Step 5: Restore from backup ──
    let restore_resp = client
        .post(format!("{}/api/settings/restore", base_url))
        .header("content-type", "application/octet-stream")
        .body(backup_bytes)
        .send()
        .await
        .expect("Restore request failed");

    assert_eq!(restore_resp.status(), 200, "Restore should succeed");
    let restore_body: serde_json::Value = restore_resp
        .json()
        .await
        .expect("Failed to parse restore response");

    // The restore endpoint returns a success message indicating restart is needed
    assert!(
        restore_body["data"]
            .as_str()
            .unwrap_or("")
            .contains("restart"),
        "Restore should mention restart"
    );
}
