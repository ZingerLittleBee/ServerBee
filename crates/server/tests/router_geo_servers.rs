//! Router-level integration tests for geo/database, server-grouping, tagging,
//! uptime, status-page, and about endpoints. Exercises real HTTP requests
//! against a freshly migrated in-memory-ish (temp-file) server per test.
mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// about — public endpoint
// ---------------------------------------------------------------------------

// GET /api/about is public and reports the build version without any auth.
#[tokio::test]
async fn about_is_public_and_returns_version() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client(); // no login at all

    let resp = client
        .get(format!("{}/api/about", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "about must be reachable unauthenticated");
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["version"].as_str().is_some(),
        "about must report a version string"
    );
}

// ---------------------------------------------------------------------------
// geoip — status (read) + download (admin write)
// ---------------------------------------------------------------------------

// GET /api/geoip/status works for an authenticated member and reports not-installed.
#[tokio::test]
async fn geoip_status_authenticated_reports_installed_flag() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "geo_member", "member").await;

    let resp = member
        .get(format!("{}/api/geoip/status", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    // No mmdb is configured in tests, so the DB is reported as not installed.
    assert_eq!(body["data"]["installed"].as_bool(), Some(false));
}

// GET /api/geoip/status requires authentication.
#[tokio::test]
async fn geoip_status_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/geoip/status", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// POST /api/geoip/download is admin-only: a member gets 403.
#[tokio::test]
async fn geoip_download_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "geo_dl_member", "member").await;

    let resp = member
        .post(format!("{}/api/geoip/download", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "geoip download is admin-only");
}

// POST /api/geoip/download requires authentication.
#[tokio::test]
async fn geoip_download_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/geoip/download", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// NOTE: POST /api/geoip/download as admin performs a real DB-IP network
// download. The handler catches download errors and still returns 200 with
// { success: false }, so we assert only the 200 envelope shape, not success,
// to avoid depending on external network reachability in CI.
#[tokio::test]
async fn geoip_download_admin_returns_envelope() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/geoip/download", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    // success may be true or false depending on network; the field must exist.
    assert!(body["data"]["success"].is_boolean());
    assert!(body["data"]["message"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// asn — status (read) + download (admin write)
// ---------------------------------------------------------------------------

// GET /api/asn/status works for an authenticated member and reports not-installed.
#[tokio::test]
async fn asn_status_authenticated_reports_installed_flag() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "asn_member", "member").await;

    let resp = member
        .get(format!("{}/api/asn/status", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["installed"].as_bool(), Some(false));
}

// GET /api/asn/status requires authentication.
#[tokio::test]
async fn asn_status_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/asn/status", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// POST /api/asn/download is admin-only: a member gets 403.
#[tokio::test]
async fn asn_download_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "asn_dl_member", "member").await;

    let resp = member
        .post(format!("{}/api/asn/download", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "asn download is admin-only");
}

