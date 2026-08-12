//! Integration tests for the browser update WebSocket `/api/ws/servers`.
//!
//! Source under test: `crates/server/src/router/ws/browser.rs`.
//!
//! Coverage:
//! - Happy path: an authenticated client (x-api-key header) connects and
//!   receives the initial `full_sync` frame.
//! - Report projection: a `full_sync` built while an agent has a cached report
//!   carries that report's metrics (including summed disk I/O), not the
//!   all-zero fallback.
//! - State change: after a browser is connected, registering + connecting a
//!   mock agent broadcasts a `server_online` frame that the browser observes.
//! - Auth paths: session-cookie auth and member API-key auth both connect.
//! - AuthZ/rejection: no key and an invalid key are rejected at the WS
//!   handshake (HTTP 401, surfaced as `tungstenite::Error::Http`).
//!
//! The endpoint authenticates via session cookie, `x-api-key`, or Bearer
//! token. There is no admin-only gate on the connect itself — both admins and
//! members may open the stream (the role only affects server-side filtering,
//! which is currently a no-op). The admin-only surface tested here is API-key
//! *creation* (`POST /api/auth/api-keys`), not the WS connect.

mod common;

use common::{
    connect_agent, create_server, http_client, login_admin, recv_agent_text, register_agent,
    send_system_info, start_test_server,
};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

/// Read half of a connected browser WebSocket.
type BrowserReader = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Create an API key as an already-authenticated admin and return the raw key
/// (the only time the plaintext `serverbee_...` value is exposed).
async fn create_api_key(client: &reqwest::Client, base_url: &str, name: &str) -> String {
    let resp = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .expect("POST /api/auth/api-keys failed");
    assert_eq!(resp.status(), 200, "API key creation should succeed");
    let body: Value = resp.json().await.expect("parse api-key response");
    body["data"]["key"]
        .as_str()
        .expect("api key missing from response")
        .to_string()
}

/// Build an `/api/ws/servers` client request carrying the given `x-api-key`.
fn browser_ws_request_with_key(base_url: &str, api_key: &str) -> tungstenite::handshake::client::Request {
    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut request = ws_url
        .into_client_request()
        .expect("browser ws request should build");
    request.headers_mut().insert(
        "x-api-key",
        HeaderValue::from_str(api_key).expect("api key header should be valid"),
    );
    request
}

/// Receive the next text frame from the browser socket, parsed as JSON (5s timeout).
async fn recv_browser_text(reader: &mut BrowserReader) -> Value {
    let message = tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .expect("timed out waiting for browser message")
        .expect("browser WebSocket stream ended")
        .expect("browser WebSocket read error");
    match message {
        tungstenite::Message::Text(text) => {
            serde_json::from_str(&text).expect("parse browser message")
        }
        other => panic!("expected Text message, got: {:?}", other),
    }
}

// ── Happy path ────────────────────────────────────────────────────────────

#[tokio::test]
async fn browser_ws_sends_full_sync_on_connect_with_api_key() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "browser-ws-key").await;

    // Seed one server so the full_sync carries a known entry.
    let server_id = create_server(&client, &base_url, "fullsync-target").await;

    let request = browser_ws_request_with_key(&base_url, &api_key);
    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("browser WebSocket connection should succeed with a valid api key");
    let (_sink, mut reader) = browser_ws.split();

    let full_sync = recv_browser_text(&mut reader).await;
    assert_eq!(full_sync["type"], "full_sync", "first frame should be full_sync");

    let servers = full_sync["servers"]
        .as_array()
        .expect("full_sync.servers should be an array");
    assert!(
        servers
            .iter()
            .any(|s| s["id"].as_str() == Some(server_id.as_str())),
        "full_sync should include the seeded server {server_id}"
    );
    // Newly created server has no live agent → reported offline.
    let seeded = servers
        .iter()
        .find(|s| s["id"].as_str() == Some(server_id.as_str()))
        .expect("seeded server present");
    assert_eq!(seeded["online"], false, "seeded server should be offline");
    assert!(
        full_sync["upgrades"].is_array(),
        "full_sync should carry an upgrades array"
    );
}

