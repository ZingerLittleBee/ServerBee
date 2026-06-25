//! Second integration suite for the AGENT -> SERVER dispatch arms in
//! `crates/server/src/router/ws/agent.rs`, covering the `handle_agent_message`
//! variants the other suites leave untouched.
//!
//! Already covered elsewhere (NOT duplicated here):
//!   - `tests/agent_messages.rs`: report / ping_result / task_result(no waiter) /
//!     security_event(ssh_brute_force persist + port_scan drop) /
//!     capability_denied(exec, no waiter) / ip_changed / capabilities_changed
//!     (grant) / network_probe_results / pong / invalid-json.
//!   - `tests/agent_ws_dispatch.rs`: traceroute round-update (complete / progress
//!     / mismatch / no-placeholder) + legacy traceroute_result, upgrade
//!     progress/result Acks, capability_denied(upgrade), unsolicited docker_event,
//!     features_update, capabilities_changed(revoked), blocklist_reset_ack,
//!     terminal output/started/error for an orphaned session.
//!   - `tests/agent_file_ops.rs` / `tests/agent_docker_extra.rs`: the file- and
//!     docker-control request/response round-trips (FileListResult, DockerNetworks,
//!     DockerActionResult, upload/download transfer handshakes, etc.).
//!   - `tests/integration.rs`: TaskResult with a pending exec waiter
//!     (correlation -> exec_finished audit), BlocklistAck (audit + persisted
//!     state), the SystemInfo handshake + ServerOnline broadcast.
//!   - the `#[cfg(test)]` unit module in `ws/agent.rs`: the
//!     superseded-connection / server-lock arms of `handle_current_connection_frame`.
//!
//! This file targets the REMAINING inbound variants, each verified through an
//! observable side effect (a follow-up GET, a browser-WS broadcast, or that the
//! socket stays alive — proven by a follow-up handshake the server still Acks):
//!
//!   - DockerInfo (unsolicited)        -> DockerAvailabilityChanged broadcast
//!   - DockerContainers (unsolicited)  -> cache visible via GET + docker_update
//!   - DockerStats (unsolicited)       -> stats visible via GET /docker/stats
//!   - SystemInfo (second, changed caps) -> mirror update + capabilities_changed
//!   - reconnect / superseded connection -> second connect wins; the first
//!     socket's next frame stops its read loop (end-to-end, not the unit arm)
//!   - SecurityEvent (ssh_login, FULL evidence) -> persisted, queryable
//!   - CapabilityDenied (capability=terminal, with session_id) -> the
//!     terminal-session unregister arm; connection survives
//!
//! NOT covered here (NOTE — genuinely structural plumbing, not per-variant
//! dispatch): the Message::Close branch, the Binary-frame decode branch, the
//! Err(read-error) branch, and the write-task abort on `rx`-close are all
//! connection-lifecycle wiring rather than `handle_agent_message` arms, so they
//! are intentionally skipped.

mod common;

use common::{
    connect_agent, http_client, login_admin, recv_agent_text, register_agent, send_system_info,
    start_test_server, AgentReader, AgentSink,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

use serverbee_common::constants::{CAP_DEFAULT, CAP_DOCKER, CAP_SECURITY_EVENTS, CAP_TERMINAL};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Send a single agent frame as a JSON text message.
async fn send_agent_frame(sink: &mut AgentSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("send agent frame");
}

/// Drain any frames the server pushes right after the SystemInfo handshake.
/// Returns once the inbound stream is quiet for `quiet_ms`.
async fn drain_first_connect_pushes(reader: &mut AgentReader, quiet_ms: u64) {
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(quiet_ms), reader.next()).await {
            Ok(Some(Ok(_))) => continue,
            Ok(_) => break,
            Err(_) => break,
        }
    }
}

/// Standard mock-agent bring-up with the default capability set, features empty.
async fn bring_up_agent(
    client: &reqwest::Client,
    base_url: &str,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome", "first frame must be welcome");
    send_system_info(&mut sink, &mut reader, "d2-handshake", Some(CAP_DEFAULT)).await;
    drain_first_connect_pushes(&mut reader, 300).await;
    (server_id, sink, reader)
}

/// Bring up a mock agent advertising the `docker` runtime feature plus the
/// given capability bitmask. The docker control/read gates require both the
/// feature and CAP_DOCKER.
async fn bring_up_docker_agent(
    client: &reqwest::Client,
    base_url: &str,
    caps: u32,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome");

    let system_info = json!({
        "type": "system_info",
        "msg_id": "d2-docker-handshake",
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
        "agent_version": "0.1.0",
        "protocol_version": serverbee_common::constants::PROTOCOL_VERSION,
        "features": ["docker"],
        "agent_local_capabilities": caps
    });
    send_agent_frame(&mut sink, system_info).await;
    loop {
        let msg = recv_agent_text(&mut reader).await;
        if msg["type"] == "ack" {
            assert_eq!(msg["msg_id"], "d2-docker-handshake");
            break;
        }
    }
    drain_first_connect_pushes(&mut reader, 300).await;
    (server_id, sink, reader)
}

