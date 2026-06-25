//! Additional router-level integration tests for `crates/server/src/router/api/server.rs`
//! that cover branches `router_server_crud.rs` left untested.
//!
//! `router_server_crud.rs` already covers the basic create/list/get/update/
//! delete/batch-delete happy paths plus their authZ (401/403) and validation
//! (400/422) arms, and the recover/regenerate error arms (pending->400,
//! stale-CAS->409, missing->404, member->403, unauth->401). This file targets
//! the remaining reachable branches:
//!
//!   - POST /servers/{id}/recover happy path on an ENROLLED server
//!     (revoke_immediately = false) — mints a fresh bound enrollment.
//!   - POST /servers/{id}/recover with revoke_immediately = true — clears the
//!     server token in the same tx (DB side effect: has_token -> false) and
//!     kicks the live agent connection.
//!   - POST /servers/{id}/recover 409 — an outstanding enrollment already
//!     exists (recover never auto-supersedes).
//!   - POST /servers/{id}/regenerate-code CAS pass — expected_enrollment_id
//!     matches the current outstanding enrollment -> 200 with a rotated id.
//!   - POST /servers/{id}/regenerate-code on an ENROLLED server -> 400
//!     ("not pending; use recover instead").
//!   - POST /servers/batch-delete with a MIX of known + unknown ids — partial
//!     delete count, and the known one is actually gone.
//!   - PUT /servers/{id} group MOVE (group A -> group B), not just assign+clear.
//!   - PUT /servers/{id} weight/hidden edge values (negative weight, hidden
//!     toggled back to false).
//!   - PUT /servers/{id} country_code manual override -> geo_manual = true,
//!     then explicit null clears it -> geo_manual = false.
//!   - GET /servers + GET /servers/{id} DTO mapping for an ONLINE server
//!     (mock agent connected + SystemInfo handshake): is the row marked online
//!     via populated agent_local_capabilities / effective_capabilities, and is
//!     has_token = true after enrollment.
//!
//! Skipped (NOTE): a clean "recover with revoke_immediately = true returns the
//! server to a verifiable reconnect" loop is not asserted end-to-end — the post-
//! commit `remove_connection` kick is observed indirectly via has_token = false
//! and a re-list. There is no HTTP surface to assert the WS was dropped.

mod common;

use common::{
    connect_agent, create_server, http_client, login_admin, login_as_new_user, recv_agent_text,
    register_agent, send_system_info, start_test_server, AgentSink,
};
use serde_json::{json, Value};
use serverbee_common::constants::CAP_DEFAULT;

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