#[tokio::test]
async fn browser_ws_full_sync_carries_latest_agent_report() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "fullsync-report-key").await;
    let (server_id, token) = register_agent(&client, &base_url).await;

    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut agent_reader).await["type"], "welcome");
    send_system_info(&mut agent_sink, &mut agent_reader, "fullsync-report", None).await;

    // `Report` is a newtype variant: the SystemReport fields sit alongside the
    // "report" tag. Every metric is non-zero so a full_sync built from the
    // all-zero fallback branch cannot pass this test.
    let report = serde_json::json!({
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
        "uptime": 86_400_u64,
        "disk_io": [
            { "name": "sda", "read_bytes_per_sec": 1_000_u64, "write_bytes_per_sec": 2_000_u64 },
            { "name": "sdb", "read_bytes_per_sec": 500_u64, "write_bytes_per_sec": 250_u64 }
        ],
        "temperature": 55.0,
        "gpu": null
    });
    agent_sink
        .send(tungstenite::Message::Text(report.to_string().into()))
        .await
        .expect("send report");

    // The agent read loop handles frames in order, so an Ack for a handshake
    // sent *after* the report proves the report already reached the cache.
    send_system_info(
        &mut agent_sink,
        &mut agent_reader,
        "post-report-handshake",
        None,
    )
    .await;

    let request = browser_ws_request_with_key(&base_url, &api_key);
    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("browser WebSocket connection should succeed");
    let (_sink, mut reader) = browser_ws.split();
    let full_sync = recv_browser_text(&mut reader).await;
    assert_eq!(full_sync["type"], "full_sync");

    let entry = full_sync["servers"]
        .as_array()
        .expect("full_sync.servers should be an array")
        .iter()
        .find(|s| s["id"].as_str() == Some(server_id.as_str()))
        .expect("full_sync should include the connected agent's server")
        .clone();

    assert_eq!(entry["online"], true, "the agent socket is still open");
    assert_eq!(entry["cpu"], 45.5);
    assert_eq!(entry["mem_used"], 8_000_000_000_i64);
    assert_eq!(entry["swap_used"], 500_000_000_i64);
    assert_eq!(entry["disk_used"], 30_000_000_000_i64);
    assert_eq!(entry["net_in_speed"], 1_000_000_i64);
    assert_eq!(entry["net_out_speed"], 500_000_i64);
    assert_eq!(entry["net_in_transfer"], 10_000_000_000_i64);
    assert_eq!(entry["net_out_transfer"], 5_000_000_000_i64);
    assert_eq!(entry["load1"], 1.5);
    assert_eq!(entry["load5"], 1.2);
    assert_eq!(entry["load15"], 0.8);
    assert_eq!(entry["tcp_conn"], 42);
    assert_eq!(entry["udp_conn"], 5);
    assert_eq!(entry["process_count"], 120);
    assert_eq!(entry["uptime"], 86_400_u64);
    // Disk I/O is summed across devices before it reaches the browser.
    assert_eq!(entry["disk_read_bytes_per_sec"], 1_500_u64);
    assert_eq!(entry["disk_write_bytes_per_sec"], 2_250_u64);

    let _ = agent_sink.close().await;
}

// ── State change → server_online broadcast ────────────────────────────────