// NOTE: POST /api/asn/download as admin performs a real DB-IP-ASN network
// download. The handler swallows download errors into { success: false } with
// a 200 status, so we assert only the envelope, not success, for determinism.
#[tokio::test]
async fn asn_download_admin_returns_envelope() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/asn/download", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["success"].is_boolean());
    assert!(body["data"]["message"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// server-groups — list (read) + create/update/delete (admin write)
// ---------------------------------------------------------------------------

// Full admin CRUD lifecycle for a server group: create, list, update, delete.
#[tokio::test]
async fn server_group_crud_lifecycle() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Create.
    let resp = admin
        .post(format!("{}/api/server-groups", base_url))
        .json(&json!({ "name": "production" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let created: Value = resp.json().await.unwrap();
    let group_id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["name"].as_str(), Some("production"));
    assert_eq!(created["data"]["weight"].as_i64(), Some(0));

    // List includes the new group.
    let list: Value = admin
        .get(format!("{}/api/server-groups", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ids: Vec<&str> = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|g| g["id"].as_str())
        .collect();
    assert!(ids.contains(&group_id.as_str()));

    // Update name + weight.
    let resp = admin
        .put(format!("{}/api/server-groups/{}", base_url, group_id))
        .json(&json!({ "name": "prod-renamed", "weight": 50 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: Value = resp.json().await.unwrap();
    assert_eq!(updated["data"]["name"].as_str(), Some("prod-renamed"));
    assert_eq!(updated["data"]["weight"].as_i64(), Some(50));

    // Delete.
    let resp = admin
        .delete(format!("{}/api/server-groups/{}", base_url, group_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

// Creating a group with an empty name is a validation error (422).
#[tokio::test]
async fn server_group_create_empty_name_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/server-groups", base_url))
        .json(&json!({ "name": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "empty group name is a validation error");
}

// Creating a duplicate group name returns 409 Conflict.
#[tokio::test]
async fn server_group_create_duplicate_409() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    admin
        .post(format!("{}/api/server-groups", base_url))
        .json(&json!({ "name": "dup-group" }))
        .send()
        .await
        .unwrap();
    let resp = admin
        .post(format!("{}/api/server-groups", base_url))
        .json(&json!({ "name": "dup-group" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409, "duplicate group name conflicts");
}

// Updating a non-existent group returns 404.
#[tokio::test]
async fn server_group_update_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/server-groups/{}", base_url, "does-not-exist"))
        .json(&json!({ "name": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// Deleting a non-existent group returns 404.
#[tokio::test]
async fn server_group_delete_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .delete(format!("{}/api/server-groups/{}", base_url, "does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// GET /api/server-groups is readable by an authenticated member.
#[tokio::test]
async fn server_group_list_member_allowed() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "grp_member", "member").await;

    let resp = member
        .get(format!("{}/api/server-groups", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "members may read server groups");
}

// POST /api/server-groups is admin-only: a member gets 403.
#[tokio::test]
async fn server_group_create_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "grp_create_member", "member").await;

    let resp = member
        .post(format!("{}/api/server-groups", base_url))
        .json(&json!({ "name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not create server groups");
}

// GET /api/server-groups requires authentication.
#[tokio::test]
async fn server_group_list_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/server-groups", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// server-tags — get (read) + set (admin write)
// ---------------------------------------------------------------------------

// Admin can set tags on a server, and the canonical normalized list comes back.
#[tokio::test]
async fn server_tags_set_and_get_roundtrip() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tag-host").await;

    // PUT normalizes (trims, dedupes, sorts ascending).
    let resp = admin
        .put(format!("{}/api/servers/{}/tags", base_url, server_id))
        .json(&json!({ "tags": ["  web ", "db", "web"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let set_body: Value = resp.json().await.unwrap();
    assert_eq!(
        set_body["data"].as_array().unwrap(),
        &vec![json!("db"), json!("web")],
        "tags must be trimmed, deduped and sorted"
    );

    // GET returns the persisted, sorted tags.
    let get_body: Value = admin
        .get(format!("{}/api/servers/{}/tags", base_url, server_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(get_body["data"].as_array().unwrap(), &vec![json!("db"), json!("web")]);
}

// GET tags is readable by an authenticated member; empty when none set.
#[tokio::test]
async fn server_tags_get_member_allowed_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tag-empty-host").await;
    let member = login_as_new_user(&admin, &base_url, "tag_member", "member").await;

    let resp = member
        .get(format!("{}/api/servers/{}/tags", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "members may read tags");
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].as_array().unwrap().is_empty());
}

// Setting tags with an invalid character is a validation error (422).
#[tokio::test]
async fn server_tags_set_invalid_char_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tag-bad-host").await;

    let resp = admin
        .put(format!("{}/api/servers/{}/tags", base_url, server_id))
        .json(&json!({ "tags": ["bad space"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "invalid tag chars are a validation error");
}

// Setting too many tags (>8) is a validation error (422).
#[tokio::test]
async fn server_tags_set_too_many_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tag-many-host").await;

    let too_many: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
    let resp = admin
        .put(format!("{}/api/servers/{}/tags", base_url, server_id))
        .json(&json!({ "tags": too_many }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "more than 8 tags is a validation error");
}

// PUT tags is admin-only: a member gets 403.
#[tokio::test]
async fn server_tags_set_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tag-authz-host").await;
    let member = login_as_new_user(&admin, &base_url, "tag_set_member", "member").await;

    let resp = member
        .put(format!("{}/api/servers/{}/tags", base_url, server_id))
        .json(&json!({ "tags": ["nope"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not set tags");
}

// GET tags requires authentication.
#[tokio::test]
async fn server_tags_get_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tag-unauth-host").await;

    let anon = http_client();
    let resp = anon
        .get(format!("{}/api/servers/{}/tags", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// uptime-daily — read with days bounds + server existence
// ---------------------------------------------------------------------------

// Default uptime-daily request returns 90 gap-filled entries for a fresh server.
#[tokio::test]
async fn uptime_daily_default_returns_90_entries() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "uptime-host").await;

    let resp = admin
        .get(format!("{}/api/servers/{}/uptime-daily", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 90, "default days is 90");
    // Each entry has the expected zero-filled shape.
    assert_eq!(entries[0]["total_minutes"].as_i64(), Some(0));
}

// A custom in-range days value returns exactly that many entries.
#[tokio::test]
async fn uptime_daily_custom_days_in_range() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "uptime-host-7").await;

    let resp = admin
        .get(format!("{}/api/servers/{}/uptime-daily?days=7", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 7);
}

// days below the lower bound (0) is rejected with 400.
#[tokio::test]
async fn uptime_daily_days_zero_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "uptime-host-zero").await;

    let resp = admin
        .get(format!("{}/api/servers/{}/uptime-daily?days=0", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "days=0 is out of the 1..=365 range");
}

// days above the upper bound (366) is rejected with 400.
#[tokio::test]
async fn uptime_daily_days_over_max_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "uptime-host-over").await;

    let resp = admin
        .get(format!("{}/api/servers/{}/uptime-daily?days=366", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "days=366 exceeds the 1..=365 range");
}

// uptime-daily on a missing server returns 404 (bounds are checked first, so use valid days).
#[tokio::test]
async fn uptime_daily_missing_server_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/servers/{}/uptime-daily?days=30", base_url, "no-such-server"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "unknown server id yields 404");
}

// uptime-daily is readable by an authenticated member.
#[tokio::test]
async fn uptime_daily_member_allowed() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "uptime-member-host").await;
    let member = login_as_new_user(&admin, &base_url, "uptime_member", "member").await;

    let resp = member
        .get(format!("{}/api/servers/{}/uptime-daily?days=5", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "members may read uptime data");
}

// uptime-daily requires authentication.
#[tokio::test]
async fn uptime_daily_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "uptime-unauth-host").await;

    let anon = http_client();
    let resp = anon
        .get(format!("{}/api/servers/{}/uptime-daily", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// status-page (admin singleton) — get (read) + update (admin write)
// ---------------------------------------------------------------------------

// GET /api/status-page returns the seeded singleton config with default title.
#[tokio::test]
async fn status_page_get_returns_singleton() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/status-page", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    // The migration seeds a single row titled "Status", disabled by default.
    assert_eq!(body["data"]["title"].as_str(), Some("Status"));
    assert_eq!(body["data"]["enabled"].as_bool(), Some(false));
}

// PUT /api/status-page applies a partial update (admin only) and persists it.
#[tokio::test]
async fn status_page_update_persists_changes() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/status-page", base_url))
        .json(&json!({ "title": "My Status", "enabled": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: Value = resp.json().await.unwrap();
    assert_eq!(updated["data"]["title"].as_str(), Some("My Status"));
    assert!(updated["data"]["enabled"].as_bool().unwrap());

    // Reloading proves the change was persisted to the singleton row.
    let reloaded: Value = admin
        .get(format!("{}/api/status-page", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reloaded["data"]["title"].as_str(), Some("My Status"));
    assert!(reloaded["data"]["enabled"].as_bool().unwrap());
}

// GET /api/status-page is readable by an authenticated member.
#[tokio::test]
async fn status_page_get_member_allowed() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "sp_member", "member").await;

    let resp = member
        .get(format!("{}/api/status-page", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "members may read the status-page config");
}

// PUT /api/status-page is admin-only: a member gets 403.
#[tokio::test]
async fn status_page_update_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "sp_update_member", "member").await;

    let resp = member
        .put(format!("{}/api/status-page", base_url))
        .json(&json!({ "title": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "members may not update the status page");
}

// GET /api/status-page requires authentication.
#[tokio::test]
async fn status_page_get_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/status-page", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