/// Bring a mock agent fully online for `register_agent`'s server: connect the
/// WS, consume the welcome, complete the SystemInfo handshake reporting `caps`,
/// then spawn a background loop that keeps draining server pushes (the
/// first-connect ping/network-sync + firewall blocklist noise) so the
/// connection stays live and thus `is_online` stays true for the duration of
/// the test. Returns the kept-alive sink (drop it to disconnect) plus the
/// JoinHandle for the drain loop.
async fn bring_agent_online(
    base_url: &str,
    token: &str,
    caps: u32,
) -> (AgentSink, tokio::task::JoinHandle<()>) {
    let (mut sink, mut reader) = connect_agent(base_url, token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_system_info(&mut sink, &mut reader, "extra-online-sysinfo", Some(caps)).await;

    // Keep reading so the WS stays open; the only frames expected after the Ack
    // are the first-connect ping/network/blocklist sync pushes, which we ignore.
    let drain = tokio::spawn(async move {
        loop {
            let _ = recv_agent_text(&mut reader).await;
        }
    });
    (sink, drain)
}

// ---------------------------------------------------------------------------
// POST /api/servers/{id}/recover — happy + side-effect branches
// ---------------------------------------------------------------------------

// Recover on an already-enrolled server (token_hash IS NOT NULL) with
// revoke_immediately = false mints a fresh bound enrollment and leaves the
// token intact (has_token stays true).
#[tokio::test]
async fn recover_enrolled_server_mints_enrollment_keeps_token() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    // register_agent logs `admin` in and enrolls a server (token set, the
    // create-time enrollment is consumed during registration).
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    // Sanity: the enrolled server has a token and no outstanding enrollment.
    let before: Value = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(before["data"]["has_token"].as_bool(), Some(true));
    assert!(before["data"]["outstanding_enrollment"].is_null(), "no outstanding before recover");

    let resp = admin
        .post(format!("{}/api/servers/{}/recover", base_url, server_id))
        .json(&json!({ "revoke_immediately": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "recover on an enrolled server should succeed");
    let body: Value = resp.json().await.unwrap();
    let enrollment = &body["data"]["enrollment"];
    assert!(
        enrollment["code"].as_str().is_some_and(|s| !s.is_empty()),
        "recover must return a fresh plaintext code"
    );
    assert!(enrollment["code_prefix"].as_str().is_some());

    // Token is untouched (revoke_immediately = false), and the new bound
    // enrollment now surfaces as outstanding on the detail DTO.
    let after: Value = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["data"]["has_token"].as_bool(), Some(true), "token preserved");
    assert_eq!(
        after["data"]["outstanding_enrollment"]["id"].as_str(),
        enrollment["id"].as_str(),
        "the minted enrollment is the outstanding one"
    );
}

// Recover with revoke_immediately = true clears the server token in the same
// transaction (DB side effect: has_token flips to false) so the server returns
// to pending until the new code is consumed.
#[tokio::test]
async fn recover_revoke_immediately_clears_token() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers/{}/recover", base_url, server_id))
        .json(&json!({ "revoke_immediately": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "recover with revoke should succeed");

    // The token was cleared inside the recover transaction.
    let after: Value = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        after["data"]["has_token"].as_bool(),
        Some(false),
        "revoke_immediately must clear the server token (back to pending)"
    );
    // A new bound enrollment is outstanding for the (now pending) server.
    assert!(
        after["data"]["outstanding_enrollment"].is_object(),
        "a fresh outstanding enrollment exists after revoke-recover"
    );
}

// Recover never auto-supersedes: a second recover while an enrollment is still
// outstanding is rejected with 409.
#[tokio::test]
async fn recover_with_outstanding_enrollment_409() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    // First recover (revoke = false) mints an outstanding enrollment.
    let first = admin
        .post(format!("{}/api/servers/{}/recover", base_url, server_id))
        .json(&json!({ "revoke_immediately": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200, "first recover should succeed");

    // Second recover sees the still-outstanding enrollment and refuses.
    let second = admin
        .post(format!("{}/api/servers/{}/recover", base_url, server_id))
        .json(&json!({ "revoke_immediately": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        second.status(),
        409,
        "recover must 409 while an enrollment is still outstanding"
    );
}

// ---------------------------------------------------------------------------
// POST /api/servers/{id}/regenerate-code — CAS pass + not-pending branches
// ---------------------------------------------------------------------------

// CAS pass: when expected_enrollment_id matches the current outstanding
// enrollment exactly, regenerate proceeds and rotates to a fresh id.
#[tokio::test]
async fn regenerate_code_matching_expected_id_rotates() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Capture the create-time enrollment id; it is the current outstanding one.
    let create_body: Value = admin
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "regen-cas-pass-host" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let server_id = create_body["data"]["server_id"].as_str().unwrap().to_string();
    let outstanding_id = create_body["data"]["enrollment"]["id"].as_str().unwrap().to_string();

    let resp = admin
        .post(format!("{}/api/servers/{}/regenerate-code", base_url, server_id))
        .json(&json!({ "expected_enrollment_id": outstanding_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "matching expected id should pass the CAS check");
    let body: Value = resp.json().await.unwrap();
    let new_id = body["data"]["enrollment"]["id"].as_str().unwrap();
    assert_ne!(new_id, outstanding_id, "regenerate must mint a fresh enrollment id");
    assert!(
        body["data"]["enrollment"]["code"].as_str().is_some_and(|s| !s.is_empty()),
        "regenerate must return a non-empty code"
    );

    // The detail DTO now reports the rotated enrollment as outstanding.
    let detail: Value = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        detail["data"]["outstanding_enrollment"]["id"].as_str(),
        Some(new_id),
        "the rotated enrollment supersedes the previous one"
    );
}

// regenerate-code on an already-enrolled server (token_hash IS NOT NULL) is
// rejected with 400 — that path must use recover instead.
#[tokio::test]
async fn regenerate_code_enrolled_server_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers/{}/regenerate-code", base_url, server_id))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        400,
        "regenerate-code on an enrolled (non-pending) server is rejected"
    );
}

// ---------------------------------------------------------------------------
// POST /api/servers/batch-delete — partial (known + unknown) ids
// ---------------------------------------------------------------------------

// A batch containing one real id plus an unknown id deletes only the real one
// (count == 1) and the real one is actually gone; the unknown id is a no-op.
#[tokio::test]
async fn batch_delete_mixed_known_and_unknown() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let real = create_server(&admin, &base_url, "batch-mixed-real").await;

    let resp = admin
        .post(format!("{}/api/servers/batch-delete", base_url))
        .json(&json!({ "ids": [real, "ghost-id-not-present"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["deleted"].as_u64(),
        Some(1),
        "only the one real id is deleted; the unknown id is a no-op"
    );

    // The real server is gone.
    let after = admin
        .get(format!("{}/api/servers/{}", base_url, real))
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), 404, "the real server was deleted");
}

// ---------------------------------------------------------------------------
// PUT /api/servers/{id} — group move + weight/hidden edge values
// ---------------------------------------------------------------------------

// Moving a server between two groups updates group_id to the new group (not
// just assign-then-clear, which router_server_crud.rs already covers).
#[tokio::test]
async fn update_server_group_move_between_groups() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "group-move-host").await;

    let ga: Value = admin
        .post(format!("{}/api/server-groups", base_url))
        .json(&json!({ "name": "move-group-a" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let group_a = ga["data"]["id"].as_str().unwrap().to_string();
    let gb: Value = admin
        .post(format!("{}/api/server-groups", base_url))
        .json(&json!({ "name": "move-group-b" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let group_b = gb["data"]["id"].as_str().unwrap().to_string();

    // Assign group A.
    let r1: Value = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "group_id": group_a }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(r1["data"]["group_id"].as_str(), Some(group_a.as_str()));

    // Move to group B.
    let r2: Value = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "group_id": group_b }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        r2["data"]["group_id"].as_str(),
        Some(group_b.as_str()),
        "the server should now belong to group B"
    );

    // Reload to confirm persistence of the move.
    let reloaded: Value = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reloaded["data"]["group_id"].as_str(), Some(group_b.as_str()));
}

// Negative weight is accepted (no validation rejects it) and persists; hidden
// can be toggled true then back to false.
#[tokio::test]
async fn update_server_negative_weight_and_hidden_toggle() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "weight-edge-host").await;

    // Negative weight + hide.
    let hidden: Value = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "weight": -5, "hidden": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hidden["data"]["weight"].as_i64(), Some(-5), "negative weight persists");
    assert_eq!(hidden["data"]["hidden"].as_bool(), Some(true));

    // Toggle hidden back to false (weight unchanged).
    let unhidden: Value = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "hidden": false }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unhidden["data"]["hidden"].as_bool(), Some(false));
    assert_eq!(
        unhidden["data"]["weight"].as_i64(),
        Some(-5),
        "weight is untouched when only hidden is updated"
    );
}

