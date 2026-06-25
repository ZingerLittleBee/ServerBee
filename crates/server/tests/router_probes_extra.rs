//! Router-level integration tests covering branches the existing router_*
//! suites leave uncovered: network-probe write paths that dispatch a
//! `NetworkProbeSync` to a live (mock) agent, the traceroute trigger →
//! result → snapshot round-trip via a mock agent, additional alert-rule
//! update/delete/validation arms, firewall block add/remove that forward a
//! `blocklist_add` / `blocklist_remove` command to a covered agent, and the
//! geoip/asn status envelopes.
//!
//! Status-code expectations follow `crate::error::AppError`'s `IntoResponse`
//! mapping: BadRequest → 400, Validation → 422, Unauthorized → 401,
//! Forbidden → 403, NotFound → 404, Conflict → 409.
mod common;

use common::{
    AgentReader, AgentSink, connect_agent, create_server, http_client, login_admin,
    login_as_new_user, recv_agent_text, register_agent, send_system_info, start_test_server,
};
use futures_util::SinkExt;
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::tungstenite;

// ───────────────────────────── helpers ─────────────────────────────

/// Bring a mock agent fully online: connect, drain the `welcome` frame, and
/// complete the SystemInfo handshake with the supplied capability bitmask
/// (`None` → server default, which includes CAP_FIREWALL_BLOCK).
async fn online_agent(base_url: &str, token: &str, caps: Option<u32>) -> (AgentSink, AgentReader) {
    let (mut sink, mut reader) = connect_agent(base_url, token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome", "first agent frame is welcome");
    send_system_info(&mut sink, &mut reader, "sysinfo-probes-extra", caps).await;
    (sink, reader)
}

/// Wait until the agent receives a frame whose `type` is `expected`, ignoring
/// the first-connect noise (ping/network-probe sync + blocklist reset/sync).
/// Returns the matched frame, or panics on timeout / an unexpected control frame.
async fn recv_until(reader: &mut AgentReader, expected: &str) -> Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let frame = tokio::time::timeout(remaining, recv_agent_text(reader))
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for agent frame `{expected}`"));
        let ty = frame["type"].as_str().unwrap_or("");
        if ty == expected {
            return frame;
        }
        match ty {
            // First-connect noise a default agent always receives; unrelated to
            // the request under test, so it is ignored.
            "ping_tasks_sync" | "network_probe_sync" | "blocklist_reset" | "blocklist_sync"
            | "blocklist_add" | "blocklist_remove" | "ip_quality_sync" => {}
            other => panic!("unexpected agent command while awaiting `{expected}`: {other}"),
        }
    }
}

/// Drain any first-connect noise the agent has already queued, so a later
/// assertion about a freshly-pushed command is not confused by it.
async fn drain_initial_noise(reader: &mut AgentReader) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return;
        }
        match tokio::time::timeout(remaining, recv_agent_text(reader)).await {
            Ok(_frame) => {}
            Err(_) => return,
        }
    }
}

// ───────────────────── network-probes: agent dispatch ─────────────────────

