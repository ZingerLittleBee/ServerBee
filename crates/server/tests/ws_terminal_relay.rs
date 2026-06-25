//! Integration tests for the terminal WebSocket relay
//! `/api/ws/terminal/{server_id}`.
//!
//! Source under test: `crates/server/src/router/ws/terminal.rs` (mounted in
//! `crates/server/src/router/mod.rs` under `/api`).
//!
//! These tests drive the relay end-to-end: a browser-style WS client (authed
//! via `x-api-key`) on one side, and a mock agent on the agent WS on the other.
//! The server bridges the two through the in-memory terminal-session registry.
//!
//! Coverage (relay/dispatch branches in `handle_terminal_ws` +
//! `terminal_ws_handler`):
//!   - Connect + session handshake: server emits `TerminalOpen` to the agent
//!     and `{"type":"session"}` to the browser.
//!   - Agent `terminal_started` -> browser `{"type":"started"}`.
//!   - Agent `terminal_output` (base64) -> browser `{"type":"output","data":..}`.
//!   - Browser `{"type":"input"}` -> server `TerminalInput` to agent.
//!   - Browser `{"type":"resize"}` -> server `TerminalResize` to agent.
//!   - Agent `terminal_error` -> browser `{"type":"error","error":..}`.
//!   - Capability gate: an agent advertising CAP_DEFAULT (no CAP_TERMINAL bit)
//!     makes the handshake fail with HTTP 403.
//!   - AuthZ: a member (non-admin) is rejected with HTTP 403.
//!   - Offline-agent arm: a registered-but-never-connected server is rejected
//!     with HTTP 400 ("Agent is offline").
//!   - Unauthenticated connect is rejected with HTTP 401.
//!
//! Frame shapes:
//!   - Browser -> server: {"type":"input","data":"<b64>"},
//!     {"type":"resize","cols":N,"rows":N}.
//!   - Server -> agent (ServerMessage): TerminalOpen / TerminalInput /
//!     TerminalResize / TerminalClose (snake_case `type`).
//!   - Agent -> server (AgentMessage): terminal_started / terminal_output
//!     (base64 `data`) / terminal_error.
//!   - Server -> browser: {"type":"session"|"started"|"output"|"error"}.
//!
//! NOT covered here (intentional):
//!   - Idle timeout (TERMINAL_IDLE_TIMEOUT_SECS) and mobile-token-expiry arms:
//!     time-driven lifecycle plumbing, not relay logic; would require either
//!     real-time waits or a mobile Bearer session whose fixed expiry is in the
//!     past. Out of scope for the relay/dispatch surface.
//!   - The orphaned-session no-op terminal_output/started/error arms on the
//!     AGENT side are already covered by `agent_ws_dispatch.rs`.
//!   - INTERNAL_SERVER_ERROR arm (server row vanishes between online-check and
//!     capability-check): not reproducible without racing a delete.

mod common;

use common::{
    connect_agent, http_client, login_admin, recv_agent_text, register_agent, send_system_info,
    start_test_server, AgentReader, AgentSink,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

use serverbee_common::constants::{CAP_DEFAULT, CAP_TERMINAL};

/// Read half of a connected browser-style WebSocket.
type BrowserReader = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;
/// Write half of a connected browser-style WebSocket.
type BrowserSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;

// ── Test helpers ───────────────────────────────────────────────────────────

/// Create an admin API key (admin-only endpoint) and return the raw key.
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

/// Build a `/api/ws/terminal/{server_id}` client request carrying an
/// `x-api-key` header.
fn terminal_ws_request_with_key(
    base_url: &str,
    server_id: &str,
    api_key: &str,
) -> tungstenite::handshake::client::Request {
    let ws_url = format!(
        "{}/api/ws/terminal/{server_id}",
        base_url.replace("http://", "ws://")
    );
    let mut request = ws_url
        .into_client_request()
        .expect("terminal ws request should build");
    request.headers_mut().insert(
        "x-api-key",
        HeaderValue::from_str(api_key).expect("api key header should be valid"),
    );
    request
}

/// Receive the next browser text frame, parsed as JSON (5s timeout).
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
        other => panic!("expected Text message, got: {other:?}"),
    }
}

/// Read browser frames until one with `type == expected` arrives (5s budget),
/// ignoring other interleaved frames. Panics on timeout.
async fn recv_browser_until(reader: &mut BrowserReader, expected: &str) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for `{expected}` frame");
        let msg = tokio::time::timeout(remaining, recv_browser_text(reader))
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for `{expected}` frame"));
        if msg["type"] == expected {
            return msg;
        }
    }
}

/// Send a single agent frame as a JSON text message.
async fn send_agent_frame(sink: &mut AgentSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("send agent frame");
}

/// First-connect pushes a default/terminal agent must tolerate and ignore.
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

/// Drain frames the server pushes right after the SystemInfo handshake, until
/// the inbound stream is quiet for `quiet_ms`.
async fn drain_first_connect_pushes(reader: &mut AgentReader, quiet_ms: u64) {
    loop {
        match tokio::time::timeout(Duration::from_millis(quiet_ms), reader.next()).await {
            Ok(Some(Ok(_))) => continue,
            Ok(_) => break,
            Err(_) => break,
        }
    }
}

