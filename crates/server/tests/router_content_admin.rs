//! Integration tests for the brand, task, setting, and traffic routers.
//!
//! Exercises each endpoint via real HTTP requests against a freshly migrated,
//! random-port test server (one temp SQLite DB per test). Covers happy paths,
//! validation errors, authz (admin-only vs member vs unauthenticated), and
//! not-found behavior. Endpoints that dispatch to a live agent over WebSocket
//! cannot fully succeed here (no agent is connected); those are noted inline.

mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

/// A minimal but valid PNG payload: the 8-byte magic header plus padding so it
/// passes the `>= 4 bytes` + `PNG_MAGIC` checks in `extract_and_validate_image`.
fn valid_png_bytes() -> Vec<u8> {
    vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D]
}

// ===========================================================================
// brand
// ===========================================================================

#[tokio::test]
async fn brand_config_public_get_returns_defaults() {
    // Public GET /api/settings/brand needs no auth and returns an empty config.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/settings/brand", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["logo_path"].is_null());
    assert!(body["data"]["site_title"].is_null());
}

#[tokio::test]
async fn brand_logo_public_get_404_when_not_uploaded() {
    // Public GET /api/brand/logo returns 404 when no logo exists on disk.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let logo = client
        .get(format!("{}/api/brand/logo", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(logo.status(), 404);

    let favicon = client
        .get(format!("{}/api/brand/favicon", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(favicon.status(), 404);
}

#[tokio::test]
async fn brand_update_config_admin_happy_path() {
    // Admin PUT /api/settings/brand with a valid /api/brand/ path persists config.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/settings/brand", base_url))
        .json(&json!({
            "logo_path": "/api/brand/logo",
            "site_title": "My Probe",
            "favicon_path": null,
            "footer_text": "footer"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["site_title"], "My Probe");

    // Read it back via the public endpoint to confirm persistence.
    let read = http_client()
        .get(format!("{}/api/settings/brand", base_url))
        .send()
        .await
        .unwrap();
    let read_body: Value = read.json().await.unwrap();
    assert_eq!(read_body["data"]["logo_path"], "/api/brand/logo");
}

#[tokio::test]
async fn brand_update_config_rejects_bad_path() {
    // logo_path not starting with /api/brand/ is a Validation error -> 422.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/settings/brand", base_url))
        .json(&json!({ "logo_path": "https://evil.example.com/x.png" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn brand_update_config_member_forbidden() {
    // PUT /api/settings/brand is admin-only -> member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "brand_member", "member").await;

    let resp = member
        .put(format!("{}/api/settings/brand", base_url))
        .json(&json!({ "site_title": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn brand_update_config_unauthenticated_401() {
    // PUT /api/settings/brand without auth -> 401.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .put(format!("{}/api/settings/brand", base_url))
        .json(&json!({ "site_title": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn brand_upload_logo_admin_valid_png() {
    // Admin POST /api/settings/brand/logo with valid PNG magic bytes -> 200,
    // and the uploaded logo is then served by the public GET endpoint.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(valid_png_bytes())
            .file_name("logo.png")
            .mime_str("image/png")
            .unwrap(),
    );
    let resp = admin
        .post(format!("{}/api/settings/brand/logo", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["path"], "/api/brand/logo");

    // The public serve endpoint now returns the bytes.
    let served = http_client()
        .get(format!("{}/api/brand/logo", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(served.status(), 200);
    assert_eq!(
        served.headers().get("content-type").unwrap(),
        "image/png"
    );
}

#[tokio::test]
async fn brand_upload_favicon_admin_valid_png() {
    // Admin POST /api/settings/brand/favicon with valid PNG magic bytes -> 200.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(valid_png_bytes())
            .file_name("favicon.png")
            .mime_str("image/png")
            .unwrap(),
    );
    let resp = admin
        .post(format!("{}/api/settings/brand/favicon", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["path"], "/api/brand/favicon");
}

#[tokio::test]
async fn brand_upload_logo_rejects_bad_magic_bytes() {
    // Non-PNG/ICO bytes fail magic-byte validation -> 422.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"not an image at all".to_vec())
            .file_name("x.png")
            .mime_str("image/png")
            .unwrap(),
    );
    let resp = admin
        .post(format!("{}/api/settings/brand/logo", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn brand_upload_logo_missing_file_field_400() {
    // Multipart without a "file" field -> BadRequest 400.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let form = reqwest::multipart::Form::new().text("other", "value");
    let resp = admin
        .post(format!("{}/api/settings/brand/logo", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn brand_upload_logo_member_forbidden() {
    // POST /api/settings/brand/logo is admin-only -> member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "logo_member", "member").await;

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(valid_png_bytes())
            .file_name("logo.png")
            .mime_str("image/png")
            .unwrap(),
    );
    let resp = member
        .post(format!("{}/api/settings/brand/logo", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// ===========================================================================
// task
// ===========================================================================

#[tokio::test]
async fn task_create_scheduled_admin_happy_path() {
    // Admin POST /api/tasks with a scheduled type + valid 6-field cron -> 200.
    // Scheduled tasks register in the scheduler without needing a live agent.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "name": "Nightly",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["task_type"], "scheduled");
    assert_eq!(body["data"]["name"], "Nightly");
    assert!(body["data"]["id"].is_string());
}

#[tokio::test]
async fn task_create_invalid_cron_422() {
    // Scheduled task with a malformed cron expression -> Validation 422.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "cron_expression": "not a cron"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn task_create_scheduled_missing_cron_422() {
    // Scheduled task without cron_expression -> Validation 422.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "scheduled"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn task_create_empty_server_ids_422() {
    // Empty server_ids -> Validation 422.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [],
            "task_type": "scheduled",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn task_create_blank_command_422() {
    // Whitespace-only command -> Validation 422.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "   ",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn task_create_invalid_retry_count_422() {
    // retry_count outside 0..=10 -> Validation 422.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "cron_expression": "0 0 * * * *",
            "retry_count": 99
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn task_oneshot_create_succeeds_without_agent() {
    // A oneshot task with no connected agent still creates the DB row and
    // returns 200; dispatch is best-effort and a dropped message is not an error.
    // NOTE: with no live agent connected, the command is never actually executed.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

    let resp = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "oneshot"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["task_type"], "oneshot");
}

#[tokio::test]
async fn task_crud_lifecycle_admin() {
    // Create -> get -> update -> list -> results -> delete, all as admin.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

    // Create a scheduled task.
    let created: Value = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "name": "Job",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = created["data"]["id"].as_str().unwrap().to_string();

    // Get it back.
    let got = admin
        .get(format!("{}/api/tasks/{}", base_url, task_id))
        .send()
        .await
        .unwrap();
    assert_eq!(got.status(), 200);
    let got_body: Value = got.json().await.unwrap();
    assert_eq!(got_body["data"]["name"], "Job");

    // Update name and re-validate cron.
    let updated = admin
        .put(format!("{}/api/tasks/{}", base_url, task_id))
        .json(&json!({ "name": "Renamed", "cron_expression": "0 30 * * * *" }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let updated_body: Value = updated.json().await.unwrap();
    assert_eq!(updated_body["data"]["name"], "Renamed");
    assert_eq!(updated_body["data"]["cron_expression"], "0 30 * * * *");

    // List shows at least the one task.
    let list = admin
        .get(format!("{}/api/tasks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let list_body: Value = list.json().await.unwrap();
    assert!(list_body["data"].as_array().unwrap().iter().any(|t| t["id"] == task_id));

    // Results are empty (no run happened) but the endpoint returns 200.
    let results = admin
        .get(format!("{}/api/tasks/{}/results", base_url, task_id))
        .send()
        .await
        .unwrap();
    assert_eq!(results.status(), 200);
    let results_body: Value = results.json().await.unwrap();
    assert!(results_body["data"].as_array().unwrap().is_empty());

    // Delete it.
    let deleted = admin
        .delete(format!("{}/api/tasks/{}", base_url, task_id))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);

    // Subsequent get is 404.
    let gone = admin
        .get(format!("{}/api/tasks/{}", base_url, task_id))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
}

#[tokio::test]
async fn task_update_invalid_cron_422() {
    // Updating a task with a bad cron expression -> Validation 422.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

    let created: Value = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo hi",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "cron_expression": "0 0 * * * *"
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
        .json(&json!({ "cron_expression": "garbage" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn task_get_not_found_404() {
    // GET /api/tasks/{id} for an unknown id -> 404.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/tasks/does-not-exist", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn task_run_oneshot_rejected_400() {
    // POST /api/tasks/{id}/run on a non-scheduled task -> BadRequest 400.
    // NOTE: scheduled-task run dispatches to a connected agent; with no agent
    // it would still flip last_run_at, so we exercise the reachable 400 branch.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-srv").await;

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
        .post(format!("{}/api/tasks/{}/run", base_url, task_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn task_run_not_found_404() {
    // POST /api/tasks/{id}/run for an unknown id -> 404.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/tasks/nope/run", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn task_routes_member_forbidden() {
    // The entire task router is admin-only -> member GET /api/tasks gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "task_member", "member").await;

    let resp = member
        .get(format!("{}/api/tasks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn task_routes_unauthenticated_401() {
    // No auth -> 401 on the admin-gated task list.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/tasks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ===========================================================================
// setting
// ===========================================================================

#[tokio::test]
async fn settings_get_and_update_admin() {
    // Admin GET /api/settings returns defaults; PUT persists; GET reflects it.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Defaults are null fields.
    let get1 = admin
        .get(format!("{}/api/settings", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(get1.status(), 200);
    let body1: Value = get1.json().await.unwrap();
    assert!(body1["data"]["site_name"].is_null());

    // Update.
    let put = admin
        .put(format!("{}/api/settings", base_url))
        .json(&json!({
            "site_name": "ServerBee Test",
            "site_description": "desc",
            "custom_css": ".a{color:red}",
            "custom_js": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 200);
    let put_body: Value = put.json().await.unwrap();
    assert_eq!(put_body["data"]["site_name"], "ServerBee Test");

    // Read back the persisted value.
    let get2 = admin
        .get(format!("{}/api/settings", base_url))
        .send()
        .await
        .unwrap();
    let body2: Value = get2.json().await.unwrap();
    assert_eq!(body2["data"]["site_description"], "desc");
}

#[tokio::test]
async fn settings_member_forbidden() {
    // /api/settings is admin-only -> member GET gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "settings_member", "member").await;

    let resp = member
        .get(format!("{}/api/settings", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn settings_unauthenticated_401() {
    // No auth -> 401 on /api/settings.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/settings", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn settings_restore_invalid_file_422() {
    // POST /api/settings/restore with a non-SQLite body -> Validation 422.
    // NOTE: a valid restore swaps the live DB and requires a restart, so we
    // only exercise the invalid-file rejection branch here.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/settings/restore", base_url))
        .body(b"this is definitely not a sqlite database file".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn settings_backup_member_forbidden() {
    // POST /api/settings/backup is admin-only -> member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "backup_member", "member").await;

    let resp = member
        .post(format!("{}/api/settings/backup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// ===========================================================================
// traffic
// ===========================================================================

#[tokio::test]
async fn traffic_overview_admin_happy_path() {
    // Admin GET /api/traffic/overview returns a (possibly empty) array.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/traffic/overview", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn traffic_overview_daily_admin_happy_path() {
    // Admin GET /api/traffic/overview/daily with a days param returns an array.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/traffic/overview/daily?days=7", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn traffic_per_server_admin_happy_path() {
    // Admin GET /api/servers/{id}/traffic for a seeded server returns a cycle.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "traffic-srv").await;

    let resp = admin
        .get(format!("{}/api/servers/{}/traffic", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    // Default billing cycle is monthly; cycle bounds + zeroed totals are present.
    assert_eq!(body["data"]["bytes_total"], 0);
    assert!(body["data"]["cycle_start"].is_string());
    assert!(body["data"]["daily"].is_array());
}

#[tokio::test]
async fn traffic_cycle_admin_happy_path() {
    // Admin GET /api/traffic/{server_id}/cycle returns current + history.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "traffic-srv").await;

    let resp = admin
        .get(format!("{}/api/traffic/{}/cycle?history=3", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["current"]["period"].is_string());
    assert!(body["data"]["history"].is_array());
}

#[tokio::test]
async fn traffic_read_accessible_to_member() {
    // traffic::read_router() is mounted in the authenticated (non-admin) block,
    // so a member can read the global overview.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "traffic_member", "member").await;

    let resp = member
        .get(format!("{}/api/traffic/overview", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn traffic_per_server_not_found_404() {
    // Unknown server id on per-server traffic -> 404 (ServerService::get_server).
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/servers/no-such-server/traffic", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn traffic_cycle_not_found_404() {
    // Unknown server id on cycle traffic -> 404.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/traffic/no-such-server/cycle", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn traffic_overview_unauthenticated_401() {
    // No auth -> 401 on the read-protected traffic overview.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/traffic/overview", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