// Updating the global setting pushes a `NetworkProbeSync` carrying the new
// interval/packet_count to every online agent. Asserts the 200 envelope AND
// the forwarded sync command (happy dispatch path, not covered elsewhere).
#[tokio::test]
async fn network_probe_setting_update_pushes_sync_to_online_agent() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let (_server_id, token) = register_agent(&admin, &base_url).await;
    let (_sink, mut reader) = online_agent(&base_url, &token, None).await;
    drain_initial_noise(&mut reader).await;

    let resp = admin
        .put(format!("{}/api/network-probes/setting", base_url))
        .json(&json!({ "interval": 90, "packet_count": 7, "default_target_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "setting update succeeds");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["interval"].as_u64(), Some(90));

    // The online agent must receive the refreshed config.
    let sync = recv_until(&mut reader, "network_probe_sync").await;
    assert_eq!(sync["interval"].as_u64(), Some(90));
    assert_eq!(sync["packet_count"].as_u64(), Some(7));
}

// Deleting a custom target that is assigned to an online server notifies that
// agent with a fresh `NetworkProbeSync` (now without the deleted target).
// Exercises the delete_target → affected_server_ids → push branch.
#[tokio::test]
async fn network_probe_delete_assigned_target_pushes_sync() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let (server_id, token) = register_agent(&admin, &base_url).await;
    let (_sink, mut reader) = online_agent(&base_url, &token, None).await;

    // Create a custom target, then assign it to this server.
    let target_id = {
        let body: Value = admin
            .post(format!("{}/api/network-probes/targets", base_url))
            .json(&json!({
                "name": "del-sync", "provider": "P", "location": "L",
                "target": "9.9.9.9", "probe_type": "icmp"
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    let assign = admin
        .put(format!(
            "{}/api/servers/{}/network-probes/targets",
            base_url, server_id
        ))
        .json(&json!({ "target_ids": [target_id] }))
        .send()
        .await
        .unwrap();
    assert_eq!(assign.status(), 200, "assigning targets succeeds");

    drain_initial_noise(&mut reader).await;

    // Deleting the assigned target triggers a per-server NetworkProbeSync.
    let del = admin
        .delete(format!(
            "{}/api/network-probes/targets/{}",
            base_url, target_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200, "delete succeeds");

    let sync = recv_until(&mut reader, "network_probe_sync").await;
    // The deleted target must no longer be present in the synced target list.
    let targets = sync["targets"].as_array().cloned().unwrap_or_default();
    assert!(
        !targets
            .iter()
            .any(|t| t["target_id"].as_str() == Some(target_id.as_str())),
        "deleted target must be absent from the refreshed sync"
    );
}

// A preset target cannot be modified: update_target hits the Forbidden arm → 403.
#[tokio::test]
async fn network_probe_update_preset_target_forbidden_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Discover a preset target id from the merged list (source starts "preset:").
    let list: Value = admin
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let preset_id = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| {
            t["source"]
                .as_str()
                .map(|s| s.starts_with("preset:"))
                .unwrap_or(false)
        })
        .and_then(|t| t["id"].as_str())
        .expect("at least one preset target should exist")
        .to_string();

    let resp = admin
        .put(format!(
            "{}/api/network-probes/targets/{}",
            base_url, preset_id
        ))
        .json(&json!({ "name": "hijacked" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "preset targets are immutable → Forbidden");
}

// A preset target cannot be deleted either: delete_target → Forbidden → 403.
#[tokio::test]
async fn network_probe_delete_preset_target_forbidden_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let list: Value = admin
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let preset_id = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| {
            t["source"]
                .as_str()
                .map(|s| s.starts_with("preset:"))
                .unwrap_or(false)
        })
        .and_then(|t| t["id"].as_str())
        .expect("a preset target id")
        .to_string();

    let resp = admin
        .delete(format!(
            "{}/api/network-probes/targets/{}",
            base_url, preset_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// Creating a target pointed at a literal-unsafe (cloud metadata) address is
// rejected by the SSRF guard → Validation/422.
#[tokio::test]
async fn network_probe_create_target_ssrf_literal_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/network-probes/targets", base_url))
        .json(&json!({
            "name": "metadata", "provider": "P", "location": "L",
            "target": "169.254.169.254", "probe_type": "tcp"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "cloud-metadata literal → Validation/422");
}

// update_setting with an unknown id in default_target_ids fails validation → 422.
#[tokio::test]
async fn network_probe_setting_invalid_default_target_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/network-probes/setting", base_url))
        .json(&json!({
            "interval": 60, "packet_count": 10,
            "default_target_ids": ["not-a-real-target-id"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "unknown default target id → Validation/422");
}

// packet_count out of the 5..=20 range is a BadRequest → 400.
#[tokio::test]
async fn network_probe_setting_packet_count_out_of_range_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/network-probes/setting", base_url))
        .json(&json!({ "interval": 60, "packet_count": 99, "default_target_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "packet_count above 20 → BadRequest/400");
}

// ───────────────────── traceroute: trigger → result → snapshot ─────────────────────

// Full round-trip: with a connected agent, triggering a traceroute returns 200
// and the agent receives a `traceroute` command. The mock agent replies with a
// completed `traceroute_result`; a subsequent GET snapshot then resolves from
// the in-memory cache (and, since completed, also persists a DB row).
#[tokio::test]
async fn traceroute_trigger_result_snapshot_roundtrip() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let (server_id, token) = register_agent(&admin, &base_url).await;
    let (mut sink, mut reader) = online_agent(&base_url, &token, None).await;
    drain_initial_noise(&mut reader).await;

    // Trigger — dispatches a `traceroute` command to the online agent.
    let trigger = admin
        .post(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .json(&json!({ "target": "1.1.1.1", "protocol": "icmp" }))
        .send()
        .await
        .unwrap();
    assert_eq!(trigger.status(), 200, "valid target + online agent → 200");
    let tbody: Value = trigger.json().await.unwrap();
    let request_id = tbody["data"]["request_id"]
        .as_str()
        .expect("request_id returned")
        .to_string();

    // Agent receives the traceroute command echoing our request_id.
    let cmd = recv_until(&mut reader, "traceroute").await;
    assert_eq!(cmd["request_id"].as_str(), Some(request_id.as_str()));
    assert_eq!(cmd["target"].as_str(), Some("1.1.1.1"));

    // Agent replies with a completed legacy TracerouteResult.
    let result = json!({
        "type": "traceroute_result",
        "request_id": request_id,
        "target": "1.1.1.1",
        "completed": true,
        "error": null,
        "hops": [
            { "hop": 1, "ip": "192.0.2.1", "hostname": null, "asn": null,
              "rtt1": 1.2, "rtt2": 1.3, "rtt3": 1.4 },
            { "hop": 2, "ip": "1.1.1.1", "hostname": "one.one.one.one", "asn": "AS13335",
              "rtt1": 9.0, "rtt2": 9.1, "rtt3": 9.2 }
        ]
    });
    sink.send(tungstenite::Message::Text(result.to_string().into()))
        .await
        .expect("send traceroute_result");

    // Poll the snapshot endpoint until the async result lands in the cache/DB.
    // The trigger seeds a *pending* entry (completed=false) that already answers
    // 200, so poll for `completed=true` rather than merely a 200 status.
    let mut snapshot: Option<Value> = None;
    for _ in 0..40 {
        let resp = admin
            .get(format!(
                "{}/api/servers/{}/traceroute/{}",
                base_url, server_id, request_id
            ))
            .send()
            .await
            .unwrap();
        if resp.status() == 200 {
            let body: Value = resp.json().await.unwrap();
            if body["data"]["completed"].as_bool() == Some(true) {
                snapshot = Some(body);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let snap = snapshot.expect("snapshot should resolve to completed after the agent result");
    assert_eq!(snap["data"]["request_id"].as_str(), Some(request_id.as_str()));
    assert_eq!(snap["data"]["target"].as_str(), Some("1.1.1.1"));
    assert_eq!(snap["data"]["completed"].as_bool(), Some(true));
    assert_eq!(
        snap["data"]["hops"].as_array().map(|h| h.len()),
        Some(2),
        "both hops are present in the persisted snapshot"
    );

    // The completed run is now listed in the server's traceroute history (DB).
    let list: Value = admin
        .get(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["request_id"].as_str() == Some(request_id.as_str())),
        "completed traceroute appears in history"
    );
}

// Deleting a single traceroute record that does not exist returns 404 (the
// delete_record service arm reports NotFound on a zero-row delete).
#[tokio::test]
async fn traceroute_delete_missing_record_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-del-missing").await;

    let resp = admin
        .delete(format!(
            "{}/api/servers/{}/traceroute/no-such-request",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "deleting an unknown record → 404");
}

// ─────────────────────── alert-rules: extra branches ───────────────────────

// Updating an unknown alert-rule id → 404 (update calls get() first).
#[tokio::test]
async fn alert_rule_update_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/alert-rules/ghost-rule", base_url))
        .json(&json!({ "name": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// Deleting an unknown alert-rule id → 404 (delete reports zero rows affected).
#[tokio::test]
async fn alert_rule_delete_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .delete(format!("{}/api/alert-rules/ghost-rule", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// Updating a rule with an invalid cover_type hits validate_cover_type → 422.
#[tokio::test]
async fn alert_rule_update_invalid_cover_type_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let created: Value = admin
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({ "name": "cov", "rules": [{ "rule_type": "cpu", "min": 1.0 }] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = admin
        .put(format!("{}/api/alert-rules/{}", base_url, id))
        .json(&json!({ "cover_type": "sideways" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "invalid cover_type on update → Validation/422");
}

// Updating a rule's rules[] to mix a security item with a metric item is
// rejected by validate_alert_rule_items → 400 (BadRequest).
#[tokio::test]
async fn alert_rule_update_mixed_security_items_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let created: Value = admin
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({ "name": "mix-upd", "rules": [{ "rule_type": "cpu", "min": 1.0 }] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = admin
        .put(format!("{}/api/alert-rules/{}", base_url, id))
        .json(&json!({
            "rules": [
                { "rule_type": "ssh_brute_force_detected" },
                { "rule_type": "memory", "min": 1.0 }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "mixing security + metric on update → BadRequest/400");
}

// A `block_source_ip` action attached to a non-source-IP rule type is rejected
// by validate_actions on create → 422.
#[tokio::test]
async fn alert_rule_create_block_action_on_wrong_type_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "bad-action",
            // cpu is not a source_ip-bearing rule, so block_source_ip is invalid.
            "rules": [{ "rule_type": "cpu", "min": 90.0 }],
            "actions": [{ "type": "block_source_ip", "cover_type": "all" }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        422,
        "block_source_ip on a non-source-ip rule → Validation/422"
    );
}

// A valid `block_source_ip` action on an ssh_brute_force_detected rule is
// accepted and persisted (the happy validate_actions branch).
#[tokio::test]
async fn alert_rule_create_block_action_on_security_rule_ok() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "ssh-autoblock",
            "rules": [{ "rule_type": "ssh_brute_force_detected" }],
            "actions": [{ "type": "block_source_ip", "cover_type": "all" }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "valid security rule + block action → 200");
    let body: Value = resp.json().await.unwrap();
    let id = body["data"]["id"].as_str().expect("rule id").to_string();

    // The persisted rule round-trips its actions_json (read back via GET).
    let got: Value = admin
        .get(format!("{}/api/alert-rules/{}", base_url, id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        got["data"]["actions_json"]
            .as_str()
            .unwrap_or("")
            .contains("block_source_ip"),
        "the block_source_ip action is persisted"
    );
}

// ─────────────────────── firewall: agent dispatch + CRUD ───────────────────────

// Creating a manual block returns 200, persists the canonicalized CIDR, and
// pushes a `blocklist_add` to the connected, covered, capable agent.
#[tokio::test]
async fn firewall_create_block_pushes_add_to_agent() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let (_server_id, token) = register_agent(&admin, &base_url).await;
    let (_sink, mut reader) = online_agent(&base_url, &token, None).await;
    drain_initial_noise(&mut reader).await;

    let resp = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "203.0.113.10", "comment": "manual test" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "manual block create succeeds");
    let body: Value = resp.json().await.unwrap();
    // Bare IP is canonicalized to a /32.
    assert_eq!(body["data"]["target"].as_str(), Some("203.0.113.10/32"));
    assert_eq!(body["data"]["origin"].as_str(), Some("manual"));
    let block_id = body["data"]["id"].as_str().unwrap().to_string();

    // The covered agent receives the add command for this entry.
    let add = recv_until(&mut reader, "blocklist_add").await;
    assert_eq!(add["entry"]["id"].as_str(), Some(block_id.as_str()));
    assert_eq!(add["entry"]["target"].as_str(), Some("203.0.113.10/32"));
}

// Deleting a manual block returns 200 and pushes a `blocklist_remove` carrying
// the block id to the covered agent.
#[tokio::test]
async fn firewall_delete_block_pushes_remove_to_agent() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let (_server_id, token) = register_agent(&admin, &base_url).await;
    let (_sink, mut reader) = online_agent(&base_url, &token, None).await;

    let block_id = {
        let body: Value = admin
            .post(format!("{}/api/firewall/blocks", base_url))
            .json(&json!({ "target": "203.0.113.20" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    // Drain the add we just triggered (plus first-connect noise).
    drain_initial_noise(&mut reader).await;

    let del = admin
        .delete(format!("{}/api/firewall/blocks/{}", base_url, block_id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200, "delete block succeeds");

    let remove = recv_until(&mut reader, "blocklist_remove").await;
    assert_eq!(remove["id"].as_str(), Some(block_id.as_str()));
}

// Blocking a protected (hard-coded guardrail) target is refused → 409 Conflict.
#[tokio::test]
async fn firewall_create_block_protected_target_409() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        // Loopback is a tier-1 hard-coded guardrail.
        .json(&json!({ "target": "127.0.0.1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409, "guardrail-protected target → Conflict/409");
}

// Blocking the same canonical target twice → unique-violation → 409 Conflict.
#[tokio::test]
async fn firewall_create_duplicate_block_409() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let first = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "198.51.100.5" }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);

    let dup = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "198.51.100.5" }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup.status(), 409, "duplicate target → Conflict/409");
}

// An unparseable target fails canonicalize_target → BadRequest/400.
#[tokio::test]
async fn firewall_create_block_invalid_target_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "not-an-ip" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "garbage target → BadRequest/400");
}

// GET a single block by id reflects the created row; an unknown id → 404.
#[tokio::test]
async fn firewall_get_block_and_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let created: Value = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "198.51.100.30", "comment": "for-get" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let got = admin
        .get(format!("{}/api/firewall/blocks/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(got.status(), 200);
    let got_body: Value = got.json().await.unwrap();
    assert_eq!(got_body["data"]["target"].as_str(), Some("198.51.100.30/32"));
    assert_eq!(got_body["data"]["comment"].as_str(), Some("for-get"));

    let missing = admin
        .get(format!("{}/api/firewall/blocks/ghost-block", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

// Deleting an unknown block id → 404 (the find_by_id guard).
#[tokio::test]
async fn firewall_delete_block_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .delete(format!("{}/api/firewall/blocks/ghost-block", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// The stats endpoint aggregates total / auto / manual / v4 / v6 counts; after
// two manual IPv4 blocks they reflect total=2, manual=2, auto=0, v4=2, v6=0.
#[tokio::test]
async fn firewall_stats_reflects_created_blocks() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    for ip in ["198.51.100.40", "198.51.100.41"] {
        let resp = admin
            .post(format!("{}/api/firewall/blocks", base_url))
            .json(&json!({ "target": ip }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    let stats: Value = admin
        .get(format!("{}/api/firewall/stats", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stats["data"]["total"].as_i64(), Some(2));
    assert_eq!(stats["data"]["manual"].as_i64(), Some(2));
    assert_eq!(stats["data"]["auto"].as_i64(), Some(0));
    assert_eq!(stats["data"]["v4"].as_i64(), Some(2));
    assert_eq!(stats["data"]["v6"].as_i64(), Some(0));
}

// list_blocks honors the `origin` and `target_q` filters and the cursor guard.
#[tokio::test]
async fn firewall_list_blocks_filters_and_bad_cursor_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "198.51.100.60" }))
        .send()
        .await
        .unwrap();

    // origin=manual + target_q substring match returns the row.
    let filtered: Value = admin
        .get(format!(
            "{}/api/firewall/blocks?origin=manual&target_q=198.51.100.60",
            base_url
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        filtered["data"]["items"].as_array().map(|i| i.len()),
        Some(1),
        "manual+substring filter matches the created block"
    );

    // origin=auto excludes the manual row.
    let auto_only: Value = admin
        .get(format!("{}/api/firewall/blocks?origin=auto", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(auto_only["data"]["items"].as_array().map(|i| i.len()), Some(0));

    // A malformed cursor is rejected → BadRequest/400.
    let bad_cursor = admin
        .get(format!(
            "{}/api/firewall/blocks?cursor=not-a-timestamp",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_cursor.status(), 400, "invalid cursor → BadRequest/400");
}

// AuthZ: a member can read the blocklist but cannot create one (admin-only write).
#[tokio::test]
async fn firewall_member_read_ok_write_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "fw_member", "member").await;

    let read = member
        .get(format!("{}/api/firewall/blocks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200, "members may read the blocklist");

    let write = member
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "203.0.113.99" }))
        .send()
        .await
        .unwrap();
    assert_eq!(write.status(), 403, "members may not create blocks");
}

// Unauthenticated access to the blocklist read route → 401.
#[tokio::test]
async fn firewall_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/firewall/stats", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ─────────────────────── geoip / asn: status fields ───────────────────────

// With no MMDB configured the geoip status reports installed=false and omits the
// optional source/file_size/updated_at fields entirely (skip_serializing_if).
#[tokio::test]
async fn geoip_status_not_installed_omits_optional_fields() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let body: Value = admin
        .get(format!("{}/api/geoip/status", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["installed"].as_bool(), Some(false));
    // The None-arm omits these via skip_serializing_if.
    assert!(body["data"].get("source").is_none(), "source omitted when absent");
    assert!(body["data"].get("file_size").is_none());
    assert!(body["data"].get("updated_at").is_none());
}

// The asn status mirrors the geoip not-installed shape.
#[tokio::test]
async fn asn_status_not_installed_omits_optional_fields() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let body: Value = admin
        .get(format!("{}/api/asn/status", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["installed"].as_bool(), Some(false));
    assert!(body["data"].get("source").is_none());
    assert!(body["data"].get("file_size").is_none());
    assert!(body["data"].get("updated_at").is_none());
}
