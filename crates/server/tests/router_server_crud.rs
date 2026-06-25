//! Router-level integration tests for server CRUD + management endpoints under
//! `crates/server/src/router/api/server.rs`. Exercises real HTTP requests
//! against a freshly migrated temp-file server per test.
//!
//! Covered: POST /api/servers (create + enrollment), GET /api/servers (list),
//! GET /api/servers/{id} (detail + 404), PUT /api/servers/{id} (update + 404),
//! DELETE /api/servers/{id} (delete + 404), POST /api/servers/batch-delete,
//! POST /api/servers/{id}/recover, POST /api/servers/{id}/regenerate-code,
//! plus authZ (member -> 403 on admin writes, reads OK), unauth -> 401, and
//! validation (empty name -> 400, bad tags -> 422).
//!
//! Server-group / server-tag endpoints are intentionally NOT duplicated here —
//! they live in router_geo_servers.rs.
mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// POST /api/servers — create (admin write)
// ---------------------------------------------------------------------------

// Happy path: creating a server returns a server_id and a one-time enrollment.
#[tokio::test]
async fn create_server_returns_id_and_enrollment() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "create-host" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    // server_id is present.
    assert!(
        body["data"]["server_id"].as_str().is_some_and(|s| !s.is_empty()),
        "create must return a non-empty server_id"
    );
    // Enrollment plaintext code is shown exactly once at mint time.
    let enrollment = &body["data"]["enrollment"];
    assert!(enrollment["code"].as_str().is_some_and(|s| !s.is_empty()), "enrollment code missing");
    assert!(enrollment["id"].as_str().is_some(), "enrollment id missing");
    assert!(enrollment["code_prefix"].as_str().is_some(), "enrollment code_prefix missing");
    assert!(enrollment["expires_at"].as_str().is_some(), "enrollment expires_at missing");
}

// Created servers accept optional metadata (group_id, tags, billing) at create.
#[tokio::test]
async fn create_server_with_tags_and_metadata() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers", base_url))
        .json(&json!({
            "name": "meta-host",
            "tags": ["web", "db"],
            "remark": "primary",
            "price": 5.0,
            "billing_cycle": "monthly",
            "currency": "USD"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let server_id = body["data"]["server_id"].as_str().unwrap().to_string();

    // The created server surfaces in the detail endpoint with the supplied metadata.
    let detail: Value = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["data"]["name"].as_str(), Some("meta-host"));
    assert_eq!(detail["data"]["remark"].as_str(), Some("primary"));
    assert_eq!(detail["data"]["billing_cycle"].as_str(), Some("monthly"));
    // A freshly created server is pending: no token yet.
    assert_eq!(detail["data"]["has_token"].as_bool(), Some(false));
    // The outstanding enrollment is exposed (without the plaintext code).
    assert!(detail["data"]["outstanding_enrollment"].is_object());
}

// Empty (whitespace-only) name is a BadRequest (400).
#[tokio::test]
async fn create_server_empty_name_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "empty name is a BadRequest");
}

// Tags with invalid characters are rejected as a validation error (422).
#[tokio::test]
async fn create_server_bad_tags_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "bad-tag-host", "tags": ["bad space"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "invalid tag chars are a validation error");
}

// Too many tags (>8) is a validation error (422).
#[tokio::test]
async fn create_server_too_many_tags_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let too_many: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
    let resp = admin
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "many-tag-host", "tags": too_many }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "more than 8 tags is a validation error");
}

// POST /api/servers is admin-only: a member gets 403.
#[tokio::test]
async fn create_server_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "create_member", "member").await;

    let resp = member
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not create servers");
}

// POST /api/servers requires authentication.
#[tokio::test]
async fn create_server_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/servers — list (read)
// ---------------------------------------------------------------------------

// List returns an array containing the created server.
#[tokio::test]
async fn list_servers_includes_created() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "list-host").await;

    let resp = admin
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let servers = body["data"].as_array().expect("data should be an array");
    assert!(
        servers.iter().any(|s| s["id"].as_str() == Some(server_id.as_str())),
        "created server should appear in the list"
    );
}

// GET /api/servers is readable by an authenticated member.
#[tokio::test]
async fn list_servers_member_allowed() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "list_member", "member").await;

    let resp = member
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "members may read the server list");
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].is_array());
}

// GET /api/servers requires authentication.
#[tokio::test]
async fn list_servers_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/servers/{id} — detail (read)
// ---------------------------------------------------------------------------

// Happy path: detail returns the server with its name field.
#[tokio::test]
async fn get_server_returns_detail() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "detail-host").await;

    let resp = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["id"].as_str(), Some(server_id.as_str()));
    assert_eq!(body["data"]["name"].as_str(), Some("detail-host"));
}

// Unknown server id yields 404.
#[tokio::test]
async fn get_server_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/servers/{}", base_url, "no-such-server"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "unknown server id yields 404");
}