/// Create an API key as an already-authenticated admin and return the raw key.
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
        .expect("api key missing")
        .to_string()
}

/// Connect a browser update WebSocket with an `x-api-key`, consume the initial
/// `full_sync`, and return the live socket.
async fn connect_browser_ws(
    base_url: &str,
    api_key: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut request = ws_url
        .into_client_request()
        .expect("browser ws request should build");
    request.headers_mut().insert(
        "x-api-key",
        HeaderValue::from_str(api_key).expect("api key header should be valid"),
    );
    let (mut browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("browser websocket should connect");
    let full_sync = tokio::time::timeout(std::time::Duration::from_secs(5), browser_ws.next())
        .await
        .expect("full_sync timeout")
        .expect("browser ws closed")
        .expect("browser ws error");
    let full_sync: Value = serde_json::from_str(full_sync.to_text().unwrap()).unwrap();
    assert_eq!(full_sync["type"], "full_sync");
    browser_ws
}

/// Drain the browser stream until a frame matching `pred` arrives (5s budget),
/// ignoring interleaved broadcasts. Returns the matched frame or `None` on
/// timeout / close.
async fn wait_for_browser<P>(
    browser_ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    pred: P,
) -> Option<Value>
where
    P: Fn(&Value) -> bool,
{
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Some(Ok(tungstenite::Message::Text(text)))) =
            tokio::time::timeout(remaining, browser_ws.next()).await
        else {
            break;
        };
        let parsed: Value = serde_json::from_str(&text).expect("parse browser frame");
        if pred(&parsed) {
            return Some(parsed);
        }
    }
    None
}

// ===========================================================================
// DockerInfo (unsolicited)  -> DockerAvailabilityChanged{available:true} broadcast
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_docker_info_unsolicited_broadcasts_availability() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "d2-docker-info-key").await;

    // Subscribe the browser BEFORE the agent comes online so it sees later pushes.
    let mut browser_ws = connect_browser_ws(&base_url, &api_key).await;

    let (server_id, mut sink, mut reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT | CAP_DOCKER).await;

    // An unsolicited DockerInfo (no msg_id) updates the info cache and always
    // broadcasts DockerAvailabilityChanged{available:true}.
    send_agent_frame(
        &mut sink,
        json!({
            "type": "docker_info",
            "msg_id": null,
            "info": {
                "docker_version": "26.1.0",
                "api_version": "1.45",
                "os": "linux",
                "arch": "x86_64",
                "containers_running": 2,
                "containers_paused": 0,
                "containers_stopped": 1,
                "images": 7,
                "memory_total": 16_000_000_000_u64
            }
        }),
    )
    .await;

    let frame = wait_for_browser(&mut browser_ws, |m| {
        m["type"] == "docker_availability_changed" && m["server_id"] == server_id
    })
    .await;
    let frame = frame.expect("expected docker_availability_changed broadcast");
    assert_eq!(
        frame["available"], true,
        "unsolicited docker_info should broadcast availability=true"
    );

    // The connection survives the frame.
    send_system_info(&mut sink, &mut reader, "post-docker-info", Some(CAP_DEFAULT | CAP_DOCKER))
        .await;

    let _ = sink.close().await;
    let _ = browser_ws.close(None).await;
}

// ===========================================================================
// DockerContainers (unsolicited)  -> cache visible via GET + docker_update broadcast
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_docker_containers_unsolicited_updates_cache_and_broadcasts() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "d2-docker-containers-key").await;

    let mut browser_ws = connect_browser_ws(&base_url, &api_key).await;

    let (server_id, mut sink, _reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT | CAP_DOCKER).await;

    send_agent_frame(
        &mut sink,
        json!({
            "type": "docker_containers",
            "msg_id": null,
            "containers": [{
                "id": "c0ffee123456",
                "name": "web",
                "image": "nginx:latest",
                "state": "running",
                "status": "Up 3 minutes",
                "created": 1_700_000_000_i64,
                "ports": [{
                    "private_port": 80,
                    "public_port": 8080,
                    "port_type": "tcp",
                    "ip": "0.0.0.0"
                }],
                "labels": {}
            }]
        }),
    )
    .await;

    // (a) The browser sees a docker_update carrying the new container.
    let frame = wait_for_browser(&mut browser_ws, |m| {
        m["type"] == "docker_update"
            && m["server_id"] == server_id
            && m["containers"]
                .as_array()
                .is_some_and(|c| c.iter().any(|x| x["id"] == "c0ffee123456"))
    })
    .await;
    assert!(
        frame.is_some(),
        "unsolicited docker_containers should broadcast a docker_update"
    );

    // (b) The cache read reflects the new container.
    let mut found = false;
    for _ in 0..30 {
        let resp = client
            .get(format!("{base_url}/api/servers/{server_id}/docker/containers"))
            .send()
            .await
            .expect("GET /docker/containers failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse containers");
        if body["data"]["containers"]
            .as_array()
            .is_some_and(|c| c.iter().any(|x| x["id"] == "c0ffee123456"))
        {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found, "docker_containers cache should be queryable via GET");

    let _ = sink.close().await;
    let _ = browser_ws.close(None).await;
}

