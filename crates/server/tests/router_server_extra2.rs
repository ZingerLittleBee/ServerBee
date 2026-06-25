//! Third router-level integration suite for `crates/server/src/router/api/server.rs`.
//!
//! `router_server_crud.rs` and `router_server_extra.rs` already cover the
//! create/list/get/update/delete/batch-delete happy paths, their authZ
//! (401/403) and validation (400/422) arms, the recover / regenerate-code
//! success + error arms, the group assign/clear/move + weight/hidden/geo edge
//! updates, and the ONLINE-server DTO mapping. This file targets the branches
//! those two leave uncovered:
//!
//!   - POST /servers/{id}/upgrade — the entire handler, which neither prior
//!     file touches over HTTP:
//!       * happy path: a mock agent advertising CAP_UPGRADE receives a
//!         `ServerMessage::Upgrade` frame (asserted by draining the WS).
//!       * capability-denied: agent connected WITHOUT the CAP_UPGRADE bit ->
//!         403 (`capability_denied_reason` fires on the agent-reported caps).
//!       * not-connected: capability gate passes via the persisted mirror
//!         (CAP_DEFAULT includes upgrade), but `get_sender` is None -> 404.
//!       * invalid version string -> 400.
//!       * conflict: a second upgrade while one is still Running -> 409.
//!       * not-found server id -> 404, member -> 403, unauth -> 401.
//!   - DELETE /servers/cleanup — `cleanup_orphaned_servers`, also untouched by
//!     the prior files:
//!       * deletes an offline orphan (name == "New Server" && os IS NULL).
//!       * preserves a renamed server and reports deleted_count accordingly.
//!       * no-orphans -> deleted_count = 0 (early return branch).
//!       * member -> 403, unauth -> 401.
//!
//! NOTE / skipped: the `send`-failure arm of `trigger_upgrade` (the agent WS
//! send returning Err, which marks the job failed and returns 500) is not
//! reachable deterministically — the channel only errors after the receiver is
//! dropped, which races the server's own connection teardown. The
//! online-orphan-skipped branch of cleanup is already unit-tested in
//! `server.rs::cleanup_tests::test_collect_orphan_server_ids_skips_online_servers`,
//! so it is not re-driven here over HTTP.

mod common;

use common::{
    connect_agent, create_server, http_client, login_admin, login_as_new_user, recv_agent_text,
    register_agent, send_system_info, start_test_server, AgentSink, AgentReader,
};
use futures_util::SinkExt;
use serde_json::{json, Value};
use serverbee_common::constants::{CAP_DEFAULT, CAP_UPGRADE};
use tokio_tungstenite::tungstenite;

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

/// Bring a mock agent online for `register_agent`'s server: connect the WS,
/// consume the welcome, finish the SystemInfo handshake reporting `caps`, then
/// drain the first-connect sync noise until the socket is briefly quiet. Returns
/// the live sink + reader so the caller can keep reading (e.g. to capture a
/// forwarded `upgrade` frame). Keep the returned sink alive to stay online.
async fn online_agent(base_url: &str, token: &str, caps: u32) -> (AgentSink, AgentReader) {
    let (mut sink, mut reader) = connect_agent(base_url, token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_system_info(&mut sink, &mut reader, "extra2-sysinfo", Some(caps)).await;

    // Drain the first-connect ping_tasks_sync / network_probe_sync / blocklist
    // pushes with a short per-frame timeout so we stop once the server is quiet.
    loop {
        let next =
            tokio::time::timeout(std::time::Duration::from_millis(300), recv_agent_text(&mut reader))
                .await;
        match next {
            Ok(msg) => {
                let ty = msg["type"].as_str().unwrap_or_default();
                // Ignore the expected first-connect control-plane sync frames.
                if matches!(
                    ty,
                    "ping_tasks_sync"
                        | "network_probe_sync"
                        | "blocklist_reset"
                        | "blocklist_sync"
                        | "blocklist_add"
                        | "blocklist_remove"
                ) {
                    continue;
                }
                // Anything else is unexpected this early; keep draining anyway.
            }
            Err(_) => break, // quiet — handshake noise drained
        }
    }
    (sink, reader)
}

// ---------------------------------------------------------------------------
// POST /api/servers/{id}/upgrade — dispatch to a connected agent
// ---------------------------------------------------------------------------

// Happy path: an agent advertising CAP_UPGRADE (in CAP_DEFAULT) receives the
// forwarded `ServerMessage::Upgrade` frame and the HTTP call returns 200.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upgrade_dispatches_to_agent_with_cap_upgrade() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    let (server_id, token) = register_agent(&admin, &base_url).await;

    // CAP_DEFAULT already includes CAP_UPGRADE, so the agent is allowed.
    assert_eq!(CAP_DEFAULT & CAP_UPGRADE, CAP_UPGRADE);
    let (mut sink, mut reader) = online_agent(&base_url, &token, CAP_DEFAULT).await;

    let resp = admin
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "v1.2.3" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "upgrade dispatch should succeed");

    // The agent must receive the forwarded Upgrade frame with the normalized
    // version (the leading 'v' stripped) and a job_id.
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            if msg["type"] == "upgrade" {
                break msg;
            }
        }
    })
    .await
    .expect("agent should receive an upgrade frame");
    assert_eq!(
        frame["version"].as_str(),
        Some("1.2.3"),
        "the 'v' prefix is normalized away before dispatch"
    );
    let job_id = frame["job_id"].as_str().expect("upgrade frame carries a tracker job id");
    assert!(!job_id.is_empty(), "job_id must be non-empty");

    // Echo an UpgradeProgress back so the server-side tracker advances; this
    // also exercises the AgentMessage::UpgradeProgress ingest path.
    sink.send(tungstenite::Message::Text(
        json!({
            "type": "upgrade_progress",
            "msg_id": "extra2-upgrade-progress",
            "job_id": job_id,
            "target_version": "1.2.3",
            "stage": "downloading"
        })
        .to_string()
        .into(),
    ))
    .await
    .expect("send UpgradeProgress");
}