// Setting country_code via update pins it as a manual override (geo_manual =
// true, code uppercased); explicit null clears the override (geo_manual =
// false, country_code null).
#[tokio::test]
async fn update_server_country_code_override_then_clear() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "geo-override-host").await;

    // Pin a manual country code (lowercase input is uppercased server-side).
    let pinned: Value = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "country_code": "jp" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        pinned["data"]["country_code"].as_str(),
        Some("JP"),
        "country_code is uppercased to ISO alpha-2"
    );
    assert_eq!(
        pinned["data"]["geo_manual"].as_bool(),
        Some(true),
        "pinning a country code sets the manual-override flag"
    );

    // Clear the override (explicit JSON null).
    let cleared: Value = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "country_code": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(cleared["data"]["country_code"].is_null(), "country_code cleared");
    assert_eq!(
        cleared["data"]["geo_manual"].as_bool(),
        Some(false),
        "clearing the override resets geo_manual to false"
    );
}

// Invalid country_code (not a 2-letter alpha-2 code) is a validation error.
#[tokio::test]
async fn update_server_invalid_country_code_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "geo-bad-host").await;

    let resp = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "country_code": "USA" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "a 3-letter country code is a validation error");
}

// ---------------------------------------------------------------------------
// GET /api/servers (+ detail) — DTO mapping for an ONLINE server
// ---------------------------------------------------------------------------