// GET /api/servers/{id} is readable by an authenticated member.
#[tokio::test]
async fn get_server_member_allowed() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "detail-member-host").await;
    let member = login_as_new_user(&admin, &base_url, "detail_member", "member").await;

    let resp = member
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "members may read server detail");
}

// GET /api/servers/{id} requires authentication.
#[tokio::test]
async fn get_server_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "detail-unauth-host").await;

    let anon = http_client();
    let resp = anon
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// PUT /api/servers/{id} — update (admin write)
// ---------------------------------------------------------------------------

// Happy path: update name, weight, and hidden, then verify they persist.
#[tokio::test]
async fn update_server_persists_fields() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "update-host").await;

    let resp = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "name": "renamed-host", "weight": 42, "hidden": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["name"].as_str(), Some("renamed-host"));
    assert_eq!(body["data"]["weight"].as_i64(), Some(42));
    assert_eq!(body["data"]["hidden"].as_bool(), Some(true));

    // Reloading proves the change was persisted.
    let reloaded: Value = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reloaded["data"]["name"].as_str(), Some("renamed-host"));
    assert_eq!(reloaded["data"]["weight"].as_i64(), Some(42));
    assert_eq!(reloaded["data"]["hidden"].as_bool(), Some(true));
}

// Assigning a group via update persists the group_id; clearing it (null) wipes it.
#[tokio::test]
async fn update_server_group_assignment_and_clear() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "group-host").await;

    // Create a server group to assign.
    let group: Value = admin
        .post(format!("{}/api/server-groups", base_url))
        .json(&json!({ "name": "assign-group" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let group_id = group["data"]["id"].as_str().unwrap().to_string();

    // Assign the group.
    let resp = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "group_id": group_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["group_id"].as_str(), Some(group_id.as_str()));

    // Clearing the group (explicit null) removes the assignment.
    let cleared: Value = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "group_id": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(cleared["data"]["group_id"].is_null(), "group_id should be cleared");
}

// Invalid billing_cycle is a validation error (422).
#[tokio::test]
async fn update_server_invalid_billing_cycle_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "billing-host").await;

    let resp = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "billing_cycle": "weekly" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "invalid billing_cycle is a validation error");
}

// Negative price is a validation error (422).
#[tokio::test]
async fn update_server_negative_price_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "price-host").await;

    let resp = admin
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "price": -1.0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "negative price is a validation error");
}

// Updating a non-existent server returns 404.
#[tokio::test]
async fn update_server_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/servers/{}", base_url, "no-such-server"))
        .json(&json!({ "name": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "unknown server id yields 404");
}

// PUT /api/servers/{id} is admin-only: a member gets 403.
#[tokio::test]
async fn update_server_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "update-authz-host").await;
    let member = login_as_new_user(&admin, &base_url, "update_member", "member").await;

    let resp = member
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not update servers");
}

// PUT /api/servers/{id} requires authentication.
#[tokio::test]
async fn update_server_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "update-unauth-host").await;

    let anon = http_client();
    let resp = anon
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// DELETE /api/servers/{id} — delete (admin write)
// ---------------------------------------------------------------------------

// Happy path: delete removes the server (subsequent GET is 404).
#[tokio::test]
async fn delete_server_removes_it() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "delete-host").await;

    let resp = admin
        .delete(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Server is gone afterwards.
    let after = admin
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), 404, "deleted server should no longer exist");
}

// Deleting a non-existent server returns 404.
#[tokio::test]
async fn delete_server_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .delete(format!("{}/api/servers/{}", base_url, "no-such-server"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "unknown server id yields 404");
}

// DELETE /api/servers/{id} is admin-only: a member gets 403.
#[tokio::test]
async fn delete_server_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "delete-authz-host").await;
    let member = login_as_new_user(&admin, &base_url, "delete_member", "member").await;

    let resp = member
        .delete(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not delete servers");
}

