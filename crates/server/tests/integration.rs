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
    assert_eq!(welcome["protocol_version"], 2);

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

    // Read messages until we get the Ack for SystemInfo (skip ping_tasks_sync etc.)
    let ack = loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws_reader.next())
            .await
            .expect("Timeout waiting for Ack")
            .expect("WebSocket stream ended")
            .expect("WebSocket read error");

        let text = match msg {
            tungstenite::Message::Text(t) => t.to_string(),
            other => panic!("Expected Text message, got: {:?}", other),
        };

        let parsed: serde_json::Value =
            serde_json::from_str(&text).expect("Failed to parse message");
        if parsed["type"] == "ack" {
            break parsed;
        }
    };
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

// ── Task 10: Authentication flow integration tests ────────────────────────────

#[tokio::test]
async fn test_login_logout_flow() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    // Login
    let login_body = login_admin(&client, &base_url).await;
    assert_eq!(login_body["data"]["username"], "admin");

    // GET /api/auth/me → should return 200 while session cookie is active
    let me_resp = client
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .expect("GET /api/auth/me failed");

    assert_eq!(me_resp.status(), 200, "auth/me should return 200 when logged in");
    let me_body: serde_json::Value = me_resp.json().await.unwrap();
    assert_eq!(me_body["data"]["username"], "admin");
    assert_eq!(me_body["data"]["role"], "admin");

    // Logout
    let logout_resp = client
        .post(format!("{}/api/auth/logout", base_url))
        .send()
        .await
        .expect("POST /api/auth/logout failed");

    assert_eq!(logout_resp.status(), 200, "logout should succeed");

    // After logout, GET /api/auth/me should return 401
    let me_after_resp = client
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .expect("GET /api/auth/me after logout failed");

    assert_eq!(
        me_after_resp.status(),
        401,
        "auth/me should return 401 after logout"
    );
}

#[tokio::test]
async fn test_api_key_lifecycle() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    // Login as admin
    login_admin(&client, &base_url).await;

    // Create an API key
    let create_resp = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "test-key" }))
        .send()
        .await
        .expect("POST /api/auth/api-keys failed");

    assert_eq!(create_resp.status(), 200, "API key creation should succeed");
    let create_body: serde_json::Value = create_resp.json().await.unwrap();
    let api_key = create_body["data"]["key"]
        .as_str()
        .expect("key field missing from API key response");
    assert!(api_key.starts_with("serverbee_"), "API key should start with 'serverbee_'");

    // Use the API key (X-API-Key header) to access a protected endpoint with a fresh client
    // (no session cookies — purely API key auth)
    let key_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build key-only HTTP client");

    let servers_resp = key_client
        .get(format!("{}/api/servers", base_url))
        .header("X-API-Key", api_key)
        .send()
        .await
        .expect("GET /api/servers with API key failed");

    assert_eq!(
        servers_resp.status(),
        200,
        "API key should grant access to /api/servers"
    );
    let servers_body: serde_json::Value = servers_resp.json().await.unwrap();
    assert!(
        servers_body["data"].is_array(),
        "data should be an array"
    );
}

#[tokio::test]
async fn test_member_read_only() {
    let (base_url, _tmp) = start_test_server().await;
    let admin_client = http_client();

    // Login as admin and create a member user
    login_admin(&admin_client, &base_url).await;

    let create_resp = admin_client
        .post(format!("{}/api/users", base_url))
        .json(&json!({
            "username": "testmember",
            "password": "memberpass123",
            "role": "member"
        }))
        .send()
        .await
        .expect("POST /api/users failed");

    assert_eq!(create_resp.status(), 200, "Admin should be able to create member");

    // Login as the member user in a separate client
    let member_client = http_client();
    let member_login = member_client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({
            "username": "testmember",
            "password": "memberpass123"
        }))
        .send()
        .await
        .expect("Member login request failed");

    assert_eq!(member_login.status(), 200, "Member login should succeed");

    // Member can do GET /api/servers (read-only route)
    let servers_resp = member_client
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .expect("GET /api/servers as member failed");

    assert_eq!(
        servers_resp.status(),
        200,
        "Member should be able to read /api/servers"
    );

    // Member cannot POST /api/users (admin-only write route)
    let create_user_resp = member_client
        .post(format!("{}/api/users", base_url))
        .json(&json!({
            "username": "anothermember",
            "password": "pass123",
            "role": "member"
        }))
        .send()
        .await
        .expect("POST /api/users as member failed");

    assert_eq!(
        create_user_resp.status(),
        403,
        "Member should receive 403 when attempting to create users"
    );
}

