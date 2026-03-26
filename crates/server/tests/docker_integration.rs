use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use serverbee_common::constants::{CAP_DEFAULT, CAP_DOCKER};
use serverbee_server::config::{AdminConfig, AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::service::config::ConfigService;
use serverbee_server::state::AppState;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

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
        .unwrap();
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .unwrap();

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

fn http_client() -> Client {
    Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}

async fn login_admin(client: &Client, base_url: &str) {
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

async fn create_api_key(client: &Client, base_url: &str) -> String {
    let resp = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "docker-test-key" }))
        .send()
        .await
        .expect("POST /api/auth/api-keys failed");

    assert_eq!(resp.status(), 200, "API key creation should succeed");
    let body: serde_json::Value = resp.json().await.expect("Failed to parse API key response");
    body["data"]["key"]
        .as_str()
        .expect("API key missing")
        .to_string()
}

async fn register_agent(client: &Client, base_url: &str) -> (String, String) {
    let resp = client
        .post(format!("{}/api/agent/register", base_url))
        .header("Authorization", "Bearer test-key")
        .send()
        .await
        .expect("Register request failed");

    assert_eq!(resp.status(), 200, "Agent registration should succeed");
    let body: serde_json::Value = resp
        .json()
        .await
        .expect("Failed to parse register response");

    (
        body["data"]["server_id"]
            .as_str()
            .expect("server_id missing")
            .to_string(),
        body["data"]["token"]
            .as_str()
            .expect("token missing")
            .to_string(),
    )
}

async fn connect_agent(
    base_url: &str,
    token: &str,
) -> (
    futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tungstenite::Message,
    >,
    futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) {
    let ws_url = format!(
        "{}/api/agent/ws?token={}",
        base_url.replace("http://", "ws://"),
        token
    );
    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("Agent WebSocket connection failed");
    ws_stream.split()
}

async fn recv_text(
    reader: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> serde_json::Value {
    let msg = tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .expect("Timeout waiting for websocket message")
        .expect("WebSocket stream ended")
        .expect("WebSocket read error");

    let text = match msg {
        tungstenite::Message::Text(t) => t.to_string(),
        other => panic!("Expected Text message, got: {:?}", other),
    };

    serde_json::from_str(&text).expect("Failed to parse websocket message")
}

async fn send_docker_system_info(
    sink: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tungstenite::Message,
    >,
    reader: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) {
    let system_info = json!({
        "type": "system_info",
        "msg_id": "docker-system-info",
        "cpu_name": "Intel Xeon",
        "cpu_cores": 8,
        "cpu_arch": "x86_64",
        "os": "Ubuntu 22.04",
        "kernel_version": "6.8.0",
        "mem_total": 16_000_000_000_i64,
        "swap_total": 4_000_000_000_i64,
        "disk_total": 100_000_000_000_i64,
        "ipv4": "1.2.3.4",
        "ipv6": null,
        "virtualization": "kvm",
        "agent_version": "0.5.0",
        "protocol_version": 3,
        "features": ["docker"]
    });

    sink.send(tungstenite::Message::Text(system_info.to_string().into()))
        .await
        .expect("Failed to send SystemInfo");

    loop {
        let msg = recv_text(reader).await;
        if msg["type"] == "ack" {
            assert_eq!(msg["msg_id"], "docker-system-info");
            break;
        }
    }
}

async fn enable_docker_capability(client: &Client, base_url: &str, server_id: &str) {
    let resp = client
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({
            "capabilities": (CAP_DEFAULT | CAP_DOCKER) as i32
        }))
        .send()
        .await
        .expect("PUT /api/servers/{id} failed");

    assert_eq!(resp.status(), 200, "Server update should succeed");
}