/// Bring up a mock agent with the given capability bitmask and drain the
/// first-connect noise. Returns `(server_id, sink, reader)`.
async fn bring_up_agent(
    client: &reqwest::Client,
    base_url: &str,
    caps: u32,
    handshake_id: &str,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome", "first frame must be welcome");
    send_system_info(&mut sink, &mut reader, handshake_id, Some(caps)).await;
    drain_first_connect_pushes(&mut reader, 300).await;
    (server_id, sink, reader)
}

// ── Happy path: full relay in both directions ──────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_ws_relays_session_started_output_and_input_resize() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "term-relay-key").await;

    // Agent MUST advertise CAP_TERMINAL or the connect is gated 403.
    let (server_id, sink, reader) =
        bring_up_agent(&client, &base_url, CAP_DEFAULT | CAP_TERMINAL, "term-relay-hs").await;

    // The mock agent: capture the TerminalOpen, emit started + output, then
    // record the TerminalInput / TerminalResize the server forwards back. The
    // captured frames are returned to the test for assertions.
    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            let mut session_id: Option<String> = None;
            let mut got_input: Option<Value> = None;
            let mut got_resize: Option<Value> = None;
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("terminal_open") => {
                        let sid = msg["session_id"].as_str().expect("session_id").to_string();
                        // The handler opens the PTY with a default 24x80 size.
                        assert_eq!(msg["rows"], 24);
                        assert_eq!(msg["cols"], 80);
                        // Reply: started, then a base64 output payload.
                        send_agent_frame(
                            &mut sink,
                            json!({ "type": "terminal_started", "session_id": sid }),
                        )
                        .await;
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "terminal_output",
                                "session_id": sid,
                                "data": "aGVsbG8=" // "hello"
                            }),
                        )
                        .await;
                        session_id = Some(sid);
                    }
                    Some("terminal_input") => {
                        got_input = Some(msg.clone());
                    }
                    Some("terminal_resize") => {
                        got_resize = Some(msg.clone());
                    }
                    Some("terminal_close") => {
                        return (session_id, got_input, got_resize);
                    }
                    other if is_ignorable_push(other) => {}
                    _ => {}
                }
            }
        })
    };

    // Open the browser-side terminal WS.
    let request = terminal_ws_request_with_key(&base_url, &server_id, &api_key);
    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("terminal WebSocket connect should succeed for an admin on a CAP_TERMINAL agent");
    let (mut browser_sink, mut browser_reader): (BrowserSink, BrowserReader) = browser_ws.split();

    // 1) Session handshake frame.
    let session = recv_browser_until(&mut browser_reader, "session").await;
    let session_id = session["session_id"].as_str().expect("session_id").to_string();
    assert!(!session_id.is_empty(), "session frame must carry a session_id");

    // 2) Agent's started → browser started.
    let started = recv_browser_until(&mut browser_reader, "started").await;
    assert_eq!(started["type"], "started");

    // 3) Agent's output → browser output (base64 preserved verbatim).
    let output = recv_browser_until(&mut browser_reader, "output").await;
    assert_eq!(output["data"], "aGVsbG8=", "output base64 must pass through unchanged");

    // 4) Browser input → server TerminalInput → agent.
    browser_sink
        .send(tungstenite::Message::Text(
            json!({ "type": "input", "data": "bHM=" }).to_string().into(),
        ))
        .await
        .expect("send input");

    // 5) Browser resize → server TerminalResize → agent.
    browser_sink
        .send(tungstenite::Message::Text(
            json!({ "type": "resize", "cols": 120, "rows": 40 }).to_string().into(),
        ))
        .await
        .expect("send resize");

    // Close the browser side; the handler then sends TerminalClose to the agent,
    // which lets the responder finish and hand back what it captured.
    browser_sink
        .send(tungstenite::Message::Close(None))
        .await
        .expect("close browser ws");

    let (agent_sid, got_input, got_resize) = tokio::time::timeout(Duration::from_secs(5), agent_task)
        .await
        .expect("agent responder timed out")
        .expect("agent responder panicked");

    assert_eq!(
        agent_sid.as_deref(),
        Some(session_id.as_str()),
        "TerminalOpen session_id must match the browser session frame"
    );

    let input = got_input.expect("server should forward a TerminalInput to the agent");
    assert_eq!(input["session_id"], session_id, "TerminalInput must carry the session_id");
    assert_eq!(input["data"], "bHM=", "TerminalInput data must pass through unchanged");

    let resize = got_resize.expect("server should forward a TerminalResize to the agent");
    assert_eq!(resize["session_id"], session_id, "TerminalResize must carry the session_id");
    assert_eq!(resize["cols"], 120, "TerminalResize cols must pass through");
    assert_eq!(resize["rows"], 40, "TerminalResize rows must pass through");
}

