//! Integration coverage for the two WebSocket relay handlers that mediate
//! browser <-> agent Docker traffic:
//!
//!   (A) `crates/server/src/router/ws/docker_logs.rs`
//!       — `/api/ws/docker/logs/{server_id}`. A browser opens the socket,
//!         `subscribe`s to a container, and the server forwards
//!         `DockerLogsStart` to the agent; the agent's `docker_log` frames are
//!         relayed back as `{"type":"logs",...}`, and `unsubscribe` forwards
//!         `DockerLogsStop`. The handshake is gated admin + online + CAP_DOCKER.
//!
//!   (B) `crates/server/src/router/ws/browser.rs` (docker viewer arms)
//!       — `/api/ws/servers`. After the initial `full_sync`, a browser
//!         `docker_subscribe` registers a viewer; the FIRST viewer makes the
//!         server push `DockerStartStats` + `DockerEventsStart` to the agent, a
//!         SECOND viewer does not re-send, and the LAST `docker_unsubscribe`
//!         pushes `DockerStopStats` + `DockerEventsStop`. A subscribe for a
//!         server whose agent lacks CAP_DOCKER / the docker feature is silently
//!         ignored (no agent message).
//!
//! Coverage focus (not already covered by `docker_integration.rs`):
//!   - docker-logs relay: subscribe -> DockerLogsStart, agent docker_log ->
//!     browser `logs`, unsubscribe -> DockerLogsStop (both directions).
//!   - docker-logs handshake gates: CAP_DOCKER-denied (403), offline (400 /
//!     handshake rejected), non-admin member (403), unauthenticated (401).
//!   - browser docker viewer arms: first-viewer start, second-viewer no-resend,
//!     last-viewer stop, and the silent no-capability ignore.
//!
//! `docker_integration.rs` already covers docker-logs subscribe AUDITING and the
//! resubscribe-after-reconnect viewer lifecycle; those are not duplicated here.

mod common;