#[tokio::test]
async fn test_public_status_no_auth() {
    let (base_url, _tmp) = start_test_server().await;
    // Use a plain client with NO cookies and NO auth headers
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build plain HTTP client");

    let resp = client
        .get(format!("{}/api/status", base_url))
        .send()
        .await
        .expect("GET /api/status failed");

    assert_eq!(resp.status(), 200, "Public /api/status should be accessible without auth");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"]["servers"].is_array(), "data.servers should be an array");
    assert!(body["data"]["total_count"].is_number(), "data.total_count should be a number");
}

#[tokio::test]
async fn test_audit_log_recorded() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    // Login — this should create an audit log entry with action "login"
    login_admin(&client, &base_url).await;

    // Fetch audit logs
    let audit_resp = client
        .get(format!("{}/api/audit-logs", base_url))
        .send()
        .await
        .expect("GET /api/audit-logs failed");

    assert_eq!(audit_resp.status(), 200, "audit-logs endpoint should be accessible to admin");
    let audit_body: serde_json::Value = audit_resp.json().await.unwrap();
    let entries = audit_body["data"]["entries"]
        .as_array()
        .expect("entries should be an array");
    let total = audit_body["data"]["total"].as_u64().unwrap_or(0);

    assert!(total >= 1, "There should be at least one audit log entry after login");
    assert!(
        entries.iter().any(|e| e["action"].as_str() == Some("login")),
        "Audit log should contain a 'login' entry"
    );
}

// ── Network probe integration tests ──────────────────────────────────────────

#[tokio::test]
async fn test_network_probe_target_crud() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    login_admin(&client, &base_url).await;

    // ── Step 1: GET /api/network-probes/targets — verify 96 preset targets ──
    let list_resp = client
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .expect("GET /api/network-probes/targets failed");

    assert_eq!(list_resp.status(), 200, "list targets should succeed");
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let targets = list_body["data"].as_array().expect("data should be an array");
    assert_eq!(targets.len(), 96, "should have 96 builtin targets");

    // ── Step 2: POST /api/network-probes/targets — create a custom target ──
    let create_resp = client
        .post(format!("{}/api/network-probes/targets", base_url))
        .json(&json!({
            "name": "My Custom Target",
            "provider": "Custom ISP",
            "location": "Test Location",
            "target": "192.168.1.1",
            "probe_type": "icmp"
        }))
        .send()
        .await
        .expect("POST /api/network-probes/targets failed");

    assert_eq!(create_resp.status(), 200, "create target should succeed");
    let create_body: serde_json::Value = create_resp.json().await.unwrap();
    let target_id = create_body["data"]["id"]
        .as_str()
        .expect("target id missing");
    assert_eq!(create_body["data"]["name"], "My Custom Target");

    // ── Step 3: GET /api/network-probes/targets — verify 97 targets ──
    let list_resp2 = client
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .expect("GET /api/network-probes/targets failed");

    assert_eq!(list_resp2.status(), 200);
    let list_body2: serde_json::Value = list_resp2.json().await.unwrap();
    let targets2 = list_body2["data"].as_array().unwrap();
    assert_eq!(targets2.len(), 97, "should have 97 targets after creating custom one");
    assert!(
        targets2.iter().any(|t| t["id"].as_str() == Some(target_id)),
        "Custom target should appear in list"
    );

    // ── Step 4: PUT /api/network-probes/targets/{id} — update the custom target ──
    let update_resp = client
        .put(format!("{}/api/network-probes/targets/{}", base_url, target_id))
        .json(&json!({
            "name": "Updated Custom Target",
            "provider": null,
            "location": null,
            "target": null,
            "probe_type": null
        }))
        .send()
        .await
        .expect("PUT /api/network-probes/targets/{id} failed");

    assert_eq!(update_resp.status(), 200, "update target should succeed");
    let update_body: serde_json::Value = update_resp.json().await.unwrap();
    assert_eq!(update_body["data"]["name"], "Updated Custom Target");
    assert_eq!(update_body["data"]["target"], "192.168.1.1", "target address should be unchanged");

    // ── Step 5: DELETE /api/network-probes/targets/{id} — delete the custom target ──
    let delete_resp = client
        .delete(format!("{}/api/network-probes/targets/{}", base_url, target_id))
        .send()
        .await
        .expect("DELETE /api/network-probes/targets/{id} failed");

    assert_eq!(delete_resp.status(), 200, "delete target should succeed");

    // ── Step 6: GET /api/network-probes/targets — verify back to 96 ──
    let list_resp3 = client
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .expect("GET /api/network-probes/targets failed");

    assert_eq!(list_resp3.status(), 200);
    let list_body3: serde_json::Value = list_resp3.json().await.unwrap();
    let targets3 = list_body3["data"].as_array().unwrap();
    assert_eq!(targets3.len(), 96, "should be back to 96 builtin targets after delete");
    assert!(
        !targets3.iter().any(|t| t["id"].as_str() == Some(target_id)),
        "Deleted target should not appear in list"
    );
}

