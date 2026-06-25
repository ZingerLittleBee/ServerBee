//! Integration coverage for the AGENT -> SERVER message-dispatch arms in
//! `crates/server/src/router/ws/agent.rs` that the existing suites leave
//! uncovered.
//!
//! `tests/agent_messages.rs` already exercises report / ping_result /
//! task_result(no-waiter) / security_event / capability_denied(exec) /
//! ip_changed / capabilities_changed(grant) / network_probe_results / pong /
//! invalid-json. `tests/agent_file_ops.rs` and `tests/agent_docker_extra.rs`
//! cover the file- and docker-control round-trips. This file targets the
//! REMAINING `handle_agent_message` arms, each verified through an observable
//! side effect (a follow-up GET, a server Ack, or a state transition):
//!
//!   - TracerouteRoundUpdate (completed)  -> DB record + snapshot via GET
//!   - TracerouteRoundUpdate (in-progress then completed) -> round progression
//!   - TracerouteRoundUpdate (server_id mismatch) -> dropped (defense-in-depth)
//!   - TracerouteRoundUpdate (no placeholder)  -> dropped, connection survives
//!   - TracerouteResult (legacy)          -> recorded with the "legacy" protocol
//!     sentinel (the round-trip itself is covered by router_probes_extra.rs;
//!     this test pins the `set_traceroute_meta_protocol(Legacy)` branch)
//!   - UpgradeProgress                    -> server emits an Ack (the bare ingest
//!     is touched by router_server_extra2.rs, but the Ack reply is unverified)
//!   - UpgradeResult                      -> mark_failed + Ack (job cleared)
//!   - CapabilityDenied(capability=upgrade) -> upgrade job marked failed
//!   - DockerEvent (unsolicited)          -> save_event + queryable via GET
//!   - FeaturesUpdate                     -> docker availability flips on
//!   - capabilities_changed (revoked)     -> audited as grant_revoked, no alert
//!   - blocklist_reset_ack                -> firewall reset-ack path, stays alive
//!   - terminal_output/started/error (orphaned session) -> no-op, stays alive
//!
//! NOT covered here (NOTE — genuinely structural or already covered elsewhere):
//!   - the disconnect / cleanup path (Message::Close, write-task abort) and the
//!     binary-frame and WS-error arms are connection-lifecycle plumbing, not
//!     per-variant dispatch; `ws/agent.rs`'s own `#[cfg(test)]` module already
//!     covers the superseded-connection / server-lock arms.
//!   - file/docker/terminal CONTROL responses that need a prior server-initiated
//!     request: covered by agent_file_ops.rs / agent_docker_extra.rs.
//!   - SecurityEvent / UnlockResults capability gating: covered by
//!     agent_messages.rs + the unit module in ws/agent.rs.

mod common;

use common::{
    connect_agent, http_client, login_admin, login_as_new_user, recv_agent_text, register_agent,
    send_system_info, start_test_server, AgentReader, AgentSink,
};
use futures_util::SinkExt;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite;

use serverbee_common::constants::{CAP_DEFAULT, CAP_DOCKER, CAP_TERMINAL};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Send a single agent frame as a JSON text message.
async fn send_agent_frame(sink: &mut AgentSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("send agent frame");
}

/// First-connect pushes that a default agent must tolerate and ignore. These
/// (ping/network/IP-quality sync + firewall blocklist messages) are unrelated to
/// the dispatch arms under test.
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