// ── Agent terminal_error → browser error frame ─────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_ws_relays_agent_error_to_browser() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "term-error-key").await;

    let (server_id, sink, reader) =
        bring_up_agent(&client, &base_url, CAP_DEFAULT | CAP_TERMINAL, "term-error-hs").await;

    // The agent answers TerminalOpen with a terminal_error instead of started.
    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("terminal_open") => {
                        let sid = msg["session_id"].as_str().expect("session_id").to_string();
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "terminal_error",
                                "session_id": sid,
                                "error": "failed to spawn shell"
                            }),
                        )
                        .await;
                    }
                    Some("terminal_close") => return,
                    other if is_ignorable_push(other) => {}
                    _ => {}
                }
            }
        })
    };

    let request = terminal_ws_request_with_key(&base_url, &server_id, &api_key);
    let (browser_ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("terminal WebSocket connect should succeed");
    let (mut browser_sink, mut browser_reader): (BrowserSink, BrowserReader) = browser_ws.split();

    // Session handshake first, then the relayed agent error.
    let session = recv_browser_until(&mut browser_reader, "session").await;
    assert!(session["session_id"].is_string());

    let error = recv_browser_until(&mut browser_reader, "error").await;
    assert_eq!(
        error["error"], "failed to spawn shell",
        "agent terminal_error message must reach the browser verbatim"
    );

    let _ = browser_sink.send(tungstenite::Message::Close(None)).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), agent_task).await;
}

// ── Capability gate: agent without CAP_TERMINAL → connect rejected (403) ────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_ws_rejects_when_agent_lacks_terminal_capability() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "term-cap-key").await;

    // CAP_DEFAULT (1852) deliberately omits CAP_TERMINAL (bit 0). The agent is
    // online, so the online-check passes, but capability_denied_reason trips.
    let (server_id, mut sink, _reader) =
        bring_up_agent(&client, &base_url, CAP_DEFAULT, "term-nocap-hs").await;

    let request = terminal_ws_request_with_key(&base_url, &server_id, &api_key);
    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("terminal WS should be rejected when the agent lacks CAP_TERMINAL");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(
            resp.status(),
            403,
            "agent without CAP_TERMINAL should fail the terminal WS handshake with 403"
        ),
        other => panic!("expected an HTTP 403 handshake failure, got {other:?}"),
    }

    let _ = sink.close().await;
}

// ── AuthZ: non-admin (member) → 403 ────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_ws_rejects_non_admin_member() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Bring an agent online WITH CAP_TERMINAL so the only failing gate is the
    // admin-only check (capability + online checks would otherwise pass).
    let (server_id, mut sink, _reader) =
        bring_up_agent(&admin, &base_url, CAP_DEFAULT | CAP_TERMINAL, "term-member-hs").await;

    // Create a member and capture their session cookie (API-key creation is
    // admin-only, so members authenticate via the browser session cookie).
    admin
        .post(format!("{base_url}/api/users"))
        .json(&json!({ "username": "term-member", "password": "memberpass", "role": "member" }))
        .send()
        .await
        .expect("create member failed");
    let member = http_client();
    let login = member
        .post(format!("{base_url}/api/auth/login"))
        .json(&json!({ "username": "term-member", "password": "memberpass" }))
        .send()
        .await
        .expect("member login failed");
    assert_eq!(login.status(), 200, "member login should succeed");
    let cookie_pair = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("login should set a session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let ws_url = format!(
        "{}/api/ws/terminal/{server_id}",
        base_url.replace("http://", "ws://")
    );
    let mut request = ws_url.into_client_request().expect("ws request should build");
    request.headers_mut().insert(
        "cookie",
        HeaderValue::from_str(&cookie_pair).expect("cookie header should be valid"),
    );

    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("terminal WS should reject a non-admin member");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(
            resp.status(),
            403,
            "member terminal WS connect should fail with 403 (admin-only)"
        ),
        other => panic!("expected an HTTP 403 handshake failure, got {other:?}"),
    }

    let _ = sink.close().await;
}

// ── Offline-agent arm: registered but never connected → 400 ────────────────

#[tokio::test]
async fn terminal_ws_rejects_when_agent_offline() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let api_key = create_api_key(&client, &base_url, "term-offline-key").await;

    // Registered but never connected → is_online == false → 400 ("Agent is
    // offline"), short-circuiting before the capability check.
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let request = terminal_ws_request_with_key(&base_url, &server_id, &api_key);
    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("terminal WS should be rejected for an offline agent");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(
            resp.status(),
            400,
            "offline-agent terminal WS connect should fail with 400"
        ),
        other => panic!("expected an HTTP 400 handshake failure, got {other:?}"),
    }
}

// ── AuthN: unauthenticated connect → 401 ───────────────────────────────────

#[tokio::test]
async fn terminal_ws_rejects_unauthenticated_connect() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    // A real server id so the failure is the auth gate, not a routing miss.
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let ws_url = format!(
        "{}/api/ws/terminal/{server_id}",
        base_url.replace("http://", "ws://")
    );
    let request = ws_url
        .into_client_request()
        .expect("terminal ws request should build");

    let err = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("terminal WS should reject an unauthenticated connect");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(
            resp.status(),
            401,
            "unauthenticated terminal WS connect should fail with 401"
        ),
        other => panic!("expected an HTTP 401 handshake failure, got {other:?}"),
    }
}
