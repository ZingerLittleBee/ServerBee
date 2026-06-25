//! Router-level integration tests for the security, firewall, rate-limit, and
//! audit API surfaces.
//!
//! These exercise the real Axum router through HTTP requests against a freshly
//! migrated, randomly-bound test server (see `tests/common/mod.rs`). Each test
//! gets its own temp DB so resource names never collide across tests.
//!
//! NOTE on live-agent dispatch: firewall block create/delete persist to SQLite
//! and then best-effort push the change to connected agents over WebSocket. No
//! agent is connected in these tests, so the push is a no-op and the DB-write
//! path still returns 200 — we assert that path and skip any agent-apply state.
mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// security: read events / get / stats (authenticated read), delete (admin)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn security_list_events_returns_paginated_envelope() {
    // Authenticated admin gets a well-formed empty list with no next_cursor.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/security/events", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["items"].is_array(), "items must be an array");
    assert!(body["data"]["next_cursor"].is_null(), "fresh DB has no next page");
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn security_list_events_accepts_filters() {
    // Filters (server_id, event_type, severity, source_ip) are accepted and
    // return 200 even when they match nothing.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "sec-filter-srv").await;

    let resp = admin
        .get(format!(
            "{}/api/security/events?server_id={server_id}&event_type=ssh_brute_force&severity=high&source_ip=1.2.3.4&limit=10",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn security_list_events_rejects_bad_cursor() {
    // A malformed cursor is decoded in-handler and maps to AppError::BadRequest.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/security/events?cursor=not-a-valid-cursor", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn security_list_events_readable_by_member() {
    // The security read surface is open to any authenticated user (member ok).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "sec-member", "member").await;

    let resp = member
        .get(format!("{}/api/security/events", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn security_list_events_requires_auth() {
    // An unauthenticated client is rejected by the auth middleware.
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/security/events", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn security_get_event_not_found() {
    // Unknown event id maps to AppError::NotFound (404).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/security/events/does-not-exist", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn security_stats_default_grouping() {
    // Stats with default grouping (event_type) returns a 200 array.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/security/stats", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].is_array(), "stats data must be an array");
}

#[tokio::test]
async fn security_stats_rejects_invalid_group_by() {
    // An unsupported group_by maps to AppError::BadRequest (400).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/security/stats?group_by=bogus", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn security_stats_accepts_day_grouping() {
    // group_by=day is a valid bucket and returns 200.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/security/stats?group_by=day&limit=5", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn security_delete_event_admin_not_found() {
    // Admin delete of an unknown event maps to 404 (route is reachable for admin).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .delete(format!("{}/api/security/events/missing", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn security_delete_event_forbidden_for_member() {
    // Delete lives on write_router → admin-only; a member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "sec-del-member", "member").await;

    let resp = member
        .delete(format!("{}/api/security/events/whatever", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// ---------------------------------------------------------------------------
// firewall: blocklist CRUD + stats
// ---------------------------------------------------------------------------

#[tokio::test]
async fn firewall_list_blocks_empty() {
    // Fresh DB yields an empty blocklist with no next_cursor.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/firewall/blocks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);
    assert!(body["data"]["next_cursor"].is_null());
}

#[tokio::test]
async fn firewall_create_get_delete_block_roundtrip() {
    // NOTE: agent push after the DB write is best-effort and a no-op without a
    // connected agent, so the create/delete DB path still returns 200.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Create: a public, non-protected IP canonicalizes to /32.
    let create = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "203.0.113.7", "comment": "abuse" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), 200);
    let created: Value = create.json().await.unwrap();
    let id = created["data"]["id"].as_str().expect("block id").to_string();
    assert_eq!(created["data"]["target"].as_str(), Some("203.0.113.7/32"));
    assert_eq!(created["data"]["family"].as_i64(), Some(4));
    assert_eq!(created["data"]["origin"].as_str(), Some("manual"));

    // Get the freshly created block by id.
    let get = admin
        .get(format!("{}/api/firewall/blocks/{id}", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), 200);
    let got: Value = get.json().await.unwrap();
    assert_eq!(got["data"]["id"].as_str(), Some(id.as_str()));

    // Stats reflect the single v4 manual block.
    let stats = admin
        .get(format!("{}/api/firewall/stats", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(stats.status(), 200);
    let s: Value = stats.json().await.unwrap();
    assert_eq!(s["data"]["total"].as_i64(), Some(1));
    assert_eq!(s["data"]["manual"].as_i64(), Some(1));
    assert_eq!(s["data"]["v4"].as_i64(), Some(1));
    assert_eq!(s["data"]["auto"].as_i64(), Some(0));

    // Delete the block; DB row removed, returns 200.
    let del = admin
        .delete(format!("{}/api/firewall/blocks/{id}", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);

    // Subsequent get is 404.
    let gone = admin
        .get(format!("{}/api/firewall/blocks/{id}", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
}

#[tokio::test]
async fn firewall_create_block_rejects_invalid_target() {
    // canonicalize_target fails on a non-IP/CIDR → AppError::BadRequest (400).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "not-an-ip-address" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn firewall_create_block_rejects_protected_range() {
    // Blocking a hard-coded protected range (loopback) hits the guardrail and
    // maps to AppError::Conflict (409) before any agent dispatch.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "127.0.0.0/8" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn firewall_create_block_rejects_duplicate_target() {
    // The same canonical target cannot be blocked twice → unique violation 409.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let first = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "198.51.100.42" }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 200);

    let dup = admin
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "198.51.100.42" }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup.status(), 409);
}

#[tokio::test]
async fn firewall_create_block_forbidden_for_member() {
    // create_block lives on write_router → admin-only; a member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "fw-member", "member").await;

    let resp = member
        .post(format!("{}/api/firewall/blocks", base_url))
        .json(&json!({ "target": "203.0.113.99" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn firewall_list_readable_by_member() {
    // The firewall read surface is open to any authenticated user.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "fw-read-member", "member").await;

    let resp = member
        .get(format!("{}/api/firewall/blocks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn firewall_list_requires_auth() {
    // Unauthenticated access to the firewall read surface is rejected (401).
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/firewall/blocks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn firewall_get_block_not_found() {
    // Unknown block id maps to AppError::NotFound (404).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/firewall/blocks/no-such-id", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn firewall_delete_block_not_found() {
    // Admin delete of an unknown block id maps to 404.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .delete(format!("{}/api/firewall/blocks/no-such-id", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn firewall_delete_block_forbidden_for_member() {
    // delete_block is admin-only; a member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "fw-del-member", "member").await;

    let resp = member
        .delete(format!("{}/api/firewall/blocks/anything", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn firewall_stats_empty() {
    // Stats on an empty blocklist return all-zero counts.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/firewall/stats", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(0));
    assert_eq!(body["data"]["auto"].as_i64(), Some(0));
    assert_eq!(body["data"]["manual"].as_i64(), Some(0));
}

// ---------------------------------------------------------------------------
// rate_limit: admin list + reset (admin-only)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rate_limit_list_returns_config_and_entries() {
    // Admin list returns configured maxima + an (initially empty) entries array.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/admin/rate-limit", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["entries"].is_array());
    assert!(body["data"]["login_max"].is_number());
    assert!(body["data"]["register_max"].is_number());
    assert_eq!(body["data"]["public_max"].as_u64(), Some(60));
    assert_eq!(body["data"]["public_window_seconds"].as_i64(), Some(60));
    assert_eq!(body["data"]["auth_window_seconds"].as_i64(), Some(900));
}

#[tokio::test]
async fn rate_limit_list_forbidden_for_member() {
    // The rate-limit admin surface is mounted under require_admin → member 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "rl-member", "member").await;

    let resp = member
        .get(format!("{}/api/admin/rate-limit", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn rate_limit_list_requires_auth() {
    // Unauthenticated access to the admin rate-limit surface is rejected (401).
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/admin/rate-limit", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn rate_limit_reset_all_returns_cleared_count() {
    // Reset with no filters returns a numeric cleared count and is idempotent:
    // the admin login above seeds at least one login bucket, so the first reset
    // clears >= 1, and an immediate second reset then clears 0.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/admin/rate-limit/reset", base_url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let first = body["data"]["cleared"]
        .as_u64()
        .expect("cleared should be a numeric count");
    assert!(first >= 1, "login should have seeded a rate-limit bucket");

    let resp2 = admin
        .post(format!("{}/api/admin/rate-limit/reset", base_url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let body2: Value = resp2.json().await.unwrap();
    assert_eq!(body2["data"]["cleared"].as_u64(), Some(0));
}

#[tokio::test]
async fn rate_limit_reset_scoped_by_ip() {
    // A scoped+IP reset is accepted; a non-existent IP clears nothing.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/admin/rate-limit/reset", base_url))
        .json(&json!({ "scope": "login", "ip": "9.9.9.9" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["cleared"].as_u64(), Some(0));
}

#[tokio::test]
async fn rate_limit_reset_forbidden_for_member() {
    // The reset endpoint is admin-only; a member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "rl-reset-member", "member").await;

    let resp = member
        .post(format!("{}/api/admin/rate-limit/reset", base_url))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// ---------------------------------------------------------------------------
// audit: list + options + clear (admin-only)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_list_returns_entries_and_total() {
    // Admin actions (e.g. login) produce audit rows; list returns them.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Generate an auditable admin write so the table is non-empty.
    create_server(&admin, &base_url, "audit-seed-srv").await;

    let resp = admin
        .get(format!("{}/api/audit-logs", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["entries"].is_array());
    assert!(body["data"]["total"].is_number());
}

#[tokio::test]
async fn audit_list_accepts_filters() {
    // action/user_id/limit/offset query filters are accepted and return 200.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!(
            "{}/api/audit-logs?action=login&user_id=admin&limit=10&offset=0",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["entries"].is_array());
}

#[tokio::test]
async fn audit_options_returns_actions_and_users() {
    // Options endpoint returns the distinct action + user filter choices.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/audit-logs/options", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["actions"].is_array());
    assert!(body["data"]["users"].is_array());
}

#[tokio::test]
async fn audit_list_forbidden_for_member() {
    // audit::router() is mounted under require_admin → member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "audit-member", "member").await;

    let resp = member
        .get(format!("{}/api/audit-logs", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn audit_list_requires_auth() {
    // Unauthenticated access to the audit surface is rejected (401).
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/audit-logs", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn audit_clear_removes_all_and_self_audits() {
    // Admin clear deletes all rows and returns the deleted count; it also logs
    // its own action, so a follow-up list is not necessarily empty.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Produce at least one row to clear.
    create_server(&admin, &base_url, "audit-clear-srv").await;

    let resp = admin
        .delete(format!("{}/api/audit-logs", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["deleted"].is_number());
}

#[tokio::test]
async fn audit_clear_forbidden_for_member() {
    // The DELETE clear route is admin-only; a member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "audit-clear-member", "member").await;

    let resp = member
        .delete(format!("{}/api/audit-logs", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}