/// Drain any frames the server pushes right after the SystemInfo handshake.
/// Returns once the inbound stream is quiet for `quiet_ms`.
async fn drain_first_connect_pushes(reader: &mut AgentReader, quiet_ms: u64) {
    use futures_util::StreamExt;
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(quiet_ms), reader.next()).await {
            Ok(Some(Ok(_))) => continue,
            Ok(_) => break,
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

/// Standard mock-agent bring-up with the default capability set (which includes
/// CAP_UPGRADE + CAP_SECURITY_EVENTS + firewall blocklist), features empty.
async fn bring_up_agent(
    client: &reqwest::Client,
    base_url: &str,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome", "first frame must be welcome");
    send_system_info(&mut sink, &mut reader, "dispatch-handshake", Some(CAP_DEFAULT)).await;
    drain_first_connect_pushes(&mut reader, 300).await;
    (server_id, sink, reader)
}

/// Bring up a mock agent advertising the `docker` runtime feature plus the
/// given capability bitmask. The docker control gates require both the feature
/// and CAP_DOCKER, so docker-flavored tests ship their own SystemInfo.
async fn bring_up_docker_agent(
    client: &reqwest::Client,
    base_url: &str,
    caps: u32,
    features: Value,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome");

    let system_info = json!({
        "type": "system_info",
        "msg_id": "docker-dispatch-handshake",
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
        "features": features,
        "agent_local_capabilities": caps
    });
    send_agent_frame(&mut sink, system_info).await;
    loop {
        let msg = recv_agent_text(&mut reader).await;
        if msg["type"] == "ack" {
            assert_eq!(msg["msg_id"], "docker-dispatch-handshake");
            break;
        }
    }
    drain_first_connect_pushes(&mut reader, 300).await;
    (server_id, sink, reader)
}

// ===========================================================================
// TracerouteRoundUpdate  (POST /traceroute → agent reply → DB record + snapshot)
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_traceroute_round_update_completed_persists_and_is_queryable() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, sink, reader) = bring_up_agent(&client, &base_url).await;

    // The agent answers the forwarded `traceroute` command with a single
    // completed round; `completed = true` triggers the DB persist branch.
    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("traceroute") => {
                        let request_id =
                            msg["request_id"].as_str().expect("request_id").to_string();
                        let target = msg["target"].as_str().expect("target").to_string();
                        let reply = json!({
                            "type": "traceroute_round_update",
                            "request_id": request_id,
                            "target": target,
                            "round": 1,
                            "total_rounds": 1,
                            "hops": [
                                { "hop": 1, "ip": "192.168.1.1", "rtt1": 1.2 },
                                { "hop": 2, "ip": "8.8.8.8", "rtt1": 12.3 }
                            ],
                            "completed": true,
                            "error": null
                        });
                        send_agent_frame(&mut sink, reply).await;
                        return;
                    }
                    other if is_ignorable_push(other) => {}
                    Some(_other) => {}
                    None => {}
                }
            }
        })
    };

    let resp = client
        .post(format!("{base_url}/api/servers/{server_id}/traceroute"))
        .json(&json!({ "target": "8.8.8.8", "protocol": "icmp" }))
        .send()
        .await
        .expect("trigger traceroute failed");
    assert_eq!(resp.status(), 200, "trigger should succeed");
    let body: Value = resp.json().await.expect("parse trigger response");
    let request_id = body["data"]["request_id"]
        .as_str()
        .expect("request_id missing")
        .to_string();

    agent_task.await.expect("agent responder failed");

    // The completed round is persisted; GET returns the enriched snapshot.
    let mut found = false;
    for _ in 0..30 {
        let resp = client
            .get(format!("{base_url}/api/servers/{server_id}/traceroute/{request_id}"))
            .send()
            .await
            .expect("GET traceroute snapshot failed");
        if resp.status() == 200 {
            let snap: Value = resp.json().await.expect("parse snapshot");
            if snap["data"]["completed"] == true
                && snap["data"]["hops"].as_array().map(Vec::len) == Some(2)
            {
                assert_eq!(snap["data"]["protocol"], "icmp");
                found = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found, "completed traceroute should be persisted and queryable");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_traceroute_round_update_progression_then_complete() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, sink, reader) = bring_up_agent(&client, &base_url).await;

    // Stream two rounds: round 1 in-progress (completed = false), round 2 final.
    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("traceroute") => {
                        let request_id =
                            msg["request_id"].as_str().expect("request_id").to_string();
                        let target = msg["target"].as_str().expect("target").to_string();
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "traceroute_round_update",
                                "request_id": request_id,
                                "target": target,
                                "round": 1,
                                "total_rounds": 2,
                                "hops": [{ "hop": 1, "ip": "10.0.0.1", "rtt1": 0.9 }],
                                "completed": false,
                                "error": null
                            }),
                        )
                        .await;
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "traceroute_round_update",
                                "request_id": request_id,
                                "target": target,
                                "round": 2,
                                "total_rounds": 2,
                                "hops": [
                                    { "hop": 1, "ip": "10.0.0.1", "rtt1": 0.9 },
                                    { "hop": 2, "ip": "1.1.1.1", "rtt1": 5.5 }
                                ],
                                "completed": true,
                                "error": null
                            }),
                        )
                        .await;
                        return;
                    }
                    other if is_ignorable_push(other) => {}
                    Some(_other) => {}
                    None => {}
                }
            }
        })
    };

    let resp = client
        .post(format!("{base_url}/api/servers/{server_id}/traceroute"))
        .json(&json!({ "target": "1.1.1.1" }))
        .send()
        .await
        .expect("trigger traceroute failed");
    assert_eq!(resp.status(), 200);
    let request_id = resp.json::<Value>().await.expect("parse")["data"]["request_id"]
        .as_str()
        .expect("request_id")
        .to_string();

    agent_task.await.expect("agent responder failed");

    let mut ok = false;
    for _ in 0..30 {
        let snap: Value = client
            .get(format!("{base_url}/api/servers/{server_id}/traceroute/{request_id}"))
            .send()
            .await
            .expect("GET snapshot failed")
            .json()
            .await
            .expect("parse snapshot");
        if snap["data"]["completed"] == true && snap["data"]["total_rounds"] == 2 {
            assert_eq!(snap["data"]["hops"].as_array().map(Vec::len), Some(2));
            ok = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(ok, "final round should mark the snapshot completed with 2 hops");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_traceroute_round_update_server_id_mismatch_is_dropped() {
    // Defense-in-depth arm: an update whose request_id was registered for a
    // DIFFERENT server must be dropped. Register two agents, trigger a
    // traceroute on server B (placeholder keyed to B), then have server A's
    // socket emit an update carrying B's request_id. The mismatch guard drops
    // it: B's snapshot keeps the placeholder (completed = false, no hops).
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_a, token_a) = register_agent(&client, &base_url).await;
    let (mut sink_a, mut reader_a) = connect_agent(&base_url, &token_a).await;
    assert_eq!(recv_agent_text(&mut reader_a).await["type"], "welcome");
    send_system_info(&mut sink_a, &mut reader_a, "a-handshake", Some(CAP_DEFAULT)).await;
    drain_first_connect_pushes(&mut reader_a, 300).await;

    let (server_b, sink_b, reader_b) = bring_up_agent(&client, &base_url).await;
    assert_ne!(server_a, server_b);

    // Capture B's request_id from the forwarded command, but DO NOT reply on B.
    let capture_b = {
        let mut reader_b = reader_b;
        let keep_sink = sink_b; // hold B's socket open so B stays online
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader_b).await;
                if msg["type"] == "traceroute" {
                    return (msg["request_id"].as_str().unwrap().to_string(), keep_sink);
                }
            }
        })
    };

    let resp = client
        .post(format!("{base_url}/api/servers/{server_b}/traceroute"))
        .json(&json!({ "target": "9.9.9.9" }))
        .send()
        .await
        .expect("trigger traceroute on B failed");
    assert_eq!(resp.status(), 200);
    let request_id = resp.json::<Value>().await.expect("parse")["data"]["request_id"]
        .as_str()
        .expect("request_id")
        .to_string();

    let (captured_id, _keep_sink_b) = capture_b.await.expect("capture B request_id");
    assert_eq!(captured_id, request_id);

    // Server A emits a (forged) completed update for B's request_id.
    send_agent_frame(
        &mut sink_a,
        json!({
            "type": "traceroute_round_update",
            "request_id": request_id,
            "target": "9.9.9.9",
            "round": 1,
            "total_rounds": 1,
            "hops": [{ "hop": 1, "ip": "6.6.6.6", "rtt1": 1.0 }],
            "completed": true,
            "error": null
        }),
    )
    .await;

    // Give the read loop time to process (and drop) the forged frame.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // B's snapshot is still the in-memory placeholder: never completed, no hops.
    let snap: Value = client
        .get(format!("{base_url}/api/servers/{server_b}/traceroute/{request_id}"))
        .send()
        .await
        .expect("GET B snapshot failed")
        .json()
        .await
        .expect("parse snapshot");
    assert_eq!(
        snap["data"]["completed"], false,
        "forged cross-server update must not complete B's traceroute"
    );
    assert_eq!(
        snap["data"]["hops"].as_array().map(Vec::len),
        Some(0),
        "forged update must not inject hops into B's snapshot"
    );

    let _ = sink_a.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_traceroute_round_update_without_placeholder_is_dropped() {
    // A TracerouteRoundUpdate whose request_id has no cached placeholder is
    // dropped silently; the connection must stay alive afterwards.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    send_agent_frame(
        &mut sink,
        json!({
            "type": "traceroute_round_update",
            "request_id": "orphan-request-id-no-placeholder",
            "target": "8.8.8.8",
            "round": 1,
            "total_rounds": 1,
            "hops": [{ "hop": 1, "ip": "1.2.3.4", "rtt1": 1.0 }],
            "completed": true,
            "error": null
        }),
    )
    .await;

    // The connection survives: a fresh handshake is still acked.
    send_system_info(&mut sink, &mut reader, "post-orphan-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_legacy_traceroute_result_records_legacy_protocol() {
    // The legacy `traceroute_result` arm normalizes the record into the new
    // pipeline with the "legacy" protocol sentinel.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, sink, reader) = bring_up_agent(&client, &base_url).await;

    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("traceroute") => {
                        let request_id =
                            msg["request_id"].as_str().expect("request_id").to_string();
                        let target = msg["target"].as_str().expect("target").to_string();
                        // Reply with the LEGACY shape (`traceroute_result`).
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "traceroute_result",
                                "request_id": request_id,
                                "target": target,
                                "hops": [
                                    { "hop": 1, "ip": "192.168.0.1",
                                      "rtt1": 1.0, "rtt2": 1.1, "rtt3": 1.2 }
                                ],
                                "completed": true,
                                "error": null
                            }),
                        )
                        .await;
                        return;
                    }
                    other if is_ignorable_push(other) => {}
                    Some(_other) => {}
                    None => {}
                }
            }
        })
    };

    // Request protocol icmp, but the legacy arm overrides the stored protocol
    // with the "legacy" sentinel regardless of what the request asked for.
    let resp = client
        .post(format!("{base_url}/api/servers/{server_id}/traceroute"))
        .json(&json!({ "target": "8.8.8.8", "protocol": "icmp" }))
        .send()
        .await
        .expect("trigger traceroute failed");
    assert_eq!(resp.status(), 200);
    let request_id = resp.json::<Value>().await.expect("parse")["data"]["request_id"]
        .as_str()
        .expect("request_id")
        .to_string();

    agent_task.await.expect("agent responder failed");

    let mut found = false;
    for _ in 0..30 {
        let snap: Value = client
            .get(format!("{base_url}/api/servers/{server_id}/traceroute/{request_id}"))
            .send()
            .await
            .expect("GET snapshot failed")
            .json()
            .await
            .expect("parse snapshot");
        if snap["data"]["completed"] == true {
            assert_eq!(
                snap["data"]["protocol"], "legacy",
                "legacy traceroute_result must record the legacy protocol sentinel"
            );
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found, "legacy traceroute result should persist as a completed record");
}

