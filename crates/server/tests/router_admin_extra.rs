//! Router-level integration tests targeting branches the existing router test
//! files leave uncovered. Each test exercises the real Axum router over HTTP (and,
//! where the endpoint forwards to an agent, a mock-agent WebSocket responder)
//! against a freshly migrated, randomly-bound test server (see
//! `tests/common/mod.rs`). Every test gets its own temp DB, so resource names
//! never collide across tests.
//!
//! Scope (gaps NOT already covered by router_content_admin.rs /
//! router_auth_user.rs / router_security_rate.rs / router_widget_dashboard.rs /
//! widget_module_integration.rs / agent_messages.rs):
//!
//! - task.rs:    run_task scheduled HAPPY PATH via a live mock agent (the actual
//!               exec dispatch + TaskResult round-trip that persists a result row),
//!               create-task numeric validation arms (timeout=0, retry_interval<1),
//!               update-task numeric validation + not-found arms, list ?type= filter.
//! - security.rs delete-event 200 happy path, get-event 200 happy path, and
//!               stats/list over REAL seeded rows (group_by=source_ip, since/until
//!               time filters) — the existing file only hits the empty/no-match arms.
//! - auth.rs:    logout with no cookie (no-op 200), 2FA disable SUCCESS path,
//!               change_password empty-new-password validation arm.
//! - widget_module.rs: ?url= scheme rejection + malformed-url rejection branches
//!               in enforce_url_safety (the SSRF private/loopback arms are already
//!               covered in widget_module_integration.rs).
//!
//! NOTE: where an endpoint needs a live agent we cannot satisfy deterministically
//! (no real exec process), we stand up a mock-agent responder that echoes a
//! TaskResult; the agent never actually runs a command, but the server's dispatch
//! → pending-request → persist round-trip is fully exercised.

mod common;

use common::{
    connect_agent, create_server, http_client, login_admin, login_as_new_user, recv_agent_text,
    register_agent, send_system_info, start_test_server, AgentReader, AgentSink,
};
use futures_util::SinkExt;
use serde_json::{json, Value};
// CAP_DEFAULT (1852) is the agent's default policy; it deliberately does NOT
// include CAP_EXEC, so scheduled-task dispatch tests OR CAP_EXEC in explicitly.
use serverbee_common::constants::{CAP_DEFAULT, CAP_EXEC};
use tokio_tungstenite::tungstenite;

// ---------------------------------------------------------------------------
// Mock-agent helpers
// ---------------------------------------------------------------------------

/// Register + connect a mock agent, consume the welcome frame, complete the
/// SystemInfo handshake with the given capability bitmask, and drain the
/// first-connect pushes a default agent receives (ping/network sync + firewall
/// blocklist reset/sync). Returns the server id plus the WebSocket halves so the
/// caller can either drop the reader or spawn a responder loop.
async fn bring_up_agent(
    client: &reqwest::Client,
    base_url: &str,
    caps: u32,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_system_info(&mut sink, &mut reader, "handshake", Some(caps)).await;
    drain_first_connect_pushes(&mut reader, 250).await;
    (server_id, sink, reader)
}

/// Drain any frames the server pushes right after the SystemInfo handshake
/// (ping/network/IP-quality sync + firewall blocklist reset/sync for a default
/// agent). Returns once the inbound stream is quiet for `quiet_ms`.
async fn drain_first_connect_pushes(reader: &mut AgentReader, quiet_ms: u64) {
    use futures_util::StreamExt;
    loop {
        match tokio::time::timeout(
            std::time::Duration::from_millis(quiet_ms),
            reader.next(),
        )
        .await
        {
            Ok(Some(Ok(_))) => {}
            // Quiet window elapsed, stream ended, or a read error: stop draining.
            _ => break,
        }
    }
}

/// Send a single agent frame as a JSON text message.
async fn send_agent_frame(sink: &mut AgentSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("send agent frame");
}

// ===========================================================================
// task.rs — run_task scheduled HAPPY PATH (live agent dispatch round-trip)
// ===========================================================================