#[tokio::test]
async fn browser_ws_receives_server_online_when_agent_connects() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "online-watch-key").await;

    // Pre-register an agent (server row exists, but no live connection yet).
    let (server_id, token) = register_agent(&client, &base_url).await;

    // Connect the browser FIRST so it is subscribed to the broadcast channel
    // before the agent comes online.
    let request = browser_ws_request_with_key(&base_url, &api_key);
    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("browser WebSocket connection should succeed");
    let (_browser_sink, mut browser_reader) = browser_ws.split();

    let full_sync = recv_browser_text(&mut browser_reader).await;
    assert_eq!(full_sync["type"], "full_sync");

    // Bring the agent online — `add_connection` broadcasts ServerOnline.
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_system_info(&mut agent_sink, &mut agent_reader, "online-watch-msg", None).await;

    // Drain the browser stream until the matching server_online arrives. Other
    // broadcasts (e.g. update) may interleave; we only require server_online.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut saw_online = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Some(Ok(tungstenite::Message::Text(text)))) =
            tokio::time::timeout(remaining, browser_reader.next()).await
        else {
            break;
        };
        let parsed: Value = serde_json::from_str(&text).expect("parse browser frame");
        if parsed["type"] == "server_online" && parsed["server_id"] == server_id {
            saw_online = true;
            break;
        }
    }
    assert!(
        saw_online,
        "browser should observe a server_online frame for {server_id} after the agent connects"
    );

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn browser_ws_ignores_non_command_frames_and_keeps_forwarding() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "browser-ws-tolerance-key").await;
    let (server_id, token) = register_agent(&client, &base_url).await;

    let request = browser_ws_request_with_key(&base_url, &api_key);
    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("browser WebSocket connection should succeed");
    let (mut browser_sink, mut browser_reader) = browser_ws.split();
    assert_eq!(
        recv_browser_text(&mut browser_reader).await["type"],
        "full_sync"
    );

    browser_sink
        .send(tungstenite::Message::Text("{not-json".into()))
        .await
        .expect("send malformed browser frame");
    browser_sink
        .send(tungstenite::Message::Binary(vec![1, 2, 3].into()))
        .await
        .expect("send binary browser frame");
    browser_sink
        .send(tungstenite::Message::Ping(vec![4, 5, 6].into()))
        .await
        .expect("send browser ping");

    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut agent_reader).await["type"], "welcome");
    send_system_info(
        &mut agent_sink,
        &mut agent_reader,
        "tolerant-browser-msg",
        None,
    )
    .await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut saw_online = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Some(Ok(frame))) = tokio::time::timeout(remaining, browser_reader.next()).await
        else {
            break;
        };
        let tungstenite::Message::Text(text) = frame else {
            continue;
        };
        let parsed: Value = serde_json::from_str(&text).expect("parse browser frame");
        if parsed["type"] == "server_online" && parsed["server_id"] == server_id {
            saw_online = true;
            break;
        }
    }

    assert!(
        saw_online,
        "malformed, binary, and ping frames must not stop broadcast forwarding"
    );
    let _ = browser_sink.close().await;
    let _ = agent_sink.close().await;
}

// ── Session-cookie auth path ──────────────────────────────────────────────

#[tokio::test]
async fn browser_ws_connects_with_session_cookie() {
    let (base_url, _tmp) = start_test_server().await;

    // Log in through a raw request so we can read the Set-Cookie value directly
    // (reqwest's cookie store is opaque) and replay it on the WS handshake.
    let raw = http_client();
    let resp = raw
        .post(format!("{}/api/auth/login", base_url))
        .json(&serde_json::json!({ "username": "admin", "password": "testpass" }))
        .send()
        .await
        .expect("raw login failed");
    assert_eq!(resp.status(), 200, "admin login should succeed");
    let cookie_header = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|c| c.split(';').next().map(|s| s.trim().to_string()))
        .expect("login should set a session cookie");
    assert!(
        cookie_header.starts_with("session_token="),
        "expected session_token cookie, got {cookie_header}"
    );

    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut request = ws_url
        .into_client_request()
        .expect("browser ws request should build");
    request.headers_mut().insert(
        "Cookie",
        HeaderValue::from_str(&cookie_header).expect("cookie header should be valid"),
    );

    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("browser WebSocket should accept a valid session cookie");
    let (_sink, mut reader) = browser_ws.split();

    let full_sync = recv_browser_text(&mut reader).await;
    assert_eq!(
        full_sync["type"], "full_sync",
        "cookie-authenticated client should receive full_sync"
    );
}