// With a mock agent connected and its SystemInfo handshake complete, the list
// and detail DTOs surface the agent-reported capability fields and has_token,
// distinguishing an online enrolled server from a pending one.
#[tokio::test]
async fn list_and_detail_dto_for_online_server() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    let (server_id, token) = register_agent(&admin, &base_url).await;

    // Bring the agent online with a deterministic capability bitmask so the DTO
    // fields are assertable. Keep `_sink` (and the `drain` task) alive for the
    // whole test so the WS stays connected (is_online stays true).
    let (_sink, drain) = bring_agent_online(&base_url, &token, CAP_DEFAULT).await;

    // Detail DTO: enrolled server has a token and reports live capabilities.
    let detail: Value = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        detail["data"]["has_token"].as_bool(),
        Some(true),
        "an enrolled server has a token"
    );
    assert_eq!(
        detail["data"]["agent_local_capabilities"].as_i64(),
        Some(CAP_DEFAULT as i64),
        "the agent-reported capability bitmask is surfaced on the DTO"
    );
    assert_eq!(
        detail["data"]["effective_capabilities"].as_i64(),
        Some(CAP_DEFAULT as i64),
        "effective capabilities mirror the agent-reported value"
    );
    // SystemInfo populated the hardware fields from the handshake.
    assert_eq!(detail["data"]["os"].as_str(), Some("Ubuntu 22.04"));
    assert_eq!(detail["data"]["cpu_cores"].as_i64(), Some(8));

    // List DTO: the same server is present with the same online capability fields.
    let list: Value = admin
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let row = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"].as_str() == Some(server_id.as_str()))
        .expect("the online server should appear in the list");
    assert_eq!(
        row["agent_local_capabilities"].as_i64(),
        Some(CAP_DEFAULT as i64),
        "list DTO also carries the live capability bitmask for an online server"
    );
    assert!(
        row["outstanding_enrollment"].is_null(),
        "an enrolled (consumed) server has no outstanding enrollment"
    );

    // A still-pending server in the SAME list has no live capabilities and is
    // not online — proves the per-row DTO branch differs by online state.
    let pending_id = create_server(&admin, &base_url, "pending-alongside-online").await;
    let list2: Value = admin
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pending_row = list2["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"].as_str() == Some(pending_id.as_str()))
        .expect("pending server should be listed");
    assert_eq!(pending_row["has_token"].as_bool(), Some(false), "pending server has no token");
    assert!(
        pending_row["agent_local_capabilities"].is_null(),
        "a pending/offline server reports no live agent capabilities"
    );
    assert!(
        pending_row["outstanding_enrollment"].is_object(),
        "a pending server still has an outstanding enrollment"
    );

    // Member can read the list too (read DTO is not gated by role).
    let member = login_as_new_user(&admin, &base_url, "online_dto_member", "member").await;
    let member_list = member
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(member_list.status(), 200, "members may read the server list");

    // Tidy up the background drain task.
    drain.abort();
}