#[tokio::test]
async fn test_network_probe_setting_crud() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    login_admin(&client, &base_url).await;

    // ── Step 1: GET /api/network-probes/setting — verify defaults ──
    let get_resp = client
        .get(format!("{}/api/network-probes/setting", base_url))
        .send()
        .await
        .expect("GET /api/network-probes/setting failed");

    assert_eq!(get_resp.status(), 200, "get setting should succeed");
    let get_body: serde_json::Value = get_resp.json().await.unwrap();
    assert_eq!(get_body["data"]["interval"], 60, "default interval should be 60");
    assert_eq!(get_body["data"]["packet_count"], 10, "default packet_count should be 10");

    // ── Step 2: PUT /api/network-probes/setting — update interval to 120 ──
    let update_resp = client
        .put(format!("{}/api/network-probes/setting", base_url))
        .json(&json!({
            "interval": 120,
            "packet_count": 10,
            "default_target_ids": []
        }))
        .send()
        .await
        .expect("PUT /api/network-probes/setting failed");

    assert_eq!(update_resp.status(), 200, "update setting should succeed");
    let update_body: serde_json::Value = update_resp.json().await.unwrap();
    assert_eq!(update_body["data"]["interval"], 120, "interval should be updated to 120");

    // ── Step 3: GET /api/network-probes/setting — verify interval=120 ──
    let get_resp2 = client
        .get(format!("{}/api/network-probes/setting", base_url))
        .send()
        .await
        .expect("GET /api/network-probes/setting failed");

    assert_eq!(get_resp2.status(), 200);
    let get_body2: serde_json::Value = get_resp2.json().await.unwrap();
    assert_eq!(get_body2["data"]["interval"], 120, "interval should persist as 120");
    assert_eq!(get_body2["data"]["packet_count"], 10, "packet_count should remain 10");
}