use common::{
    connect_agent, http_client, login_admin, login_as_new_user, recv_agent_text, register_agent,
    start_test_server, AgentReader, AgentSink,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

use serverbee_common::constants::{CAP_DEFAULT, CAP_DOCKER};

// ---------------------------------------------------------------------------
// Generic WS read/write half aliases for the browser/logs client side.
// ---------------------------------------------------------------------------

type WsClient =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsSink = futures_util::stream::SplitSink<WsClient, tungstenite::Message>;
type WsReader = futures_util::stream::SplitStream<WsClient>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create an admin API key and return the raw `serverbee_...` value.
async fn create_api_key(client: &reqwest::Client, base_url: &str, name: &str) -> String {
    let resp = client
        .post(format!("{base_url}/api/auth/api-keys"))
        .json(&json!({ "name": name }))
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

/// Send a single agent frame as a JSON text message.
async fn send_agent_frame(sink: &mut AgentSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("send agent frame");
}

/// First-connect pushes a default agent must tolerate and ignore.
fn is_ignorable_push(msg_type: Option<&str>) -> bool {
    matches!(
        msg_type,
        Some("ping_tasks_sync")
            | Some("network_probe_sync")
            | Some("ip_quality_sync")
            | Some("blocklist_reset")
            | Some("blocklist_sync")
            | Some("blocklist_add")
            | Some("blocklist_remove")
    )
}

/// Drain frames the server pushes right after the SystemInfo handshake; returns
/// once the inbound stream is quiet for `quiet_ms`.
async fn drain_first_connect_pushes(reader: &mut AgentReader, quiet_ms: u64) {
    loop {
        match tokio::time::timeout(Duration::from_millis(quiet_ms), reader.next()).await {
            Ok(Some(Ok(_))) => continue,
            _ => break,
        }
    }
}

/// Bring up a mock agent advertising the `docker` feature plus the given
/// capability bitmask, then drain first-connect noise. Returns
/// `(server_id, sink, reader)`.
async fn bring_up_docker_agent(
    client: &reqwest::Client,
    base_url: &str,
    caps: u32,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    assert_eq!(recv_agent_text(&mut reader).await["type"], "welcome");

    let system_info = json!({
        "type": "system_info",
        "msg_id": "docker-logs-handshake",
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
        "protocol_version": serverbee_common::constants::PROTOCOL_VERSION,
        "features": ["docker"],
        "agent_local_capabilities": caps
    });
    send_agent_frame(&mut sink, system_info).await;
    loop {
        let msg = recv_agent_text(&mut reader).await;
        if msg["type"] == "ack" {
            assert_eq!(msg["msg_id"], "docker-logs-handshake");
            break;
        }
    }
    drain_first_connect_pushes(&mut reader, 300).await;
    (server_id, sink, reader)
}

/// Build a docker-logs WS client request carrying the given `x-api-key`.
fn docker_logs_request_with_key(
    base_url: &str,
    server_id: &str,
    api_key: &str,
) -> tungstenite::handshake::client::Request {
    let ws_url = format!(
        "{}/api/ws/docker/logs/{server_id}",
        base_url.replace("http://", "ws://")
    );
    let mut request = ws_url
        .into_client_request()
        .expect("docker-logs ws request should build");
    request.headers_mut().insert(
        "x-api-key",
        HeaderValue::from_str(api_key).expect("api key header should be valid"),
    );
    request
}

/// Build an `/api/ws/servers` (browser) client request carrying `x-api-key`.
fn browser_request_with_key(
    base_url: &str,
    api_key: &str,
) -> tungstenite::handshake::client::Request {
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

/// Receive the next text frame from a generic client socket, parsed as JSON (5s timeout).
async fn recv_client_text(reader: &mut WsReader) -> Value {
    let message = tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .expect("timed out waiting for client message")
        .expect("client WebSocket stream ended")
        .expect("client WebSocket read error");
    match message {
        tungstenite::Message::Text(text) => {
            serde_json::from_str(&text).expect("parse client message")
        }
        other => panic!("expected Text message, got: {other:?}"),
    }
}

/// Send a JSON text frame on a generic client socket.
async fn send_client_text(sink: &mut WsSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("send client frame");
}

/// Read forwarded agent frames until the set of `expected` `type`s has all been
/// observed (deduped) or `budget` elapses. Ignorable first-connect pushes are
/// skipped. Returns the set actually seen.
async fn agent_recv_until_types(
    reader: &mut AgentReader,
    expected: &[&str],
    budget: Duration,
) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut seen: Vec<String> = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Some(Ok(tungstenite::Message::Text(text)))) =
            tokio::time::timeout(remaining, reader.next()).await
        else {
            break;
        };
        let parsed: Value = serde_json::from_str(&text).expect("parse agent frame");
        let Some(msg_type) = parsed["type"].as_str() else {
            continue;
        };
        if is_ignorable_push(Some(msg_type)) {
            continue;
        }
        if expected.contains(&msg_type) && !seen.iter().any(|s| s == msg_type) {
            seen.push(msg_type.to_string());
        }
        if seen.len() == expected.len() {
            break;
        }
    }
    seen
}

// ===========================================================================
// (A) Docker-logs WS relay
// ===========================================================================