// DELETE /api/servers/{id} requires authentication.
#[tokio::test]
async fn delete_server_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "delete-unauth-host").await;

    let anon = http_client();
    let resp = anon
        .delete(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// POST /api/servers/batch-delete — batch delete (admin write)
// ---------------------------------------------------------------------------

// Happy path: batch delete removes multiple servers and reports the count.
#[tokio::test]
async fn batch_delete_removes_multiple() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let a = create_server(&admin, &base_url, "batch-a").await;
    let b = create_server(&admin, &base_url, "batch-b").await;

    let resp = admin
        .post(format!("{}/api/servers/batch-delete", base_url))
        .json(&json!({ "ids": [a, b] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["deleted"].as_u64(), Some(2), "two servers deleted");

    // Both are gone.
    let after_a = admin
        .get(format!("{}/api/servers/{}", base_url, a))
        .send()
        .await
        .unwrap();
    assert_eq!(after_a.status(), 404);
    let after_b = admin
        .get(format!("{}/api/servers/{}", base_url, b))
        .send()
        .await
        .unwrap();
    assert_eq!(after_b.status(), 404);
}

// Batch delete with unknown ids reports zero deletions (no-op, still 200).
#[tokio::test]
async fn batch_delete_unknown_ids_zero() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers/batch-delete", base_url))
        .json(&json!({ "ids": ["unknown-1", "unknown-2"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["deleted"].as_u64(), Some(0), "unknown ids delete nothing");
}

// POST /api/servers/batch-delete is admin-only: a member gets 403.
#[tokio::test]
async fn batch_delete_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "batch-authz-host").await;
    let member = login_as_new_user(&admin, &base_url, "batch_member", "member").await;

    let resp = member
        .post(format!("{}/api/servers/batch-delete", base_url))
        .json(&json!({ "ids": [server_id] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not batch-delete servers");
}

// POST /api/servers/batch-delete requires authentication.
#[tokio::test]
async fn batch_delete_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .post(format!("{}/api/servers/batch-delete", base_url))
        .json(&json!({ "ids": ["x"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// POST /api/servers/{id}/recover — recover enrollment (admin write)
// ---------------------------------------------------------------------------

// Recover on a freshly created (pending) server is rejected with 400 — a pending
// server should use regenerate-code instead.
#[tokio::test]
async fn recover_pending_server_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "recover-pending-host").await;

    let resp = admin
        .post(format!("{}/api/servers/{}/recover", base_url, server_id))
        .json(&json!({ "revoke_immediately": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "recover on a pending server is rejected");
}

// Recover on a non-existent server returns 404.
#[tokio::test]
async fn recover_missing_server_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers/{}/recover", base_url, "no-such-server"))
        .json(&json!({ "revoke_immediately": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "unknown server id yields 404");
}

// POST recover is admin-only: a member gets 403.
#[tokio::test]
async fn recover_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "recover-authz-host").await;
    let member = login_as_new_user(&admin, &base_url, "recover_member", "member").await;

    let resp = member
        .post(format!("{}/api/servers/{}/recover", base_url, server_id))
        .json(&json!({ "revoke_immediately": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not recover servers");
}

// POST recover requires authentication.
#[tokio::test]
async fn recover_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "recover-unauth-host").await;

    let anon = http_client();
    let resp = anon
        .post(format!("{}/api/servers/{}/recover", base_url, server_id))
        .json(&json!({ "revoke_immediately": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// POST /api/servers/{id}/regenerate-code — regenerate enrollment (admin write)
// ---------------------------------------------------------------------------

// Happy path: regenerate on a pending server mints a fresh enrollment, replacing
// the outstanding one (last-writer-wins when no expected id is supplied).
#[tokio::test]
async fn regenerate_code_pending_server_mints_new() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Create returns the initial enrollment id; capture it to verify rotation.
    let create_body: Value = admin
        .post(format!("{}/api/servers", base_url))
        .json(&json!({ "name": "regen-host" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let server_id = create_body["data"]["server_id"].as_str().unwrap().to_string();
    let original_enrollment_id = create_body["data"]["enrollment"]["id"].as_str().unwrap().to_string();

    let resp = admin
        .post(format!("{}/api/servers/{}/regenerate-code", base_url, server_id))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let enrollment = &body["data"]["enrollment"];
    let new_code = enrollment["code"].as_str().expect("regenerate must return a fresh code");
    assert!(!new_code.is_empty(), "regenerated code must be non-empty");
    assert_ne!(
        enrollment["id"].as_str(),
        Some(original_enrollment_id.as_str()),
        "regenerate must mint a fresh enrollment id"
    );
}

// Optimistic concurrency: a stale expected_enrollment_id is rejected with 409.
#[tokio::test]
async fn regenerate_code_stale_expected_id_409() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "regen-cas-host").await;

    let resp = admin
        .post(format!("{}/api/servers/{}/regenerate-code", base_url, server_id))
        .json(&json!({ "expected_enrollment_id": "stale-id-that-does-not-match" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409, "expected_enrollment_id mismatch yields 409");
}

// Regenerate on a non-existent server returns 404.
#[tokio::test]
async fn regenerate_code_missing_server_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/servers/{}/regenerate-code", base_url, "no-such-server"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "unknown server id yields 404");
}

// POST regenerate-code is admin-only: a member gets 403.
#[tokio::test]
async fn regenerate_code_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "regen-authz-host").await;
    let member = login_as_new_user(&admin, &base_url, "regen_member", "member").await;

    let resp = member
        .post(format!("{}/api/servers/{}/regenerate-code", base_url, server_id))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not regenerate enrollment codes");
}

// POST regenerate-code requires authentication.
#[tokio::test]
async fn regenerate_code_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "regen-unauth-host").await;

    let anon = http_client();
    let resp = anon
        .post(format!("{}/api/servers/{}/regenerate-code", base_url, server_id))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