#[tokio::test]
async fn test_network_probe_server_targets() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    login_admin(&client, &base_url).await;

    // ── Step 1: Register an agent to get a server id ──
    let register_resp = client
        .post(format!("{}/api/agent/register", base_url))
        .header("Authorization", "Bearer test-key")
        .send()
        .await
        .expect("Agent register failed");

    assert_eq!(register_resp.status(), 200);
    let register_body: serde_json::Value = register_resp.json().await.unwrap();
    let server_id = register_body["data"]["server_id"]
        .as_str()
        .expect("server_id missing");

    // ── Step 2: Get two builtin target ids ──
    let targets_resp = client
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .expect("GET /api/network-probes/targets failed");

    assert_eq!(targets_resp.status(), 200);
    let targets_body: serde_json::Value = targets_resp.json().await.unwrap();
    let all_targets = targets_body["data"].as_array().unwrap();
    let target_id_1 = all_targets[0]["id"].as_str().unwrap().to_string();
    let target_id_2 = all_targets[1]["id"].as_str().unwrap().to_string();

    // ── Step 3: PUT /api/servers/{id}/network-probes/targets — assign 2 targets ──
    let assign_resp = client
        .put(format!("{}/api/servers/{}/network-probes/targets", base_url, server_id))
        .json(&json!({
            "target_ids": [target_id_1, target_id_2]
        }))
        .send()
        .await
        .expect("PUT /api/servers/{id}/network-probes/targets failed");

    assert_eq!(assign_resp.status(), 200, "assigning targets should succeed");

    // ── Step 4: GET /api/servers/{id}/network-probes/targets — verify 2 targets ──
    let server_targets_resp = client
        .get(format!("{}/api/servers/{}/network-probes/targets", base_url, server_id))
        .send()
        .await
        .expect("GET /api/servers/{id}/network-probes/targets failed");

    assert_eq!(server_targets_resp.status(), 200, "get server targets should succeed");
    let server_targets_body: serde_json::Value = server_targets_resp.json().await.unwrap();
    let server_targets = server_targets_body["data"].as_array().unwrap();
    assert_eq!(server_targets.len(), 2, "server should have 2 assigned targets");

    // ── Step 5: PUT /api/servers/{id}/network-probes/targets — assign 0 targets ──
    let clear_resp = client
        .put(format!("{}/api/servers/{}/network-probes/targets", base_url, server_id))
        .json(&json!({ "target_ids": [] }))
        .send()
        .await
        .expect("PUT /api/servers/{id}/network-probes/targets (clear) failed");

    assert_eq!(clear_resp.status(), 200, "clearing targets should succeed");

    // ── Step 6: GET /api/servers/{id}/network-probes/targets — verify empty ──
    let server_targets_resp2 = client
        .get(format!("{}/api/servers/{}/network-probes/targets", base_url, server_id))
        .send()
        .await
        .expect("GET /api/servers/{id}/network-probes/targets failed");

    assert_eq!(server_targets_resp2.status(), 200);
    let server_targets_body2: serde_json::Value = server_targets_resp2.json().await.unwrap();
    let server_targets2 = server_targets_body2["data"].as_array().unwrap();
    assert!(server_targets2.is_empty(), "server targets should be empty after clearing");
}

#[tokio::test]
async fn test_builtin_target_cannot_be_deleted() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    login_admin(&client, &base_url).await;

    // ── Step 1: Try to DELETE a known preset target id ──
    let preset_id = "cn-bj-ct";

    let delete_resp = client
        .delete(format!("{}/api/network-probes/targets/{}", base_url, preset_id))
        .send()
        .await
        .expect("DELETE /api/network-probes/targets/{id} failed");

    assert!(
        delete_resp.status() == 400 || delete_resp.status() == 403,
        "Deleting a builtin target should return 400 or 403, got {}",
        delete_resp.status()
    );

    // ── Step 2: Verify preset target still exists ──
    let list_resp2 = client
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .expect("GET /api/network-probes/targets failed");

    assert_eq!(list_resp2.status(), 200);
    let list_body2: serde_json::Value = list_resp2.json().await.unwrap();
    let targets2 = list_body2["data"].as_array().unwrap();
    assert_eq!(targets2.len(), 96, "preset targets should remain 96 after failed delete");
    assert!(
        targets2.iter().any(|t| t["id"].as_str() == Some(preset_id)),
        "Preset target should still be present after failed delete"
    );
}

#[tokio::test]
async fn test_preset_target_source_field() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // ── Step 1: Verify preset targets have source field ──
    let list_resp = client
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .expect("GET targets failed");

    let body: serde_json::Value = list_resp.json().await.unwrap();
    let targets = body["data"].as_array().unwrap();

    // Find a known preset target
    let preset = targets.iter().find(|t| t["id"] == "cn-bj-ct").unwrap();
    assert_eq!(preset["source"], "preset:china-telecom");
    assert_eq!(preset["source_name"], "中国电信");
    assert!(preset["created_at"].is_null());

    let intl = targets.iter().find(|t| t["id"] == "intl-cloudflare").unwrap();
    assert_eq!(intl["source"], "preset:international");
    assert_eq!(intl["source_name"], "国际节点");

    // ── Step 2: Create a custom target and verify no source ──
    let create_resp = client
        .post(format!("{}/api/network-probes/targets", base_url))
        .json(&serde_json::json!({
            "name": "Custom Test",
            "provider": "Test",
            "location": "Test",
            "target": "10.0.0.1",
            "probe_type": "tcp"
        }))
        .send()
        .await
        .expect("POST targets failed");

    let create_body: serde_json::Value = create_resp.json().await.unwrap();

    // Verify custom target via list (create returns Model, not TargetDto)
    let list_resp2 = client
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .expect("GET targets failed");

    let body2: serde_json::Value = list_resp2.json().await.unwrap();
    let targets2 = body2["data"].as_array().unwrap();
    let custom_id = create_body["data"]["id"].as_str().unwrap();
    let custom = targets2
        .iter()
        .find(|t| t["id"].as_str() == Some(custom_id))
        .unwrap();
    assert!(custom["source"].is_null());
    assert!(custom["source_name"].is_null());
    assert!(!custom["created_at"].is_null());

    // ── Step 3: Cleanup ──
    client
        .delete(format!(
            "{}/api/network-probes/targets/{}",
            base_url, custom_id
        ))
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_preset_target_cannot_be_updated() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let update_resp = client
        .put(format!("{}/api/network-probes/targets/cn-bj-ct", base_url))
        .json(&serde_json::json!({
            "name": "Hacked",
            "provider": null,
            "location": null,
            "target": null,
            "probe_type": null
        }))
        .send()
        .await
        .expect("PUT preset target failed");

    assert_eq!(
        update_resp.status(),
        403,
        "Updating a preset target should return 403"
    );
}