/// POST /api/tasks/{id}/run on a SCHEDULED task with a connected agent dispatches
/// an `exec` to the agent; the agent's `task_result` reply is correlated back to
/// the pending request and persisted as a row queryable via /tasks/{id}/results.
/// This is the one run_task branch the no-agent tests in router_content_admin.rs
/// explicitly skip ("with no agent it would still flip last_run_at").
///
/// Uses a multi-threaded runtime so the spawned mock-agent responder runs on its
/// own worker thread: the `/run` handler blocks on the agent's reply, and on a
/// single-threaded runtime under CPU contention the responder can be starved past
/// its recv timeout, leaving the handler waiting forever. A second worker thread
/// keeps the responder and the blocked handler making progress concurrently.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_scheduled_task_dispatches_to_agent_and_persists_result() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // A default agent reports CAP_EXEC, so the scheduler dispatch is not gated.
    let (server_id, mut sink, mut reader) =
        bring_up_agent(&client, &base_url, CAP_DEFAULT | CAP_EXEC).await;

    // Create a scheduled task targeting the connected agent.
    let created: Value = client
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo from-agent",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "name": "Dispatch me",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .expect("create scheduled task failed")
        .json()
        .await
        .expect("parse create response");
    let task_id = created["data"]["id"].as_str().expect("task id").to_string();

    // Mock-agent responder: ignore first-connect noise, and on the first `exec`
    // reply with a task_result echoing the correlation id (sent as `task_id`).
    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some("exec") => {
                    let correlation = msg["task_id"].as_str().expect("exec task_id missing");
                    send_agent_frame(
                        &mut sink,
                        json!({
                            "type": "task_result",
                            "msg_id": "exec-reply-1",
                            "task_id": correlation,
                            "output": "from-agent\n",
                            "exit_code": 0
                        }),
                    )
                    .await;
                    return;
                }
                // First-connect noise from a default agent: ignore.
                Some("ping_tasks_sync")
                | Some("network_probe_sync")
                | Some("blocklist_reset")
                | Some("blocklist_sync")
                | Some("blocklist_add")
                | Some("blocklist_remove") => {}
                _ => {}
            }
        }
    });

    // Trigger the run. The handler kicks off execute_scheduled_task and returns
    // the (re-fetched) task with last_run_at set.
    let run = client
        .post(format!("{}/api/tasks/{}/run", base_url, task_id))
        .send()
        .await
        .expect("run request failed");
    assert_eq!(run.status(), 200, "scheduled run should be accepted");
    let run_body: Value = run.json().await.expect("parse run response");
    assert!(
        run_body["data"]["last_run_at"].is_string(),
        "a successful manual trigger updates last_run_at"
    );

    agent_task.await.expect("agent responder task panicked");

    // The agent's TaskResult is correlated to the pending request and written as
    // a result row. Poll because the write happens on a spawned join_set task.
    let mut found = false;
    for _ in 0..40 {
        let resp = client
            .get(format!("{}/api/tasks/{}/results", base_url, task_id))
            .send()
            .await
            .expect("GET task results failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("parse results");
        let results = body["data"].as_array().expect("results array");
        if results
            .iter()
            .any(|r| r["output"] == "from-agent\n" && r["exit_code"].as_i64() == Some(0))
        {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        found,
        "agent task_result should be persisted via the dispatch/pending-request path"
    );
}

/// POST /api/tasks/{id}/run twice in quick succession: the second trigger lands
/// while the first run still holds the active-runs slot, so it is rejected with
/// 409 Conflict. We hold the agent's reply until both runs have been attempted.
///
/// Multi-threaded runtime (see the dispatch test above): the spawned responder
/// holds the exec on its own worker thread while the main task fires the second
/// trigger, so the overlap window is reliable under CPU contention.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_scheduled_task_while_running_is_conflict() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, mut reader) =
        bring_up_agent(&client, &base_url, CAP_DEFAULT | CAP_EXEC).await;

    let created: Value = client
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "sleep-ish",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "name": "Overlap",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .expect("create scheduled task failed")
        .json()
        .await
        .expect("parse create response");
    let task_id = created["data"]["id"].as_str().expect("task id").to_string();

    // Responder that holds the exec without replying for a moment so the active
    // run stays in-flight while we fire the second trigger.
    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            if msg["type"].as_str() == Some("exec") {
                let correlation = msg["task_id"].as_str().expect("exec task_id").to_string();
                // Delay the reply so the first run is still active.
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                send_agent_frame(
                    &mut sink,
                    json!({
                        "type": "task_result",
                        "msg_id": "exec-reply-overlap",
                        "task_id": correlation,
                        "output": "done\n",
                        "exit_code": 0
                    }),
                )
                .await;
                return;
            }
        }
    });

    // First trigger: accepted (200) and claims the active-runs slot.
    let first = client
        .post(format!("{}/api/tasks/{}/run", base_url, task_id))
        .send()
        .await
        .expect("first run failed");
    assert_eq!(first.status(), 200);

    // Second trigger arrives while the first run is still active -> 409.
    let second = client
        .post(format!("{}/api/tasks/{}/run", base_url, task_id))
        .send()
        .await
        .expect("second run failed");
    assert_eq!(
        second.status(),
        409,
        "an overlapping manual trigger must be rejected as Conflict"
    );

    agent_task.await.expect("agent responder task panicked");
}

