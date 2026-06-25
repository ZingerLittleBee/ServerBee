//! Integration coverage for the AGENT -> SERVER message router in
//! `crates/server/src/router/ws/agent.rs`.
//!
//! Each test stands up a mock agent (register -> connect -> welcome ->
//! SystemInfo handshake), then sends one or more `AgentMessage` variants as
//! JSON text frames and asserts the server processed them. Side effects are
//! verified through the admin HTTP API where an endpoint exists (security
//! events, ping records, task results) and otherwise by asserting the agent
//! socket stays connected (the server keeps reading and Acks where applicable).
//!
//! `unit`-level coverage for SecurityEvent / UnlockResults capability gating
//! already lives in the `#[cfg(test)]` module inside `ws/agent.rs`; this file
//! exercises the full WebSocket -> serde -> `handle_agent_message` path end to
//! end so the framing, `serde(tag = "type")` snake_case dispatch, and HTTP-
//! visible persistence are covered together.
//!
//! NOT covered here (NOTE): terminal / file / docker control responses and the
//! upload/download transfer messages all require a prior server-initiated
//! request (a pending `msg_id` / `transfer_id` / session) before the agent's
//! reply is meaningful. Those round-trips are exercised by
//! `tests/agent_file_ops.rs`, `tests/docker_integration.rs`, and
//! `tests/agent_docker_extra.rs`. Sending them unsolicited is a no-op (orphaned
//! dispatch is silently dropped), so they would assert nothing here.

mod common;

use common::{
    connect_agent, http_client, login_admin, recv_agent_text, register_agent, send_system_info,
    start_test_server, AgentReader, AgentSink,
};
use futures_util::SinkExt;
use serde_json::{json, Value};
use serverbee_common::constants::CAP_DEFAULT;
use tokio_tungstenite::tungstenite;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Send a single agent frame as a JSON text message.
async fn send_agent_frame(sink: &mut AgentSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("send agent frame");
}

/// Drain any frames the server pushes right after the SystemInfo handshake
/// (ping/network/IP-quality sync + firewall blocklist reset/sync for a default
/// agent). Returns once the inbound stream is quiet for `quiet_ms`.
///
/// Calling this before sending test frames keeps later assertions clean and,
/// because it reads from the socket, also proves the connection is still alive
/// after the handshake.
async fn drain_first_connect_pushes(reader: &mut AgentReader, quiet_ms: u64) {
    use futures_util::StreamExt;
    loop {
        match tokio::time::timeout(
            std::time::Duration::from_millis(quiet_ms),
            reader.next(),
        )
        .await
        {
            // Got a frame within the window — keep draining.
            Ok(Some(Ok(_))) => continue,
            // Stream ended or errored — stop; callers assert liveness separately.
            Ok(_) => break,
            // Timed out: no more pushes are pending.
            Err(_) => break,
        }
    }
}

/// Read frames until one with `type == expected` arrives (5s budget), ignoring
/// unrelated server pushes. Panics on timeout.
async fn recv_until_type(reader: &mut AgentReader, expected: &str) -> Value {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for `{expected}` frame");
        let msg = tokio::time::timeout(remaining, recv_agent_text(reader))
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for `{expected}` frame"));
        if msg["type"] == expected {
            return msg;
        }
    }
}

/// Standard mock-agent bring-up: register, connect, consume Welcome, complete
/// the SystemInfo handshake with the default capability set, and drain the
/// first-connect pushes. Returns the connected socket halves plus the server id.
async fn bring_up_agent(
    client: &reqwest::Client,
    base_url: &str,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;

    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome", "first frame must be welcome");

    // None lets the server apply CAP_DEFAULT (which already includes
    // CAP_SECURITY_EVENTS), enabling the security-event path below.
    send_system_info(&mut sink, &mut reader, "handshake-1", None).await;
    drain_first_connect_pushes(&mut reader, 300).await;

    (server_id, sink, reader)
}