#[tokio::test]
async fn browser_ws_closes_after_session_logout() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    let login = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&serde_json::json!({ "username": "admin", "password": "testpass" }))
        .send()
        .await
        .expect("login failed");
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("login should set a cookie")
        .to_str()
        .expect("cookie should be valid text")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string();

    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut request = ws_url.into_client_request().expect("WS request");
    request.headers_mut().insert(
        "Cookie",
        HeaderValue::from_str(&cookie).expect("cookie header"),
    );
    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("browser WebSocket connection");
    let (_sink, mut reader) = browser_ws.split();
    assert_eq!(recv_browser_text(&mut reader).await["type"], "full_sync");

    let logout = client
        .post(format!("{}/api/auth/logout", base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("logout failed");
    assert_eq!(logout.status(), 200);

    let message = tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .expect("revoked WebSocket should close promptly")
        .expect("server should send a close frame")
        .expect("WebSocket read should succeed");
    assert!(
        matches!(message, tungstenite::Message::Close(_)),
        "expected close frame after logout, got {message:?}"
    );
}

// ── Member API key authenticates (read-only, not admin-gated) ─────────────

#[tokio::test]
async fn browser_ws_connects_with_member_session_cookie() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // API-key creation is admin-only, so a member authenticates the (non-admin-
    // gated) browser WS via their browser session cookie instead. Create the
    // member, log them in, and capture the session cookie from the response.
    admin
        .post(format!("{}/api/users", base_url))
        .json(&serde_json::json!({ "username": "ws-member", "password": "memberpass", "role": "member" }))
        .send()
        .await
        .expect("create member failed");

    let member = http_client();
    let login = member
        .post(format!("{}/api/auth/login", base_url))
        .json(&serde_json::json!({ "username": "ws-member", "password": "memberpass" }))
        .send()
        .await
        .expect("member login failed");
    assert_eq!(login.status(), 200, "member login should succeed");
    let set_cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("login should set a session cookie")
        .to_str()
        .unwrap()
        .to_string();
    // The "name=value" prefix before the first ';' is what a Cookie header needs.
    let cookie_pair = set_cookie.split(';').next().unwrap().to_string();

    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut request = ws_url.into_client_request().expect("ws request should build");
    request.headers_mut().insert(
        "cookie",
        HeaderValue::from_str(&cookie_pair).expect("cookie header should be valid"),
    );

    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("member session cookie should be accepted on the browser WS");
    let (_sink, mut reader) = browser_ws.split();

    let full_sync = recv_browser_text(&mut reader).await;
    assert_eq!(
        full_sync["type"], "full_sync",
        "member-authenticated client should receive full_sync"
    );
}

// ── AuthZ: unauthenticated and invalid-key connects are rejected ──────────

#[tokio::test]
async fn browser_ws_rejects_connect_without_credentials() {
    let (base_url, _tmp) = start_test_server().await;

    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let request = ws_url
        .into_client_request()
        .expect("browser ws request should build");

    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("browser WebSocket should reject an unauthenticated connect");
    match err {
        tungstenite::Error::Http(resp) => {
            assert_eq!(
                resp.status(),
                401,
                "unauthenticated browser WS connect should fail with 401"
            );
        }
        other => panic!("expected an HTTP 401 handshake failure, got {other:?}"),
    }
}

#[tokio::test]
async fn browser_ws_rejects_connect_with_invalid_api_key() {
    let (base_url, _tmp) = start_test_server().await;

    let request = browser_ws_request_with_key(&base_url, "serverbee_not_a_real_key");
    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("browser WebSocket should reject an invalid api key");
    match err {
        tungstenite::Error::Http(resp) => {
            assert_eq!(
                resp.status(),
                401,
                "invalid api key should fail the browser WS handshake with 401"
            );
        }
        other => panic!("expected an HTTP 401 handshake failure, got {other:?}"),
    }
}