// ===========================================================================
// DockerStats (unsolicited, after containers cached) -> visible via GET /docker/stats
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_docker_stats_unsolicited_updates_cache() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, _reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT | CAP_DOCKER).await;

    // Seed containers first: the DockerStats broadcast branch only fires when a
    // container list is already cached, but the stats cache itself is updated
    // unconditionally and is what the GET reads back.
    send_agent_frame(
        &mut sink,
        json!({
            "type": "docker_containers",
            "msg_id": null,
            "containers": [{
                "id": "statbox01",
                "name": "db",
                "image": "postgres:16",
                "state": "running",
                "status": "Up 1 hour",
                "created": 1_700_000_000_i64,
                "ports": [],
                "labels": {}
            }]
        }),
    )
    .await;

    send_agent_frame(
        &mut sink,
        json!({
            "type": "docker_stats",
            "stats": [{
                "id": "statbox01",
                "name": "db",
                "cpu_percent": 12.5,
                "memory_usage": 256_000_000_u64,
                "memory_limit": 1_000_000_000_u64,
                "memory_percent": 25.6,
                "network_rx": 1024_u64,
                "network_tx": 2048_u64,
                "block_read": 4096_u64,
                "block_write": 8192_u64
            }]
        }),
    )
    .await;

    let mut found = false;
    for _ in 0..30 {
        let resp = client
            .get(format!("{base_url}/api/servers/{server_id}/docker/stats"))
            .send()
            .await
            .expect("GET /docker/stats failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse stats");
        if body["data"]["stats"]
            .as_array()
            .is_some_and(|s| s.iter().any(|x| x["id"] == "statbox01"))
        {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found, "docker_stats cache should be queryable via GET /docker/stats");

    let _ = sink.close().await;
}

// ===========================================================================
// SystemInfo (second, changed caps) -> mirror update + capabilities_changed broadcast
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_second_system_info_updates_capability_mirror() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "d2-sysinfo-mirror-key").await;

    let mut browser_ws = connect_browser_ws(&base_url, &api_key).await;

    let (server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // The first handshake reported CAP_DEFAULT. A SECOND SystemInfo that reports
    // a wider bitmask (adding CAP_TERMINAL) re-runs the capability-mirror branch:
    // it persists the new mirror and broadcasts CapabilitiesChanged.
    let new_caps = CAP_DEFAULT | CAP_TERMINAL;
    send_system_info(&mut sink, &mut reader, "second-sysinfo", Some(new_caps)).await;

    // The browser observes the CapabilitiesChanged broadcast with the new bitmask.
    let frame = wait_for_browser(&mut browser_ws, |m| {
        m["type"] == "capabilities_changed"
            && m["server_id"] == server_id
            && m["capabilities"].as_u64() == Some(new_caps as u64)
    })
    .await;
    assert!(
        frame.is_some(),
        "second SystemInfo with wider caps should broadcast capabilities_changed"
    );

    // The persisted mirror reflects the wider bitmask.
    let mut mirrored = false;
    for _ in 0..30 {
        let resp = client
            .get(format!("{base_url}/api/servers/{server_id}"))
            .send()
            .await
            .expect("GET server failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse server");
        if body["data"]["capabilities"].as_u64() == Some(new_caps as u64) {
            mirrored = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(mirrored, "second SystemInfo should update the persisted capability mirror");

    let _ = sink.close().await;
    let _ = browser_ws.close(None).await;
}

// ===========================================================================
// Reconnect / superseded connection  (second connect wins; first socket dies)
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_reconnect_supersedes_first_connection() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // One server, two sequential agent connections sharing the same token.
    let (server_id, token) = register_agent(&client, &base_url).await;

    let (mut sink_a, mut reader_a) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut reader_a).await["type"], "welcome");
    send_system_info(&mut sink_a, &mut reader_a, "recon-a", Some(CAP_DEFAULT)).await;
    drain_first_connect_pushes(&mut reader_a, 300).await;

    // Second connection for the SAME server supersedes the first in the
    // AgentManager registry (add_connection bumps the connection id).
    let (mut sink_b, mut reader_b) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut reader_b).await["type"], "welcome");
    send_system_info(&mut sink_b, &mut reader_b, "recon-b", Some(CAP_DEFAULT)).await;
    drain_first_connect_pushes(&mut reader_b, 300).await;

    // The FIRST socket is now superseded: handle_current_connection_frame returns
    // false on its next frame, which breaks its read loop and closes the socket.
    // Sending a frame on A and then draining its stream must reach end-of-stream.
    send_agent_frame(&mut sink_a, json!({ "type": "pong" })).await;

    let mut closed = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, reader_a.next()).await {
            Ok(None) => {
                closed = true;
                break;
            }
            Ok(Some(Ok(tungstenite::Message::Close(_)))) => {
                closed = true;
                break;
            }
            Ok(Some(Err(_))) => {
                closed = true;
                break;
            }
            Ok(Some(Ok(_))) => continue,
            Err(_) => break,
        }
    }
    assert!(
        closed,
        "the superseded first connection should be torn down once it sends a frame"
    );

    // The SECOND connection is the live one: it still gets Acked.
    send_system_info(&mut sink_b, &mut reader_b, "recon-b-still-live", Some(CAP_DEFAULT)).await;

    // And the server is online (the surviving connection keeps it up).
    let resp = client
        .get(format!("{base_url}/api/servers/{server_id}"))
        .send()
        .await
        .expect("GET server failed");
    assert_eq!(resp.status(), 200);

    let _ = sink_a.close().await;
    let _ = sink_b.close().await;
}