/// Happy path, both directions: subscribe -> DockerLogsStart relay, the agent's
/// `docker_log` -> browser `logs` relay, and unsubscribe -> DockerLogsStop.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn docker_logs_relay_subscribe_logs_and_unsubscribe() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "docker-logs-relay-key").await;

    let (server_id, sink, reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT | CAP_DOCKER).await;

    // The mock agent: on DockerLogsStart it echoes the server-assigned session_id
    // back through a `docker_log` frame, then waits for DockerLogsStop. It reports
    // the observed frames over a channel so the test can assert the relay shape.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Value>(8);
    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("docker_logs_start") => {
                        let session_id =
                            msg["session_id"].as_str().expect("session_id").to_string();
                        // Surface the start frame (with container/tail/follow) to the test.
                        let _ = tx.send(msg.clone()).await;
                        // Relay back a log batch tagged with the same session_id.
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "docker_log",
                                "session_id": session_id,
                                "entries": [
                                    { "timestamp": "2026-06-25T00:00:00Z",
                                      "stream": "stdout", "message": "hello from container" },
                                    { "timestamp": null,
                                      "stream": "stderr", "message": "a warning line" }
                                ]
                            }),
                        )
                        .await;
                    }
                    Some("docker_logs_stop") => {
                        let _ = tx.send(msg.clone()).await;
                        return;
                    }
                    other if is_ignorable_push(other) => {}
                    Some(_other) => {}
                    None => {}
                }
            }
        })
    };

    // Open the docker-logs browser socket.
    let request = docker_logs_request_with_key(&base_url, &server_id, &api_key);
    let (logs_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("docker-logs WebSocket should connect for an admin with CAP_DOCKER");
    let (mut logs_sink, mut logs_reader) = logs_ws.split();

    // First frame is the session announcement.
    let session = recv_client_text(&mut logs_reader).await;
    assert_eq!(session["type"], "session", "first frame should be `session`");
    assert!(
        session["session_id"].as_str().is_some(),
        "session frame should carry a session_id"
    );

    // Browser -> Server -> Agent: subscribe forwards DockerLogsStart.
    send_client_text(
        &mut logs_sink,
        json!({
            "type": "subscribe",
            "container_id": "nginx-abc",
            "tail": 50,
            "follow": true
        }),
    )
    .await;

    let start = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for DockerLogsStart")
        .expect("agent channel closed before DockerLogsStart");
    assert_eq!(start["type"], "docker_logs_start");
    assert_eq!(
        start["container_id"], "nginx-abc",
        "DockerLogsStart should relay the subscribed container_id"
    );
    assert_eq!(start["tail"], 50, "DockerLogsStart should relay tail");
    assert_eq!(start["follow"], true, "DockerLogsStart should relay follow");
    assert_eq!(
        start["session_id"], session["session_id"],
        "DockerLogsStart should carry the same server-assigned session_id"
    );

    // Agent -> Server -> Browser: the docker_log batch surfaces as a `logs` frame.
    let logs = recv_client_text(&mut logs_reader).await;
    assert_eq!(logs["type"], "logs", "agent docker_log should relay as `logs`");
    let entries = logs["entries"].as_array().expect("logs.entries array");
    assert_eq!(entries.len(), 2, "both log entries should be relayed");
    assert_eq!(entries[0]["message"], "hello from container");
    assert_eq!(entries[0]["stream"], "stdout");
    assert_eq!(entries[1]["stream"], "stderr");

    // Browser -> Server -> Agent: unsubscribe forwards DockerLogsStop.
    send_client_text(&mut logs_sink, json!({ "type": "unsubscribe" })).await;

    let stop = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for DockerLogsStop")
        .expect("agent channel closed before DockerLogsStop");
    assert_eq!(stop["type"], "docker_logs_stop");
    assert_eq!(
        stop["session_id"], session["session_id"],
        "DockerLogsStop should target the same session_id"
    );

    agent_task.await.expect("agent responder failed");
    let _ = logs_sink.close().await;
}

/// Capability gate: an agent reporting caps WITHOUT CAP_DOCKER makes the
/// docker-logs handshake fail (403) before any session is opened.
#[tokio::test]
async fn docker_logs_ws_rejects_agent_without_cap_docker() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "docker-logs-nocap-key").await;

    // Online + docker feature, but caps lack CAP_DOCKER → capability_denied_reason
    // returns a reason and the handshake is rejected.
    let (server_id, mut sink, _reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT).await;

    let request = docker_logs_request_with_key(&base_url, &server_id, &api_key);
    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("docker-logs WS should reject an agent without CAP_DOCKER");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(
            resp.status(),
            403,
            "missing CAP_DOCKER should fail the docker-logs handshake with 403"
        ),
        other => panic!("expected an HTTP 403 handshake failure, got {other:?}"),
    }

    let _ = sink.close().await;
}