// ── Task 11: CRUD integration tests ──────────────────────────────────────────

#[tokio::test]
async fn test_notification_and_alert_crud() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    login_admin(&client, &base_url).await;

    // ── Create notification channel ──
    let notif_resp = client
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
        .expect("POST /api/notifications failed");

    assert_eq!(notif_resp.status(), 200, "notification creation should succeed");
    let notif_body: serde_json::Value = notif_resp.json().await.unwrap();
    let notif_id = notif_body["data"]["id"].as_str().expect("notification id missing");

    // ── Create notification group ──
    let group_resp = client
        .post(format!("{}/api/notification-groups", base_url))
        .json(&json!({
            "name": "Test Group",
            "notification_ids": [notif_id]
        }))
        .send()
        .await
        .expect("POST /api/notification-groups failed");

    assert_eq!(group_resp.status(), 200, "notification group creation should succeed");
    let group_body: serde_json::Value = group_resp.json().await.unwrap();
    let group_id = group_body["data"]["id"].as_str().expect("group id missing");

    // ── Create alert rule ──
    let alert_resp = client
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "High CPU Alert",
            "rules": [
                {
                    "rule_type": "cpu",
                    "min": 90.0
                }
            ],
            "trigger_mode": "once",
            "notification_group_id": group_id,
            "cover_type": "all",
            "enabled": true
        }))
        .send()
        .await
        .expect("POST /api/alert-rules failed");

    assert_eq!(alert_resp.status(), 200, "alert rule creation should succeed");
    let alert_body: serde_json::Value = alert_resp.json().await.unwrap();
    let alert_id = alert_body["data"]["id"].as_str().expect("alert id missing");
    assert_eq!(alert_body["data"]["name"], "High CPU Alert");

    // ── List alert rules — verify the rule appears ──
    let list_resp = client
        .get(format!("{}/api/alert-rules", base_url))
        .send()
        .await
        .expect("GET /api/alert-rules failed");

    assert_eq!(list_resp.status(), 200);
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let rules = list_body["data"].as_array().expect("data should be array");
    assert!(
        rules.iter().any(|r| r["id"].as_str() == Some(alert_id)),
        "Created alert rule should appear in list"
    );

    // ── Delete alert rule ──
    let delete_resp = client
        .delete(format!("{}/api/alert-rules/{}", base_url, alert_id))
        .send()
        .await
        .expect("DELETE /api/alert-rules/{id} failed");

    assert_eq!(delete_resp.status(), 200, "alert rule deletion should succeed");

    // Verify it's gone
    let list_after_resp = client
        .get(format!("{}/api/alert-rules", base_url))
        .send()
        .await
        .expect("GET /api/alert-rules after delete failed");

    let list_after_body: serde_json::Value = list_after_resp.json().await.unwrap();
    let rules_after = list_after_body["data"].as_array().unwrap();
    assert!(
        !rules_after.iter().any(|r| r["id"].as_str() == Some(alert_id)),
        "Deleted alert rule should not appear in list"
    );
}