async fn recv_until_types(
    reader: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    expected: &[&str],
) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut seen = Vec::new();

    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Some(Ok(tungstenite::Message::Text(text)))) =
            tokio::time::timeout(remaining, reader.next()).await
        else {
            break;
        };
        let parsed: serde_json::Value =
            serde_json::from_str(&text).expect("Failed to parse server message");
        let Some(msg_type) = parsed["type"].as_str() else {
            continue;
        };
        if expected.contains(&msg_type) && !seen.iter().any(|s| s == msg_type) {
            seen.push(msg_type.to_string());
        }
        if seen.len() == expected.len() {
            break;
        }
    }

    seen
}

#[tokio::test]
async fn test_docker_info_endpoint_requests_agent_when_cache_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;

    let welcome = recv_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");

    send_docker_system_info(&mut agent_sink, &mut agent_reader).await;
    enable_docker_capability(&client, &base_url, &server_id).await;

    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_text(&mut agent_reader).await;
            match msg["type"].as_str() {
                Some("docker_get_info") => {
                    let response = json!({
                        "type": "docker_info",
                        "msg_id": msg["msg_id"].as_str().expect("docker_get_info msg_id missing"),
                        "info": {
                            "docker_version": "27.1.1",
                            "api_version": "1.46",
                            "os": "linux",
                            "arch": "x86_64",
                            "containers_running": 3,
                            "containers_paused": 0,
                            "containers_stopped": 1,
                            "images": 9,
                            "memory_total": 8_000_000_000_u64
                        }
                    });
                    agent_sink
                        .send(tungstenite::Message::Text(response.to_string().into()))
                        .await
                        .expect("Failed to send DockerInfo response");
                    return;
                }
                Some("capabilities_sync")
                | Some("ping_tasks_sync")
                | Some("network_probe_sync") => {}
                Some(other) => panic!("Unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let resp = client
        .get(format!(
            "{}/api/servers/{}/docker/info",
            base_url, server_id
        ))
        .send()
        .await
        .expect("GET /api/servers/{id}/docker/info failed");

    assert_eq!(
        resp.status(),
        200,
        "Docker info endpoint should fetch from the agent"
    );
    let body: serde_json::Value = resp
        .json()
        .await
        .expect("Failed to parse docker info response");
    assert_eq!(body["data"]["info"]["docker_version"], "27.1.1");

    agent_task.await.expect("Agent task failed");
}

#[tokio::test]
async fn test_docker_streams_restart_for_existing_browser_viewers_after_agent_reconnect() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;

    let welcome = recv_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");

    send_docker_system_info(&mut agent_sink, &mut agent_reader).await;
    enable_docker_capability(&client, &base_url, &server_id).await;

    let mut request = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"))
        .into_client_request()
        .expect("Failed to build browser websocket request");
    request
        .headers_mut()
        .insert("x-api-key", HeaderValue::from_str(&api_key).unwrap());

    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("Browser WebSocket connection failed");
    let (mut browser_sink, mut browser_reader) = browser_ws.split();

    let full_sync = recv_text(&mut browser_reader).await;
    assert_eq!(full_sync["type"], "full_sync");

    browser_sink
        .send(tungstenite::Message::Text(
            json!({
                "type": "docker_subscribe",
                "server_id": server_id
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("Failed to send docker_subscribe");

    let first_seen = recv_until_types(
        &mut agent_reader,
        &["docker_start_stats", "docker_events_start"],
    )
    .await;
    assert_eq!(
        first_seen.len(),
        2,
        "First subscription should start docker streams"
    );

    agent_sink
        .close()
        .await
        .expect("Failed to close first agent");

    let (mut agent_sink2, mut agent_reader2) = connect_agent(&base_url, &token).await;
    let welcome2 = recv_text(&mut agent_reader2).await;
    assert_eq!(welcome2["type"], "welcome");

    send_docker_system_info(&mut agent_sink2, &mut agent_reader2).await;

    let resumed_seen = recv_until_types(
        &mut agent_reader2,
        &["docker_start_stats", "docker_events_start"],
    )
    .await;
    assert_eq!(
        resumed_seen.len(),
        2,
        "Existing browser viewers should resume docker streams after agent reconnect"
    );

    let _ = browser_sink.close().await;
    let _ = agent_sink2.close().await;
}

#[tokio::test]
async fn test_docker_subscribe_requires_capability_and_feature() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url).await;

    let (_server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;

    let welcome = recv_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");

    send_docker_system_info(&mut agent_sink, &mut agent_reader).await;

    let mut request = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"))
        .into_client_request()
        .expect("Failed to build browser websocket request");
    request
        .headers_mut()
        .insert("x-api-key", HeaderValue::from_str(&api_key).unwrap());

    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("Browser WebSocket connection failed");
    let (mut browser_sink, mut browser_reader) = browser_ws.split();

    let full_sync = recv_text(&mut browser_reader).await;
    assert_eq!(full_sync["type"], "full_sync");

    let server_id = full_sync["servers"][0]["id"]
        .as_str()
        .expect("server id missing from full sync");

    browser_sink
        .send(tungstenite::Message::Text(
            json!({
                "type": "docker_subscribe",
                "server_id": server_id
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("Failed to send docker_subscribe");

    let seen = recv_until_types(
        &mut agent_reader,
        &["docker_start_stats", "docker_events_start"],
    )
    .await;
    assert!(
        seen.is_empty(),
        "Docker subscribe should be ignored when CAP_DOCKER is disabled, saw {:?}",
        seen
    );

    let _ = browser_sink.close().await;
    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_unavailable_fails_pending_request_and_clears_feature_state() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;

    let welcome = recv_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");

    send_docker_system_info(&mut agent_sink, &mut agent_reader).await;
    enable_docker_capability(&client, &base_url, &server_id).await;

    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_text(&mut agent_reader).await;
            match msg["type"].as_str() {
                Some("docker_get_info") => {
                    let response = json!({
                        "type": "docker_unavailable",
                        "msg_id": msg["msg_id"].as_str().expect("docker_get_info msg_id missing")
                    });
                    agent_sink
                        .send(tungstenite::Message::Text(response.to_string().into()))
                        .await
                        .expect("Failed to send DockerUnavailable response");
                    return;
                }
                Some("capabilities_sync")
                | Some("ping_tasks_sync")
                | Some("network_probe_sync") => {}
                Some(other) => panic!("Unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let resp = tokio::time::timeout(
        Duration::from_secs(2),
        client
            .get(format!(
                "{}/api/servers/{}/docker/info",
                base_url, server_id
            ))
            .send(),
    )
    .await
    .expect("GET /api/servers/{id}/docker/info should not wait for the request timeout")
    .expect("GET /api/servers/{id}/docker/info failed");

    assert_eq!(
        resp.status(),
        403,
        "Docker unavailable should return 403 immediately"
    );

    let server_resp = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("GET /api/servers/{id} failed");
    assert_eq!(server_resp.status(), 200);
    let server_body: serde_json::Value = server_resp
        .json()
        .await
        .expect("Failed to parse server response");
    let features = server_body["data"]["features"]
        .as_array()
        .expect("features should be an array");
    assert!(
        !features
            .iter()
            .any(|value| value.as_str() == Some("docker")),
        "Docker feature should be removed from persisted server state after DockerUnavailable"
    );

    let second_resp = tokio::time::timeout(
        Duration::from_secs(2),
        client
            .get(format!(
                "{}/api/servers/{}/docker/info",
                base_url, server_id
            ))
            .send(),
    )
    .await
    .expect("Subsequent GET /api/servers/{id}/docker/info should fail immediately")
    .expect("Second GET /api/servers/{id}/docker/info failed");
    assert_eq!(
        second_resp.status(),
        403,
        "Docker should remain unavailable in the in-memory feature state"
    );

    agent_task.await.expect("Agent task failed");
}