/// Offline gate: a registered-but-never-connected agent makes the docker-logs
/// handshake fail (400, "Agent is offline").
#[tokio::test]
async fn docker_logs_ws_rejects_offline_agent() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "docker-logs-offline-key").await;

    // Registered server row exists, but no live agent connection → offline.
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let request = docker_logs_request_with_key(&base_url, &server_id, &api_key);
    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("docker-logs WS should reject an offline agent");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(
            resp.status(),
            400,
            "offline agent should fail the docker-logs handshake with 400"
        ),
        other => panic!("expected an HTTP 400 handshake failure, got {other:?}"),
    }
}

/// Non-admin gate: a member (read-only) is forbidden from opening docker logs
/// even when the agent is online + docker-capable (403).
#[tokio::test]
async fn docker_logs_ws_rejects_non_admin_member() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let (server_id, sink, _reader) =
        bring_up_docker_agent(&admin, &base_url, CAP_DEFAULT | CAP_DOCKER).await;

    // Member authenticates via their browser session cookie (API-key creation is
    // admin-only). Capture the cookie pair and replay it on the WS handshake.
    let member = login_as_new_user(&admin, &base_url, "docker-logs-member", "member").await;
    let login = member
        .post(format!("{base_url}/api/auth/login"))
        .json(&json!({ "username": "docker-logs-member", "password": "memberpass" }))
        .send()
        .await
        .expect("member re-login failed");
    let cookie_pair = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("member login should set a session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let ws_url = format!(
        "{}/api/ws/docker/logs/{server_id}",
        base_url.replace("http://", "ws://")
    );
    let mut request = ws_url
        .into_client_request()
        .expect("docker-logs ws request should build");
    request.headers_mut().insert(
        "cookie",
        HeaderValue::from_str(&cookie_pair).expect("cookie header should be valid"),
    );

    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("docker-logs WS should reject a non-admin member");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(
            resp.status(),
            403,
            "a member should be forbidden (403) from the docker-logs WS"
        ),
        other => panic!("expected an HTTP 403 handshake failure, got {other:?}"),
    }

    let mut sink = sink;
    let _ = sink.close().await;
}

/// Auth gate: an unauthenticated connect is rejected at the handshake (401).
#[tokio::test]
async fn docker_logs_ws_rejects_unauthenticated() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, sink, _reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT | CAP_DOCKER).await;

    let ws_url = format!(
        "{}/api/ws/docker/logs/{server_id}",
        base_url.replace("http://", "ws://")
    );
    let request = ws_url
        .into_client_request()
        .expect("docker-logs ws request should build");

    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("docker-logs WS should reject an unauthenticated connect");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(
            resp.status(),
            401,
            "unauthenticated docker-logs WS connect should fail with 401"
        ),
        other => panic!("expected an HTTP 401 handshake failure, got {other:?}"),
    }

    let mut sink = sink;
    let _ = sink.close().await;
}

// ===========================================================================
// (B) Browser update WS docker viewer arms
// ===========================================================================