// ===========================================================================
// SecurityEvent (ssh_login, FULL evidence) -> persisted, queryable by event_type
// ===========================================================================

#[tokio::test]
async fn test_security_event_ssh_login_persists_and_is_queryable() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // CAP_DEFAULT includes CAP_SECURITY_EVENTS; advertise it explicitly so the
    // gate clearly passes for the ssh_login event.
    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut reader).await["type"], "welcome");
    send_system_info(
        &mut sink,
        &mut reader,
        "ssh-login-handshake",
        Some(CAP_DEFAULT | CAP_SECURITY_EVENTS),
    )
    .await;
    drain_first_connect_pushes(&mut reader, 300).await;

    // SshLogin evidence is fully tagged with `kind = "ssh_login"` and an
    // `auth_method`; it is a distinct event_type from ssh_brute_force.
    let source_ip = "198.51.100.77";
    send_agent_frame(
        &mut sink,
        json!({
            "type": "security_event",
            "event_type": "ssh_login",
            "severity": "low",
            "source_ip": source_ip,
            "source_port": 54_321,
            "username": "deploy",
            "started_at": 1_700_000_100_i64,
            "ended_at": 1_700_000_100_i64,
            "first_seen": true,
            "detector_source": "journal",
            "evidence": {
                "kind": "ssh_login",
                "auth_method": "publickey"
            }
        }),
    )
    .await;

    // Filter the events list by event_type so the assertion is unambiguous.
    let mut found = false;
    for _ in 0..30 {
        let resp = client
            .get(format!(
                "{base_url}/api/security/events?server_id={server_id}&event_type=ssh_login"
            ))
            .send()
            .await
            .expect("GET security events failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse security events");
        let items = body["data"]["items"].as_array().expect("items array");
        if items
            .iter()
            .any(|e| e["source_ip"] == source_ip && e["event_type"] == "ssh_login")
        {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        found,
        "ssh_login security_event should persist and be queryable filtered by event_type"
    );

    let _ = sink.close().await;
}

// ===========================================================================
// CapabilityDenied (capability=terminal, with session_id) -> unregister arm
// ===========================================================================

#[tokio::test]
async fn test_capability_denied_terminal_session_is_noop_and_survives() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Advertise CAP_TERMINAL so the frame is unambiguously a real terminal-flavored
    // denial. With no server-registered session for "ghost-term", the
    // unregister_terminal_session arm is a no-op; the connection must survive.
    let (_server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut reader).await["type"], "welcome");
    send_system_info(
        &mut sink,
        &mut reader,
        "cap-denied-term-handshake",
        Some(CAP_DEFAULT | CAP_TERMINAL),
    )
    .await;
    drain_first_connect_pushes(&mut reader, 300).await;

    // A CapabilityDenied for the terminal capability carrying a session_id but NO
    // msg_id: the exec/task branch is skipped, and the terminal-session
    // unregister branch runs for an unknown session (no-op).
    send_agent_frame(
        &mut sink,
        json!({
            "type": "capability_denied",
            "msg_id": null,
            "session_id": "ghost-term",
            "capability": "terminal",
            "reason": "agent_capability_disabled"
        }),
    )
    .await;

    // The connection stays alive: a follow-up handshake is still Acked.
    send_system_info(
        &mut sink,
        &mut reader,
        "post-cap-denied-term",
        Some(CAP_DEFAULT | CAP_TERMINAL),
    )
    .await;

    let _ = sink.close().await;
}