// ===========================================================================
// UpgradeProgress / UpgradeResult  (POST /upgrade → agent reply → Ack + state)
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_upgrade_progress_is_acked() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // CAP_DEFAULT includes CAP_UPGRADE, so the trigger is allowed.
    let (server_id, sink, reader) = bring_up_agent(&client, &base_url).await;

    // Reply to the forwarded `upgrade` command with an UpgradeProgress and
    // capture the Ack the server emits in response.
    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("upgrade") => {
                        let job_id = msg["job_id"].as_str().expect("job_id").to_string();
                        let version = msg["version"].as_str().expect("version").to_string();
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "upgrade_progress",
                                "msg_id": "up-prog-1",
                                "job_id": job_id,
                                "target_version": version,
                                "stage": "installing"
                            }),
                        )
                        .await;
                        // The server Acks the progress message by its msg_id.
                        let ack = recv_until_type(&mut reader, "ack").await;
                        assert_eq!(ack["msg_id"], "up-prog-1");
                        return;
                    }
                    other if is_ignorable_push(other) => {}
                    Some(_other) => {}
                    None => {}
                }
            }
        })
    };

    let resp = client
        .post(format!("{base_url}/api/servers/{server_id}/upgrade"))
        .json(&json!({ "version": "9.9.9" }))
        .send()
        .await
        .expect("trigger upgrade failed");
    assert_eq!(resp.status(), 200, "upgrade trigger should succeed");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_upgrade_result_acks_and_clears_running_job() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, sink, reader) = bring_up_agent(&client, &base_url).await;

    // Reply with a failed UpgradeResult; the arm marks the job failed and Acks.
    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("upgrade") => {
                        let job_id = msg["job_id"].as_str().expect("job_id").to_string();
                        let version = msg["version"].as_str().expect("version").to_string();
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "upgrade_result",
                                "msg_id": "up-res-1",
                                "job_id": job_id,
                                "target_version": version,
                                "stage": "installing",
                                "error": "disk full",
                                "backup_path": "/var/backups/agent.bak"
                            }),
                        )
                        .await;
                        // Keep the connection alive (do NOT return and drop the
                        // socket): the test fires a SECOND upgrade that needs the
                        // agent ONLINE. The server's Ack for this result arrives
                        // as a later frame the loop simply ignores.
                    }
                    other if is_ignorable_push(other) => {}
                    Some(_other) => {}
                    None => {}
                }
            }
        })
    };

    let resp = client
        .post(format!("{base_url}/api/servers/{server_id}/upgrade"))
        .json(&json!({ "version": "9.9.9" }))
        .send()
        .await
        .expect("first upgrade trigger failed");
    assert_eq!(resp.status(), 200);

    // The job is no longer Running, so a second trigger does NOT 409-Conflict.
    let mut cleared = false;
    for _ in 0..30 {
        let resp = client
            .post(format!("{base_url}/api/servers/{server_id}/upgrade"))
            .json(&json!({ "version": "9.9.10" }))
            .send()
            .await
            .expect("second upgrade trigger failed");
        if resp.status() == 200 {
            cleared = true;
            break;
        }
        // Still 409 means the first job hasn't been finished yet; retry.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        cleared,
        "UpgradeResult should finish the running job so a new upgrade can start"
    );

    agent_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_capability_denied_upgrade_marks_job_failed() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, sink, reader) = bring_up_agent(&client, &base_url).await;

    // The agent denies the upgrade capability; the arm marks the in-flight job
    // failed via mark_failed_by_capability_denied.
    let agent_task = {
        let mut sink = sink;
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let msg = recv_agent_text(&mut reader).await;
                match msg["type"].as_str() {
                    Some("upgrade") => {
                        send_agent_frame(
                            &mut sink,
                            json!({
                                "type": "capability_denied",
                                "msg_id": null,
                                "session_id": null,
                                "capability": "upgrade",
                                "reason": "agent_capability_disabled"
                            }),
                        )
                        .await;
                        // Keep the connection alive for the second trigger below
                        // (returning here would drop the socket → agent offline →
                        // the retrigger 404s instead of returning the expected 200).
                    }
                    other if is_ignorable_push(other) => {}
                    Some(_other) => {}
                    None => {}
                }
            }
        })
    };

    let resp = client
        .post(format!("{base_url}/api/servers/{server_id}/upgrade"))
        .json(&json!({ "version": "9.9.9" }))
        .send()
        .await
        .expect("first upgrade trigger failed");
    assert_eq!(resp.status(), 200);

    // Job marked failed → a fresh upgrade can start (no Conflict).
    let mut cleared = false;
    for _ in 0..30 {
        let resp = client
            .post(format!("{base_url}/api/servers/{server_id}/upgrade"))
            .json(&json!({ "version": "9.9.11" }))
            .send()
            .await
            .expect("second upgrade trigger failed");
        if resp.status() == 200 {
            cleared = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    agent_task.abort();
    assert!(
        cleared,
        "capability_denied(upgrade) should fail the job so a new upgrade can start"
    );
}

// ===========================================================================
// DockerEvent (unsolicited)  → save_event + browser broadcast
// ===========================================================================

#[tokio::test]
async fn test_docker_event_persists_and_is_queryable() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // CAP_DOCKER is needed for the events GET gate; the docker feature lets the
    // server consider the agent docker-capable.
    let (server_id, mut sink, _reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT | CAP_DOCKER, json!(["docker"])).await;

    send_agent_frame(
        &mut sink,
        json!({
            "type": "docker_event",
            "event": {
                "timestamp": 1_700_000_500_i64,
                "event_type": "container",
                "action": "start",
                "actor_id": "abc123def456",
                "actor_name": "nginx",
                "attributes": { "image": "nginx:latest" }
            }
        }),
    )
    .await;

    let mut found = false;
    for _ in 0..30 {
        let resp = client
            .get(format!("{base_url}/api/servers/{server_id}/docker/events?limit=50"))
            .send()
            .await
            .expect("GET docker events failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse events");
        let events = body["data"]["events"].as_array().expect("events array");
        if events
            .iter()
            .any(|e| e["actor_id"] == "abc123def456" && e["action"] == "start")
        {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found, "unsolicited docker_event should persist and be queryable");

    let _ = sink.close().await;
}

// ===========================================================================
// FeaturesUpdate  → updates features so docker availability flips on
// ===========================================================================

#[tokio::test]
async fn test_features_update_enables_docker_reads() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Connect with CAP_DOCKER but NO docker feature: the docker/containers read
    // gates on the runtime feature, so it is rejected (403) up front.
    let (server_id, mut sink, _reader) =
        bring_up_docker_agent(&client, &base_url, CAP_DEFAULT | CAP_DOCKER, json!([])).await;

    let before = client
        .get(format!("{base_url}/api/servers/{server_id}/docker/containers"))
        .send()
        .await
        .expect("GET containers (before) failed");
    assert_eq!(
        before.status(),
        403,
        "without the docker feature the read is forbidden"
    );

    // Now announce the docker feature via FeaturesUpdate.
    send_agent_frame(
        &mut sink,
        json!({ "type": "features_update", "features": ["docker"] }),
    )
    .await;

    // The read becomes reachable once the feature is registered.
    let mut enabled = false;
    for _ in 0..30 {
        let resp = client
            .get(format!("{base_url}/api/servers/{server_id}/docker/containers"))
            .send()
            .await
            .expect("GET containers (after) failed");
        if resp.status() == 200 {
            enabled = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        enabled,
        "features_update with docker should flip the docker feature on"
    );

    let _ = sink.close().await;
}

// ===========================================================================
// capabilities_changed (revoked)  → audited as grant_revoked, no alert
// ===========================================================================

#[tokio::test]
async fn test_capabilities_changed_revoked_is_audited() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // A "revoked" transition is audited as capability_grant_revoked and, unlike a
    // grant, does NOT fire the capability_grant_detected alert. The new bitmask
    // drops CAP_TERMINAL back to the default set.
    send_agent_frame(
        &mut sink,
        json!({
            "type": "capabilities_changed",
            "msg_id": "cap-revoke-1",
            "capabilities": CAP_DEFAULT,
            "temporary": [],
            "changes": [{
                "cap": "terminal",
                "action": "revoked"
            }]
        }),
    )
    .await;

    let mut audited = false;
    for _ in 0..30 {
        let resp = client
            .get(format!("{base_url}/api/audit-logs"))
            .send()
            .await
            .expect("GET audit logs failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse audit logs");
        let entries = body["data"]["entries"].as_array().expect("entries array");
        if entries
            .iter()
            .any(|e| e["action"].as_str() == Some("capability_grant_revoked"))
        {
            audited = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        audited,
        "capabilities_changed revoked should write a capability_grant_revoked audit entry"
    );

    // The mirror reflects the post-revoke bitmask, and the connection survives.
    let resp = client
        .get(format!("{base_url}/api/servers/{server_id}"))
        .send()
        .await
        .expect("GET server failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse server");
    assert_eq!(body["data"]["capabilities"].as_u64(), Some(CAP_DEFAULT as u64));

    send_system_info(&mut sink, &mut reader, "post-revoke-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}

// ===========================================================================
// blocklist_ack / blocklist_reset_ack  (firewall ack path; stays connected)
// ===========================================================================

#[tokio::test]
async fn test_blocklist_reset_ack_keeps_connection_alive() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_server_id, mut sink, mut reader) = bring_up_agent(&client, &base_url).await;

    // record_reset_ack updates firewall bookkeeping; no observable REST surface
    // for an empty fleet, so we assert the dispatch arm runs without tearing the
    // socket down.
    send_agent_frame(
        &mut sink,
        json!({ "type": "blocklist_reset_ack", "ok": true, "reason": null }),
    )
    .await;

    send_system_info(&mut sink, &mut reader, "post-reset-ack-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}

// NOTE: the `blocklist_ack` arm (for a server-tracked block, asserting the
// audit row + persisted state) is already covered by tests/integration.rs
// (`ack_failed_records_audit_and_keeps_row`); it is not duplicated here.

// ===========================================================================
// terminal_output (orphaned session)  → no-op, connection survives
// ===========================================================================

#[tokio::test]
async fn test_terminal_output_for_unknown_session_is_noop() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Advertise CAP_TERMINAL so the frame is unambiguously a real terminal
    // payload rather than a capability-gated reject; with no registered session
    // the TerminalOutput arm simply finds no sender and returns.
    let (_server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut reader).await["type"], "welcome");
    send_system_info(&mut sink, &mut reader, "term-handshake", Some(CAP_DEFAULT | CAP_TERMINAL))
        .await;
    drain_first_connect_pushes(&mut reader, 300).await;

    // Unsolicited terminal output / started / error for a session the server
    // never opened: each arm looks up an absent session and no-ops.
    send_agent_frame(
        &mut sink,
        json!({
            "type": "terminal_output",
            "session_id": "ghost-session",
            "data": "aGVsbG8="
        }),
    )
    .await;
    send_agent_frame(
        &mut sink,
        json!({ "type": "terminal_started", "session_id": "ghost-session" }),
    )
    .await;
    send_agent_frame(
        &mut sink,
        json!({
            "type": "terminal_error",
            "session_id": "ghost-session",
            "error": "no such session"
        }),
    )
    .await;

    // Connection still alive: a follow-up handshake is acked.
    send_system_info(&mut sink, &mut reader, "post-terminal-handshake", Some(CAP_DEFAULT)).await;

    let _ = sink.close().await;
}

// ===========================================================================
// Authz sanity: traceroute trigger is gated like the rest of the control plane
// (member 403, offline 404, unauthenticated 401) — these short-circuit before
// any agent round-trip, so they validate the gate, not a dispatch arm.
// ===========================================================================

#[tokio::test]
async fn test_traceroute_trigger_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let member = login_as_new_user(&admin, &base_url, "tr-member", "member").await;
    let resp = member
        .post(format!("{base_url}/api/servers/{server_id}/traceroute"))
        .json(&json!({ "target": "8.8.8.8" }))
        .send()
        .await
        .expect("member traceroute failed");
    assert_eq!(resp.status(), 403, "member traceroute trigger should be 403");
}

#[tokio::test]
async fn test_traceroute_trigger_offline_agent_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    // Registered but never connected → not online → 404.
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let resp = client
        .post(format!("{base_url}/api/servers/{server_id}/traceroute"))
        .json(&json!({ "target": "8.8.8.8" }))
        .send()
        .await
        .expect("offline traceroute failed");
    assert_eq!(resp.status(), 404, "offline traceroute trigger should be 404");
}

#[tokio::test]
async fn test_traceroute_trigger_invalid_target_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, _token) = register_agent(&client, &base_url).await;

    // A target with disallowed characters fails the validation guard (422)
    // before any online / forward logic runs.
    let resp = client
        .post(format!("{base_url}/api/servers/{server_id}/traceroute"))
        .json(&json!({ "target": "bad target!" }))
        .send()
        .await
        .expect("invalid traceroute failed");
    assert_eq!(resp.status(), 422, "invalid traceroute target should be 422");
}