// ===========================================================================
// task.rs — additional create/update validation arms
// ===========================================================================

/// POST /api/tasks with timeout=0 hits the explicit `timeout must be > 0` guard.
#[tokio::test]
async fn create_task_timeout_zero_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-t0-srv").await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "oneshot",
            "timeout": 0
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

/// POST /api/tasks with retry_interval < 1 hits the `retry_interval must be >= 1`
/// guard.
#[tokio::test]
async fn create_task_retry_interval_below_one_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-ri-srv").await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "oneshot",
            "retry_interval": 0
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

/// PUT /api/tasks/{id} for an unknown id is 404 (the update not-found arm, which
/// router_content_admin.rs does not exercise — it only covers the cron arm).
#[tokio::test]
async fn update_task_not_found_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/tasks/ghost-task", base_url))
        .json(&json!({ "name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

/// PUT /api/tasks/{id} with timeout < 1 hits the in-handler `timeout must be >= 1`
/// validation arm.
#[tokio::test]
async fn update_task_timeout_below_one_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-upd-t-srv").await;

    let created: Value = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "oneshot"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = admin
        .put(format!("{}/api/tasks/{}", base_url, task_id))
        .json(&json!({ "timeout": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

/// PUT /api/tasks/{id} with retry_count out of 0..=10 hits its validation arm.
#[tokio::test]
async fn update_task_retry_count_out_of_range_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-upd-rc-srv").await;

    let created: Value = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "oneshot"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = admin
        .put(format!("{}/api/tasks/{}", base_url, task_id))
        .json(&json!({ "retry_count": 50 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

/// PUT /api/tasks/{id} with retry_interval < 1 hits its validation arm.
#[tokio::test]
async fn update_task_retry_interval_below_one_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-upd-ri-srv").await;

    let created: Value = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "oneshot"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = admin
        .put(format!("{}/api/tasks/{}", base_url, task_id))
        .json(&json!({ "retry_interval": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

/// GET /api/tasks?type=scheduled exercises the optional task_type list filter,
/// returning only scheduled tasks (the no-agent oneshot is excluded).
#[tokio::test]
async fn list_tasks_filters_by_type() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-filter-srv").await;

    // One scheduled, one oneshot.
    let scheduled: Value = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo sched",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "name": "Sched",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let scheduled_id = scheduled["data"]["id"].as_str().unwrap().to_string();

    admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo once",
            "server_ids": [server_id],
            "task_type": "oneshot"
        }))
        .send()
        .await
        .unwrap();

    let list = admin
        .get(format!("{}/api/tasks?type=scheduled", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let body: Value = list.json().await.unwrap();
    let items = body["data"].as_array().unwrap();
    assert!(
        items.iter().any(|t| t["id"] == scheduled_id.as_str()),
        "scheduled task must be present in the filtered list"
    );
    assert!(
        items.iter().all(|t| t["task_type"] == "scheduled"),
        "type=scheduled filter must exclude oneshot tasks, got: {items:?}"
    );
}

// ===========================================================================
// security.rs — delete / get / stats over REAL seeded rows
// ===========================================================================

/// Seed a security event through the agent (the supported ingest path), then
/// fetch it by id (200) and delete it (200) — the delete-event happy path and
/// get-event happy path that router_security_rate.rs only hits as 404 arms.
#[tokio::test]
async fn get_and_delete_security_event_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Default caps include CAP_SECURITY_EVENTS, so the event is recorded.
    let (server_id, mut sink, _reader) = bring_up_agent(&client, &base_url, CAP_DEFAULT).await;

    let source_ip = "203.0.113.77";
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
                "failed_count": 9,
                "distinct_users": 2,
                "sample_users": ["root", "admin"],
                "invalid_user_count": 3,
                "window_seconds": 60,
                "threshold": 5
            }
        }),
    )
    .await;

    // Poll for the persisted event and capture its id.
    let mut event_id: Option<String> = None;
    for _ in 0..40 {
        let resp = client
            .get(format!(
                "{}/api/security/events?server_id={}",
                base_url, server_id
            ))
            .send()
            .await
            .expect("GET events failed");
        let body: Value = resp.json().await.expect("parse events");
        if let Some(item) = body["data"]["items"]
            .as_array()
            .and_then(|items| items.iter().find(|e| e["source_ip"] == source_ip))
        {
            event_id = Some(item["id"].as_str().expect("event id").to_string());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let event_id = event_id.expect("security event should be persisted");

    // GET the single event by id -> 200 happy path.
    let got = client
        .get(format!("{}/api/security/events/{}", base_url, event_id))
        .send()
        .await
        .expect("GET event by id failed");
    assert_eq!(got.status(), 200);
    let got_body: Value = got.json().await.expect("parse event");
    assert_eq!(got_body["data"]["source_ip"], source_ip);
    assert_eq!(got_body["data"]["event_type"], "ssh_brute_force");

    // DELETE the event -> 200 happy path (rows_affected == 1).
    let del = client
        .delete(format!("{}/api/security/events/{}", base_url, event_id))
        .send()
        .await
        .expect("DELETE event failed");
    assert_eq!(del.status(), 200);
    let del_body: Value = del.json().await.expect("parse delete");
    assert_eq!(del_body["data"], true);

    // It is gone afterwards -> 404.
    let gone = client
        .get(format!("{}/api/security/events/{}", base_url, event_id))
        .send()
        .await
        .expect("GET deleted event failed");
    assert_eq!(gone.status(), 404);

    let _ = sink.close().await;
}

/// GET /api/security/stats?group_by=source_ip over a real seeded row returns a
/// non-empty bucket keyed on the source IP — the stats-with-data path (the
/// existing file only checks empty stats and invalid group_by).
#[tokio::test]
async fn security_stats_group_by_source_ip_over_real_rows() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, _reader) = bring_up_agent(&client, &base_url, CAP_DEFAULT).await;

    let source_ip = "198.51.100.5";
    send_agent_frame(
        &mut sink,
        json!({
            "type": "security_event",
            "event_type": "port_scan",
            "severity": "medium",
            "source_ip": source_ip,
            "source_port": 4444,
            "username": null,
            "started_at": 1_700_000_000_i64,
            "ended_at": 1_700_000_030_i64,
            "first_seen": true,
            "detector_source": "conntrack",
            "evidence": {
                "kind": "port_scan",
                "distinct_ports": 30,
                "sample_ports": [22, 80, 443],
                "total_attempts": 30,
                "window_seconds": 60,
                "threshold": 10,
                "blocked_count": 0
            }
        }),
    )
    .await;

    // Wait until the event is queryable, then assert the stats bucket reflects it.
    let mut found = false;
    for _ in 0..40 {
        let stats = client
            .get(format!(
                "{}/api/security/stats?group_by=source_ip&server_id={}",
                base_url, server_id
            ))
            .send()
            .await
            .expect("GET stats failed");
        assert_eq!(stats.status(), 200);
        let body: Value = stats.json().await.expect("parse stats");
        let buckets = body["data"].as_array().expect("stats array");
        if buckets
            .iter()
            .any(|b| b["key"] == source_ip && b["count"].as_i64() == Some(1))
        {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        found,
        "group_by=source_ip stats must include a bucket for the seeded source ip"
    );

    let _ = sink.close().await;
}

/// GET /api/security/events with since/until time-window filters that bracket the
/// seeded event's created_at returns it; a window in the far past returns nothing.
/// Exercises the created_at gte/lte filter arms over real data.
#[tokio::test]
async fn security_list_events_time_window_filters_over_real_rows() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, mut sink, _reader) = bring_up_agent(&client, &base_url, CAP_DEFAULT).await;

    let source_ip = "203.0.113.200";
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
                "failed_count": 20,
                "distinct_users": 4,
                "sample_users": ["root", "admin", "test"],
                "invalid_user_count": 6,
                "window_seconds": 60,
                "threshold": 5
            }
        }),
    )
    .await;

    // Wait for persistence (created_at is server "now").
    for _ in 0..40 {
        let resp = client
            .get(format!(
                "{}/api/security/events?server_id={}",
                base_url, server_id
            ))
            .send()
            .await
            .unwrap();
        let body: Value = resp.json().await.unwrap();
        if body["data"]["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|e| e["source_ip"] == source_ip))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // A wide window around "now" includes the event (since/until both applied).
    let wide = client
        .get(format!(
            "{}/api/security/events?server_id={}&since=2000-01-01T00:00:00Z&until=2100-01-01T00:00:00Z",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(wide.status(), 200);
    let wide_body: Value = wide.json().await.unwrap();
    assert!(
        wide_body["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["source_ip"] == source_ip),
        "a wide since/until window must include the seeded event"
    );

    // A window entirely in the past (until before the event) excludes it.
    let past = client
        .get(format!(
            "{}/api/security/events?server_id={}&until=2001-01-01T00:00:00Z",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(past.status(), 200);
    let past_body: Value = past.json().await.unwrap();
    assert_eq!(
        past_body["data"]["items"].as_array().unwrap().len(),
        0,
        "an until cutoff before the event must exclude it"
    );

    let _ = sink.close().await;
}

// ===========================================================================
// auth.rs — logout (no cookie), 2FA disable success, change-password validation
// ===========================================================================

/// POST /api/auth/logout with no session cookie is a benign no-op that still
/// returns 200 and clears the cookie. (router_auth_user.rs only logs out a live
/// session.) Uses an API key for auth so the protected route is reachable without
/// a session_token cookie.
#[tokio::test]
async fn logout_without_session_cookie_is_noop_200() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Mint an API key so we can authenticate the protected /logout route without
    // a session cookie (admin-only key creation).
    let created: Value = admin
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "logout-probe-key" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let key = created["data"]["key"].as_str().unwrap().to_string();

    // Fresh cookieless client authenticating via X-API-Key: the logout handler's
    // cookie-scan loop finds no session_token and short-circuits to the 200 path.
    let resp = http_client()
        .post(format!("{}/api/auth/logout", base_url))
        .header("X-API-Key", &key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"], "ok");
}

/// 2FA disable SUCCESS path: the handler verifies the account *password* (not a
/// TOTP code) and then calls disable_2fa, which clears the secret idempotently.
/// Supplying the correct password returns 200 and status stays disabled — this
/// exercises the verify_password-OK + disable_2fa branch that router_auth_user.rs
/// only covers as the wrong-password 400 arm.
///
/// NOTE: the enable→disable *round trip* would need a live TOTP code, but the
/// `totp-rs` crate the server uses is a normal `[dependencies]` entry, not a
/// `[dev-dependencies]` one, so it cannot be linked from this integration-test
/// crate without modifying Cargo.toml (out of scope). The success arm of the
/// disable handler is reachable without enabling first because disable_2fa is
/// idempotent, so we cover that here instead.
#[tokio::test]
async fn totp_disable_with_correct_password_succeeds() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Disable with the correct account password -> 200 (the success arm:
    // verify_password passes, then disable_2fa runs).
    let disable = client
        .post(format!("{}/api/auth/2fa/disable", base_url))
        .json(&json!({ "password": "testpass" }))
        .send()
        .await
        .unwrap();
    assert_eq!(disable.status(), 200, "correct password should reach the disable success arm");
    let body: Value = disable.json().await.unwrap();
    assert_eq!(body["data"], "ok");

    // Status remains disabled (no secret was ever set).
    let status: Value = client
        .get(format!("{}/api/auth/2fa/status", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["data"]["enabled"], false);
}

/// PUT /api/auth/password with an empty new_password hits the handler's explicit
/// `new_password.is_empty()` guard (422), a distinct arm from the
/// short/weak-password service rejection covered in router_auth_user.rs.
#[tokio::test]
async fn change_password_empty_new_password_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .put(format!("{}/api/auth/password", base_url))
        .json(&json!({ "old_password": "testpass", "new_password": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

// ===========================================================================
// widget_module.rs — enforce_url_safety scheme + parse rejection arms
// ===========================================================================

/// POST /api/widget-modules?url=ftp://... hits the `url must be http(s)` arm of
/// enforce_url_safety (a non-loopback rejection branch the SSRF tests in
/// widget_module_integration.rs do not cover).
#[tokio::test]
async fn install_widget_non_http_scheme_url_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!(
            "{}/api/widget-modules?url=ftp://example.com/widget.js",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "non-http(s) scheme must be rejected");
}

/// POST /api/widget-modules?url=<garbage> hits the `bad url` parse-failure arm of
/// enforce_url_safety.
#[tokio::test]
async fn install_widget_malformed_url_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!(
            "{}/api/widget-modules?url=not%20a%20valid%20url",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "an unparseable url must be rejected");
}

/// POST /api/widget-modules?url=... from a member is blocked by require_admin
/// (403) before the URL is ever fetched — confirms the install route stays
/// admin-gated for the URL source too.
#[tokio::test]
async fn install_widget_url_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "wm-url-member", "member").await;

    let resp = member
        .post(format!(
            "{}/api/widget-modules?url=https://example.com/w.js",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}