/// First viewer starts the docker streams; a second concurrent viewer does NOT
/// re-send; the last unsubscribe stops them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_docker_viewer_first_starts_second_noop_last_stops() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "docker-viewer-key").await;

    let (server_id, mut agent_sink, mut agent_reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT | CAP_DOCKER).await;

    // First browser connection.
    let (browser1_ws, _) = tokio_tungstenite::connect_async(browser_request_with_key(
        &base_url, &api_key,
    ))
    .await
    .expect("first browser WS should connect");
    let (mut browser1_sink, mut browser1_reader) = browser1_ws.split();
    assert_eq!(recv_client_text(&mut browser1_reader).await["type"], "full_sync");

    // First viewer subscribe → agent receives DockerStartStats + DockerEventsStart.
    send_client_text(
        &mut browser1_sink,
        json!({ "type": "docker_subscribe", "server_id": server_id }),
    )
    .await;
    let first = agent_recv_until_types(
        &mut agent_reader,
        &["docker_start_stats", "docker_events_start"],
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        first.len(),
        2,
        "first viewer should start both docker streams, saw {first:?}"
    );

    // Second browser connection subscribes to the SAME server.
    let (browser2_ws, _) = tokio_tungstenite::connect_async(browser_request_with_key(
        &base_url, &api_key,
    ))
    .await
    .expect("second browser WS should connect");
    let (mut browser2_sink, mut browser2_reader) = browser2_ws.split();
    assert_eq!(recv_client_text(&mut browser2_reader).await["type"], "full_sync");

    send_client_text(
        &mut browser2_sink,
        json!({ "type": "docker_subscribe", "server_id": server_id }),
    )
    .await;
    // The second viewer is not the first → no fresh start frames should arrive.
    let second = agent_recv_until_types(
        &mut agent_reader,
        &["docker_start_stats", "docker_events_start"],
        Duration::from_millis(800),
    )
    .await;
    assert!(
        second.is_empty(),
        "a second viewer must not re-send docker start frames, saw {second:?}"
    );

    // First viewer unsubscribes — still one viewer left, no stop frames.
    send_client_text(
        &mut browser1_sink,
        json!({ "type": "docker_unsubscribe", "server_id": server_id }),
    )
    .await;
    let after_first_unsub = agent_recv_until_types(
        &mut agent_reader,
        &["docker_stop_stats", "docker_events_stop"],
        Duration::from_millis(800),
    )
    .await;
    assert!(
        after_first_unsub.is_empty(),
        "stop frames must not fire while a viewer remains, saw {after_first_unsub:?}"
    );

    // Last viewer unsubscribes — agent receives DockerStopStats + DockerEventsStop.
    send_client_text(
        &mut browser2_sink,
        json!({ "type": "docker_unsubscribe", "server_id": server_id }),
    )
    .await;
    let stopped = agent_recv_until_types(
        &mut agent_reader,
        &["docker_stop_stats", "docker_events_stop"],
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        stopped.len(),
        2,
        "last unsubscribe should stop both docker streams, saw {stopped:?}"
    );

    let _ = browser1_sink.close().await;
    let _ = browser2_sink.close().await;
    let _ = agent_sink.close().await;
}

/// A docker_subscribe for a server whose agent lacks CAP_DOCKER is silently
/// ignored: no start frame ever reaches the agent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browser_docker_subscribe_without_capability_is_ignored() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "docker-nocap-viewer-key").await;

    // Agent online + docker feature advertised, but caps lack CAP_DOCKER.
    let (server_id, mut agent_sink, mut agent_reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT).await;

    let (browser_ws, _) =
        tokio_tungstenite::connect_async(browser_request_with_key(&base_url, &api_key))
            .await
            .expect("browser WS should connect");
    let (mut browser_sink, mut browser_reader) = browser_ws.split();
    assert_eq!(recv_client_text(&mut browser_reader).await["type"], "full_sync");

    send_client_text(
        &mut browser_sink,
        json!({ "type": "docker_subscribe", "server_id": server_id }),
    )
    .await;

    let seen = agent_recv_until_types(
        &mut agent_reader,
        &["docker_start_stats", "docker_events_start"],
        Duration::from_millis(800),
    )
    .await;
    assert!(
        seen.is_empty(),
        "docker_subscribe without CAP_DOCKER must be silently ignored, saw {seen:?}"
    );

    let _ = browser_sink.close().await;
    let _ = agent_sink.close().await;
}