#[tokio::test]
async fn test_user_management_crud() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    login_admin(&client, &base_url).await;

    // ── Create user ──
    let create_resp = client
        .post(format!("{}/api/users", base_url))
        .json(&json!({
            "username": "crudusr",
            "password": "crudpass123",
            "role": "member"
        }))
        .send()
        .await
        .expect("POST /api/users failed");

    assert_eq!(create_resp.status(), 200, "user creation should succeed");
    let create_body: serde_json::Value = create_resp.json().await.unwrap();
    let user_id = create_body["data"]["id"].as_str().expect("user id missing");
    assert_eq!(create_body["data"]["role"], "member");

    // ── List users — verify the new user appears ──
    let list_resp = client
        .get(format!("{}/api/users", base_url))
        .send()
        .await
        .expect("GET /api/users failed");

    assert_eq!(list_resp.status(), 200);
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let users = list_body["data"].as_array().expect("data should be array");
    assert!(
        users.iter().any(|u| u["id"].as_str() == Some(user_id)),
        "Newly created user should appear in user list"
    );

    // ── Update role to admin ──
    let update_resp = client
        .put(format!("{}/api/users/{}", base_url, user_id))
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .expect("PUT /api/users/{id} failed");

    assert_eq!(update_resp.status(), 200, "user role update should succeed");
    let update_body: serde_json::Value = update_resp.json().await.unwrap();
    assert_eq!(update_body["data"]["role"], "admin", "Role should be updated to admin");

    // ── Delete user ──
    let delete_resp = client
        .delete(format!("{}/api/users/{}", base_url, user_id))
        .send()
        .await
        .expect("DELETE /api/users/{id} failed");

    assert_eq!(delete_resp.status(), 200, "user deletion should succeed");

    // Verify user is gone
    let list_after_resp = client
        .get(format!("{}/api/users", base_url))
        .send()
        .await
        .expect("GET /api/users after delete failed");

    let list_after_body: serde_json::Value = list_after_resp.json().await.unwrap();
    let users_after = list_after_body["data"].as_array().unwrap();
    assert!(
        !users_after.iter().any(|u| u["id"].as_str() == Some(user_id)),
        "Deleted user should not appear in user list"
    );
}

#[tokio::test]
async fn test_settings_auto_discovery_key() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    login_admin(&client, &base_url).await;

    // ── GET current auto-discovery key ──
    let get_resp = client
        .get(format!("{}/api/settings/auto-discovery-key", base_url))
        .send()
        .await
        .expect("GET /api/settings/auto-discovery-key failed");

    assert_eq!(get_resp.status(), 200, "GET auto-discovery-key should succeed");
    let get_body: serde_json::Value = get_resp.json().await.unwrap();
    let original_key = get_body["data"]["key"]
        .as_str()
        .expect("key field missing")
        .to_string();
    assert!(!original_key.is_empty(), "Auto-discovery key should not be empty");

    // ── PUT to regenerate the key ──
    let regen_resp = client
        .put(format!("{}/api/settings/auto-discovery-key", base_url))
        .send()
        .await
        .expect("PUT /api/settings/auto-discovery-key failed");

    assert_eq!(regen_resp.status(), 200, "Regenerate auto-discovery-key should succeed");
    let regen_body: serde_json::Value = regen_resp.json().await.unwrap();
    let new_key = regen_body["data"]["key"]
        .as_str()
        .expect("key field missing after regeneration")
        .to_string();

    assert!(!new_key.is_empty(), "New auto-discovery key should not be empty");
    assert_ne!(
        original_key, new_key,
        "Regenerated key should differ from the original key"
    );
}

#[tokio::test]
async fn test_alert_states_endpoint() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create an alert rule
    let resp = client
        .post(format!("{base_url}/api/alert-rules"))
        .json(&serde_json::json!({
            "name": "Test States",
            "rules": [{"rule_type": "cpu", "min": 1.0}],
            "cover_type": "all",
            "trigger_mode": "always"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let rule_id = body["data"]["id"].as_str().unwrap();

    // Query states (should be empty initially)
    let resp = client
        .get(format!("{base_url}/api/alert-rules/{rule_id}/states"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let states = body["data"].as_array().unwrap();
    assert!(states.is_empty());

    // Cleanup
    client
        .delete(format!("{base_url}/api/alert-rules/{rule_id}"))
        .send()
        .await
        .unwrap();
}