// Capability-denied: an agent connected WITHOUT the CAP_UPGRADE bit causes the
// upgrade request to be rejected with 403 (the gate reads the agent-reported
// capabilities, which now lack upgrade).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upgrade_capability_denied_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    let (server_id, token) = register_agent(&admin, &base_url).await;

    // Report CAP_DEFAULT minus the upgrade bit so the gate denies the request.
    let caps_without_upgrade = CAP_DEFAULT & !CAP_UPGRADE;
    let (_sink, _reader) = online_agent(&base_url, &token, caps_without_upgrade).await;

    let resp = admin
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        403,
        "an agent without CAP_UPGRADE must reject the upgrade"
    );
}

// Not connected: capability gate passes via the persisted CAP_DEFAULT mirror
// (which includes upgrade), but no agent WS is connected, so `get_sender`
// returns None and the handler maps that to 404.
#[tokio::test]
async fn upgrade_no_agent_connected_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    // A pending server (no agent ever connected). Its mirror caps default to
    // CAP_DEFAULT, so the capability check is satisfied; the missing sender is
    // what fails.
    let server_id = create_server(&admin, &base_url, "upgrade-offline-host").await;

    let resp = admin
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        404,
        "upgrade with no connected agent is a 404 (agent not connected)"
    );
}

// Invalid SemVer version string -> 400. The capability gate passes on the
// pending server's mirror caps, then the version parse fails before any send.
#[tokio::test]
async fn upgrade_invalid_version_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "upgrade-badver-host").await;

    let resp = admin
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "not-a-version" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "a non-SemVer version is a BadRequest");
}

// Conflict: starting a second upgrade while the first job is still Running is
// rejected with 409. The first dispatch leaves a Running job in the tracker
// (the mock agent never reports progress/result), so the second collides.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upgrade_conflict_when_job_running_409() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    let (server_id, token) = register_agent(&admin, &base_url).await;
    let (_sink, _reader) = online_agent(&base_url, &token, CAP_DEFAULT).await;

    // First upgrade starts a Running job.
    let first = admin
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200, "first upgrade should start a job");

    // Second upgrade collides with the still-running job.
    let second = admin
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "1.0.1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        second.status(),
        409,
        "a second upgrade while one is running is a conflict"
    );
}

// Upgrade on a non-existent server id -> 404 (the server lookup fails first).
#[tokio::test]
async fn upgrade_missing_server_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers/{}/upgrade", base_url, "no-such-server"))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "unknown server id yields 404");
}

// POST upgrade is admin-only: a member gets 403 (authZ middleware, before the
// handler runs).
#[tokio::test]
async fn upgrade_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "upgrade-authz-host").await;
    let member = login_as_new_user(&admin, &base_url, "upgrade_member", "member").await;

    let resp = member
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not trigger upgrades");
}

// POST upgrade requires authentication.
#[tokio::test]
async fn upgrade_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "upgrade-unauth-host").await;

    let anon = http_client();
    let resp = anon
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "1.0.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// DELETE /api/servers/cleanup — purge orphaned pending servers
// ---------------------------------------------------------------------------

// A server named exactly "New Server" with no OS (never enrolled, offline) is an
// orphan candidate; cleanup deletes it and reports the count.
#[tokio::test]
async fn cleanup_deletes_offline_orphan() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // The cleanup heuristic only targets rows literally named "New Server" with
    // os IS NULL — i.e. abandoned create-without-enroll rows.
    let orphan = create_server(&admin, &base_url, "New Server").await;
    // A renamed server is NOT an orphan candidate and must survive cleanup.
    let keep = create_server(&admin, &base_url, "kept-named-host").await;

    let resp = admin
        .delete(format!("{}/api/servers/cleanup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "cleanup should succeed");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["deleted_count"].as_u64(),
        Some(1),
        "exactly the one 'New Server' orphan is purged"
    );

    // The orphan is gone; the renamed server survives.
    let after_orphan = admin
        .get(format!("{}/api/servers/{}", base_url, orphan))
        .send()
        .await
        .unwrap();
    assert_eq!(after_orphan.status(), 404, "the orphan was deleted");
    let after_keep = admin
        .get(format!("{}/api/servers/{}", base_url, keep))
        .send()
        .await
        .unwrap();
    assert_eq!(after_keep.status(), 200, "the renamed server is preserved");
}

// Cleanup with no orphan candidates returns deleted_count = 0 (the early-return
// branch when `orphan_ids.is_empty()`).
#[tokio::test]
async fn cleanup_no_orphans_returns_zero() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    // Only a renamed server exists — not a candidate.
    create_server(&admin, &base_url, "no-orphan-host").await;

    let resp = admin
        .delete(format!("{}/api/servers/cleanup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["deleted_count"].as_u64(),
        Some(0),
        "no orphan candidates means nothing is deleted"
    );
}

// DELETE /api/servers/cleanup is admin-only: a member gets 403.
#[tokio::test]
async fn cleanup_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "cleanup_member", "member").await;

    let resp = member
        .delete(format!("{}/api/servers/cleanup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not run orphan cleanup");
}

// DELETE /api/servers/cleanup requires authentication.
#[tokio::test]
async fn cleanup_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .delete(format!("{}/api/servers/cleanup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