// ---------------------------------------------------------------------------
// report  (system metrics → in-memory report cache; no Ack, no disconnect)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_report_is_accepted_and_keeps_connection_alive() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // `Report` is a newtype variant: serde flattens the SystemReport fields
    // directly alongside the "report" tag.
    send_agent_frame(
        &mut sink,
        json!({
            "type": "report",
            "cpu": 12.5,
            "mem_used": 4_000_000_000_i64,
            "swap_used": 0_i64,
            "disk_used": 20_000_000_000_i64,
            "net_in_speed": 1024_i64,
            "net_out_speed": 2048_i64,
            "net_in_transfer": 10_000_i64,
            "net_out_transfer": 20_000_i64,
            "load1": 0.5,
            "load5": 0.4,
            "load15": 0.3,
            "tcp_conn": 42,
            "udp_conn": 7,
            "process_count": 120,
            "uptime": 86_400_u64,
            "temperature": 55.0,
            "gpu": null
        }),
    )
    .await;

    // The server does not Ack a Report. Prove it processed the frame without
    // tearing the connection down by completing a fresh SystemInfo handshake on
    // the same socket — the Ack only arrives if the read loop is still running.
    send_system_info(&mut sink, &mut reader, "post-report-handshake", Some(CAP_DEFAULT)).await;

    // The report also surfaces the server as online in the REST listing.
    let resp = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("GET /api/servers/{id} failed");
    assert_eq!(resp.status(), 200);

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// ping_result  (→ ping_record row, visible via /ping-tasks/{id}/records)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ping_result_persists_record_visible_via_api() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // Seed a ping task so the task id is a real, queryable handle. The agent is
    // not required to actually own the task for save_ping_result to insert a row
    // (it keys on task_id + server_id directly), but using a real task lets us
    // read it back through the public records endpoint.
    let create_resp = client
        .post(format!("{}/api/ping-tasks", base_url))
        .json(&json!({
            "name": "probe-google",
            "probe_type": "icmp",
            "target": "8.8.8.8",
            "interval": 60,
            "server_ids": [server_id]
        }))
        .send()
        .await
        .expect("create ping task failed");
    assert_eq!(create_resp.status(), 200, "ping task creation should succeed");
    let task_body: Value = create_resp.json().await.expect("parse ping task response");
    let task_id = task_body["data"]["id"]
        .as_str()
        .expect("ping task id missing")
        .to_string();

    // The pinger may have synced tasks to the agent; drain those pushes.
    drain_first_connect_pushes(&mut reader, 300).await;

    // Fixed RFC3339 timestamp (avoids a chrono dev-dependency); the records
    // query below uses a window that brackets it.
    let recorded_at = "2026-01-01T12:00:00Z";
    send_agent_frame(
        &mut sink,
        json!({
            "type": "ping_result",
            "task_id": task_id,
            "latency": 12.34,
            "success": true,
            "error": null,
            "time": recorded_at
        }),
    )
    .await;

    // Poll the records endpoint — the insert happens asynchronously in the read
    // loop, so allow a brief retry window.
    let from = "2026-01-01T00:00:00Z";
    let to = "2026-01-02T00:00:00Z";
    let mut found = false;
    for _ in 0..20 {
        let resp = client
            .get(format!("{}/api/ping-tasks/{}/records", base_url, task_id))
            .query(&[("from", from), ("to", to), ("server_id", server_id.as_str())])
            .send()
            .await
            .expect("GET ping records failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse ping records");
        let records = body["data"].as_array().expect("records should be array");
        if records.iter().any(|r| {
            (r["latency"].as_f64().unwrap_or_default() - 12.34).abs() < 1e-6
                && r["success"].as_bool() == Some(true)
        }) {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found, "ping_result should be persisted and queryable via records API");

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// task_result  (no pending waiter → save_task_result row via /tasks/{id}/results)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_task_result_without_waiter_persists_and_acks() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // Create a oneshot task but DO NOT run it, so no pending dispatch waiter is
    // registered. The agent's unsolicited task_result then takes the direct
    // save_task_result path keyed on the task_id we pass.
    let create_resp = client
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "oneshot",
            "name": "manual-result"
        }))
        .send()
        .await
        .expect("create task failed");
    assert_eq!(create_resp.status(), 200, "task creation should succeed");
    let task_body: Value = create_resp.json().await.expect("parse task response");
    let task_id = task_body["data"]["id"]
        .as_str()
        .expect("task id missing")
        .to_string();

    send_agent_frame(
        &mut sink,
        json!({
            "type": "task_result",
            "msg_id": "tr-msg-1",
            "task_id": task_id,
            "output": "hi\n",
            "exit_code": 0
        }),
    )
    .await;

    // The server Acks a task_result with the supplied msg_id.
    let ack = recv_until_type(&mut reader, "ack").await;
    assert_eq!(ack["msg_id"], "tr-msg-1", "task_result should be acked by msg_id");

    // And the row is visible through the task results endpoint.
    let mut found = false;
    for _ in 0..20 {
        let resp = client
            .get(format!("{}/api/tasks/{}/results", base_url, task_id))
            .send()
            .await
            .expect("GET task results failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse task results");
        let results = body["data"].as_array().expect("results should be array");
        if results
            .iter()
            .any(|r| r["output"] == "hi\n" && r["exit_code"].as_i64() == Some(0))
        {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found, "task_result should be persisted and queryable");

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// security_event  (cap granted → security_event row via /security/events)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_security_event_persists_when_capability_present() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Default caps include CAP_SECURITY_EVENTS, so the event is recorded.
    let (server_id, mut sink, _reader) = bring_up_agent(&client, &base_url).await;

    let source_ip = "203.0.113.42";
    send_agent_frame(
        &mut sink,
        json!({
            "type": "security_event",
            "event_type": "ssh_brute_force",
            "severity": "high",
            "source_ip": source_ip,
            "source_port": null,
            "username": null,
            "started_at": 1_700_000_000_i64,
            "ended_at": 1_700_000_060_i64,
            "first_seen": true,
            "detector_source": "journal",
            "evidence": {
                "kind": "ssh_brute_force",
                "failed_count": 12,
                "distinct_users": 1,
                "sample_users": ["root"],
                "invalid_user_count": 0,
                "window_seconds": 60,
                "threshold": 10
            }
        }),
    )
    .await;

    let mut found = false;
    for _ in 0..20 {
        let resp = client
            .get(format!(
                "{}/api/security/events?server_id={}",
                base_url, server_id
            ))
            .send()
            .await
            .expect("GET security events failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse security events");
        let items = body["data"]["items"]
            .as_array()
            .expect("items should be array");
        if items.iter().any(|e| e["source_ip"] == source_ip) {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found, "security_event should be persisted with CAP_SECURITY_EVENTS");

    let _ = sink.close().await;
}

#[tokio::test]
async fn test_security_event_dropped_when_capability_missing() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome");

    // CAP_DEFAULT includes CAP_SECURITY_EVENTS (256); clearing that bit revokes
    // it while keeping the rest of the default policy intact.
    let caps_without_security = CAP_DEFAULT & !serverbee_common::constants::CAP_SECURITY_EVENTS;
    send_system_info(
        &mut sink,
        &mut reader,
        "handshake-no-sec",
        Some(caps_without_security),
    )
    .await;
    drain_first_connect_pushes(&mut reader, 300).await;

    send_agent_frame(
        &mut sink,
        json!({
            "type": "security_event",
            "event_type": "port_scan",
            "severity": "medium",
            "source_ip": "198.51.100.7",
            "source_port": 22,
            "username": null,
            "started_at": 1_700_000_000_i64,
            "ended_at": 1_700_000_030_i64,
            "first_seen": true,
            "detector_source": "conntrack",
            "evidence": {
                "kind": "port_scan",
                "distinct_ports": 50,
                "sample_ports": [22, 80, 443],
                "total_attempts": 200,
                "window_seconds": 30,
                "threshold": 20,
                "blocked_count": 0
            }
        }),
    )
    .await;

    // Give the read loop a moment, then confirm nothing was persisted.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let resp = client
        .get(format!(
            "{}/api/security/events?server_id={}",
            base_url, server_id
        ))
        .send()
        .await
        .expect("GET security events failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse security events");
    let items = body["data"]["items"].as_array().expect("items array");
    assert!(
        items.is_empty(),
        "security_event must be dropped when CAP_SECURITY_EVENTS is revoked, got {items:?}"
    );

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// capability_denied  (msg_id present, no waiter → synthetic task_result row)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_capability_denied_writes_synthetic_task_result() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // A CapabilityDenied carrying a msg_id (treated as a task id) with no pending
    // exec waiter is persisted directly as a task_result with exit_code = -2.
    let denied_task_id = "denied-task-42";
    send_agent_frame(
        &mut sink,
        json!({
            "type": "capability_denied",
            "msg_id": denied_task_id,
            "session_id": null,
            "capability": "exec",
            "reason": "agent_capability_disabled"
        }),
    )
    .await;

    // No Ack is sent for CapabilityDenied; verify the synthetic row instead.
    let mut found = false;
    for _ in 0..20 {
        let resp = client
            .get(format!("{}/api/tasks/{}/results", base_url, denied_task_id))
            .send()
            .await
            .expect("GET task results failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse task results");
        let results = body["data"].as_array().expect("results array");
        if results.iter().any(|r| r["exit_code"].as_i64() == Some(-2)) {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        found,
        "capability_denied should persist a synthetic task_result with exit_code -2"
    );

    // The connection must stay alive after a CapabilityDenied frame.
    send_system_info(&mut sink, &mut reader, "post-denied-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// ip_changed  (→ DB IP update + audit + browser broadcast; agent stays alive)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ip_changed_updates_server_and_audits() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // The handshake reported ipv4 = 1.2.3.4; report a new address to trigger the
    // change branch (DB update + audit log + ServerIpChanged broadcast).
    let new_ipv4 = "5.6.7.8";
    send_agent_frame(
        &mut sink,
        json!({
            "type": "ip_changed",
            "ipv4": new_ipv4,
            "ipv6": null,
            "interfaces": [{
                "name": "eth0",
                "ipv4": [new_ipv4],
                "ipv6": []
            }]
        }),
    )
    .await;

    // The new IP is persisted on the server row.
    let mut updated = false;
    for _ in 0..20 {
        let resp = client
            .get(format!("{}/api/servers/{}", base_url, server_id))
            .send()
            .await
            .expect("GET server failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse server");
        if body["data"]["ipv4"] == new_ipv4 {
            updated = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(updated, "ip_changed should update the persisted server ipv4");

    // The change is audited under the `ip_changed` action.
    let audit_resp = client
        .get(format!("{}/api/audit-logs", base_url))
        .send()
        .await
        .expect("GET audit logs failed");
    assert_eq!(audit_resp.status(), 200);
    let audit_body: Value = audit_resp.json().await.expect("parse audit logs");
    let entries = audit_body["data"]["entries"]
        .as_array()
        .expect("entries array");
    assert!(
        entries
            .iter()
            .any(|e| e["action"].as_str() == Some("ip_changed")),
        "ip_changed should write an audit log entry, got {entries:?}"
    );

    // Connection survives the frame.
    send_system_info(&mut sink, &mut reader, "post-ipchange-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// capabilities_changed  (→ mirror persist + audit + browser broadcast)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_capabilities_changed_is_audited_and_mirrored() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // Report a temporary grant of a high-risk capability (terminal). This audits
    // `capability_temporarily_granted` and broadcasts CapabilitiesChanged.
    let new_caps = CAP_DEFAULT | serverbee_common::constants::CAP_TERMINAL;
    send_agent_frame(
        &mut sink,
        json!({
            "type": "capabilities_changed",
            "msg_id": "cap-change-1",
            "capabilities": new_caps,
            "temporary": [{
                "cap": "terminal",
                "granted_at": 1_700_000_000_i64,
                "expires_at": 1_700_003_600_i64
            }],
            "changes": [{
                "cap": "terminal",
                "action": "granted",
                "expires_at": 1_700_003_600_i64,
                "granted_by": "operator",
                "reason": "debugging"
            }]
        }),
    )
    .await;

    // The grant transition is audited.
    let mut audited = false;
    for _ in 0..20 {
        let resp = client
            .get(format!("{}/api/audit-logs", base_url))
            .send()
            .await
            .expect("GET audit logs failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse audit logs");
        let entries = body["data"]["entries"].as_array().expect("entries array");
        if entries
            .iter()
            .any(|e| e["action"].as_str() == Some("capability_temporarily_granted"))
        {
            audited = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        audited,
        "capabilities_changed grant should write a capability_temporarily_granted audit entry"
    );

    // The mirror column is updated; the server detail reflects the new bitmask.
    let resp = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("GET server failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse server");
    assert_eq!(
        body["data"]["capabilities"].as_u64(),
        Some(new_caps as u64),
        "capabilities mirror should reflect the agent-reported bitmask"
    );

    // Connection survives.
    send_system_info(&mut sink, &mut reader, "post-capchange-handshake", Some(new_caps)).await;

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// network_probe_results  (→ broadcast + persist; empty payload is a no-op save)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_network_probe_results_empty_is_accepted() {
    // An empty results vector exercises the broadcast + save_results no-op branch
    // without needing seeded probe targets, and proves the variant is routed.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    send_agent_frame(
        &mut sink,
        json!({
            "type": "network_probe_results",
            "results": []
        }),
    )
    .await;

    // The connection must remain usable afterwards.
    send_system_info(&mut sink, &mut reader, "post-probe-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// pong  (protocol-level liveness; touches the connection, no disconnect)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pong_frame_keeps_connection_alive() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    send_agent_frame(&mut sink, json!({ "type": "pong" })).await;

    // Still alive: a follow-up handshake is acked.
    send_system_info(&mut sink, &mut reader, "post-pong-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}

// ---------------------------------------------------------------------------
// Malformed frame  (server logs + ignores; must NOT disconnect)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_invalid_json_frame_does_not_disconnect() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // Unknown tag → serde_json::from_str fails; the read loop logs a warning and
    // continues without closing the socket.
    send_agent_frame(&mut sink, json!({ "type": "totally_unknown_variant" })).await;

    // The connection is still serviced afterwards.
    send_system_info(&mut sink, &mut reader, "post-garbage-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}
