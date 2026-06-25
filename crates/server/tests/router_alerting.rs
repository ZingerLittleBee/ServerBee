//! Router-level integration tests for the alerting surface:
//! alert rules, alert events, notifications + groups, incidents, maintenances.
//!
//! Status-code expectations follow `crate::error::AppError`'s `IntoResponse`
//! mapping: Validation → 422, BadRequest → 400, NotFound → 404,
//! Unauthorized → 401, Forbidden → 403. Axum's `Json` extractor also returns
//! 422 when a syntactically-valid body fails to deserialize into the DTO.
mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ───────────────────────── Alert Rules (admin-only) ─────────────────────────

// Happy path: admin creates an alert rule, then reads it back by id.
#[tokio::test]
async fn create_and_get_alert_rule() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "high-cpu",
            "rules": [{ "rule_type": "cpu", "min": 90.0 }],
            "trigger_mode": "always",
            "cover_type": "all",
            "enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "admin should create alert rule");
    let body: Value = resp.json().await.unwrap();
    let id = body["data"]["id"].as_str().expect("rule id").to_string();
    assert_eq!(body["data"]["name"].as_str(), Some("high-cpu"));

    // GET by id returns the same rule.
    let got = client
        .get(format!("{}/api/alert-rules/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(got.status(), 200);
    let got_body: Value = got.json().await.unwrap();
    assert_eq!(got_body["data"]["id"].as_str(), Some(id.as_str()));
}

// Full lifecycle: list, update, delete an alert rule (admin).
#[tokio::test]
async fn update_and_delete_alert_rule() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({ "name": "mem", "rules": [{ "rule_type": "memory", "min": 1.0 }] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();

    // List contains the new rule.
    let list: Value = client
        .get(format!("{}/api/alert-rules", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list["data"].as_array().unwrap().iter().any(|r| r["id"] == id));

    // Update renames + disables it.
    let updated = client
        .put(format!("{}/api/alert-rules/{}", base_url, id))
        .json(&json!({ "name": "mem-renamed", "enabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let updated_body: Value = updated.json().await.unwrap();
    assert_eq!(updated_body["data"]["name"].as_str(), Some("mem-renamed"));
    assert_eq!(updated_body["data"]["enabled"].as_bool(), Some(false));

    // Delete succeeds, then the rule is gone (404 on subsequent GET).
    let del = client
        .delete(format!("{}/api/alert-rules/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);
    let gone = client
        .get(format!("{}/api/alert-rules/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404, "deleted rule should be 404");
}

// Validation: an invalid cover_type is rejected with 422 (AppError::Validation).
#[tokio::test]
async fn create_alert_rule_invalid_cover_type_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({ "name": "bad", "rules": [{ "rule_type": "cpu", "min": 1.0 }], "cover_type": "bogus" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "invalid cover_type → Validation/422");
}

// Validation: mixing a security rule type with a metric item → 400 (AppError::BadRequest).
#[tokio::test]
async fn create_alert_rule_mixed_security_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "mixed",
            "rules": [
                { "rule_type": "ssh_brute_force_detected" },
                { "rule_type": "cpu", "min": 90.0 }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "mixing security + metric → BadRequest/400");
}

// Not found: GET on an unknown alert-rule id returns 404.
#[tokio::test]
async fn get_alert_rule_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/alert-rules/does-not-exist", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// AuthZ: a member cannot create an alert rule (admin-only write) → 403.
#[tokio::test]
async fn member_cannot_create_alert_rule() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "alert-member", "member").await;

    let resp = member
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({ "name": "x", "rules": [{ "rule_type": "cpu", "min": 1.0 }] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "member must not create alert rules");
}

// Unauthenticated: no session cookie on a protected route → 401.
#[tokio::test]
async fn unauthenticated_list_alert_rules_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/alert-rules", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "no session → 401");
}

// Alert-rule states list is reachable by admin and empty for a fresh rule.
#[tokio::test]
async fn list_alert_rule_states_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({ "name": "s", "rules": [{ "rule_type": "cpu", "min": 1.0 }] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap();

    let resp = client
        .get(format!("{}/api/alert-rules/{}/states", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].as_array().unwrap().is_empty(), "no states yet");
}

// ───────────────────────── Alert Events (read-only) ─────────────────────────

// Read route: alert-events list works for members (authenticated, non-admin).
#[tokio::test]
async fn member_can_list_alert_events() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "events-member", "member").await;

    let resp = member
        .get(format!("{}/api/alert-events", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "members may read alert events");
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].as_array().unwrap().is_empty(), "no events yet");
}

// Validation: an alert_key without a ':' separator → 400 (AppError::BadRequest).
#[tokio::test]
async fn alert_event_detail_bad_key_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/alert-events/no-colon-here", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "alert_key without ':' → BadRequest/400");
}

// Not found: a well-formed alert_key with no matching state → 404.
#[tokio::test]
async fn alert_event_detail_missing_state_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/alert-events/rule-x:server-y", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "no alert_state row → 404");
}

// Unauthenticated: alert-events read route still requires a session → 401.
#[tokio::test]
async fn unauthenticated_alert_events_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/alert-events", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ───────────────────────── Notifications (admin-only) ─────────────────────────

// Happy path: admin creates, reads, updates, and deletes a webhook notification.
#[tokio::test]
async fn notification_crud_lifecycle() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create a valid webhook channel.
    let created = client
        .post(format!("{}/api/notifications", base_url))
        .json(&json!({
            "name": "ops-hook",
            "notify_type": "webhook",
            "config_json": { "url": "https://example.com/hook", "method": "POST" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);
    let created_body: Value = created.json().await.unwrap();
    let id = created_body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created_body["data"]["notify_type"].as_str(), Some("webhook"));

    // Get by id.
    let got = client
        .get(format!("{}/api/notifications/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(got.status(), 200);

    // Update: rename + disable.
    let updated = client
        .put(format!("{}/api/notifications/{}", base_url, id))
        .json(&json!({ "name": "ops-hook-2", "enabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let updated_body: Value = updated.json().await.unwrap();
    assert_eq!(updated_body["data"]["name"].as_str(), Some("ops-hook-2"));
    assert_eq!(updated_body["data"]["enabled"].as_bool(), Some(false));

    // Delete, then 404 on subsequent get.
    let del = client
        .delete(format!("{}/api/notifications/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);
    let gone = client
        .get(format!("{}/api/notifications/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
}

// Validation: a webhook config missing the required `url` → 422 (Validation).
#[tokio::test]
async fn create_notification_invalid_config_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/notifications", base_url))
        .json(&json!({
            "name": "broken",
            "notify_type": "webhook",
            "config_json": { "method": "GET" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "webhook without url → Validation/422");
}

// Validation: an email channel with an empty `to` list → 422 (Validation).
#[tokio::test]
async fn create_notification_email_empty_to_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/notifications", base_url))
        .json(&json!({
            "name": "no-recipients",
            "notify_type": "email",
            "config_json": { "from": "alerts@example.com", "to": [] }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "empty email to[] → Validation/422");
}

// Not found: GET on an unknown notification id → 404.
#[tokio::test]
async fn get_notification_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/notifications/ghost", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// Test endpoint: POST /test on a missing channel id → 404 (no real send attempted).
// NOTE: a successful test send dispatches to an external provider; only the
// not-found branch is asserted to avoid real network I/O.
#[tokio::test]
async fn test_notification_missing_id_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/notifications/ghost/test", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "test on missing channel → 404");
}

// AuthZ: a member cannot create a notification → 403.
#[tokio::test]
async fn member_cannot_create_notification() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "notif-member", "member").await;

    let resp = member
        .post(format!("{}/api/notifications", base_url))
        .json(&json!({
            "name": "x",
            "notify_type": "webhook",
            "config_json": { "url": "https://example.com" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// Unauthenticated: listing notifications without a session → 401.
#[tokio::test]
async fn unauthenticated_list_notifications_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/notifications", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ─────────────────────── Notification Groups (admin-only) ───────────────────────

// Happy path: create, get, update, delete a notification group.
#[tokio::test]
async fn notification_group_crud_lifecycle() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create a channel first so the group can reference it.
    let channel: Value = client
        .post(format!("{}/api/notifications", base_url))
        .json(&json!({
            "name": "grp-hook",
            "notify_type": "webhook",
            "config_json": { "url": "https://example.com/hook" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let channel_id = channel["data"]["id"].as_str().unwrap().to_string();

    let created = client
        .post(format!("{}/api/notification-groups", base_url))
        .json(&json!({ "name": "critical", "notification_ids": [channel_id] }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);
    let created_body: Value = created.json().await.unwrap();
    let group_id = created_body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created_body["data"]["name"].as_str(), Some("critical"));

    // Get by id.
    let got = client
        .get(format!("{}/api/notification-groups/{}", base_url, group_id))
        .send()
        .await
        .unwrap();
    assert_eq!(got.status(), 200);

    // Update name + ids.
    let updated = client
        .put(format!("{}/api/notification-groups/{}", base_url, group_id))
        .json(&json!({ "name": "critical-2", "notification_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let updated_body: Value = updated.json().await.unwrap();
    assert_eq!(updated_body["data"]["name"].as_str(), Some("critical-2"));

    // Delete, then 404.
    let del = client
        .delete(format!("{}/api/notification-groups/{}", base_url, group_id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);
    let gone = client
        .get(format!("{}/api/notification-groups/{}", base_url, group_id))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
}

// Not found: GET on an unknown notification-group id → 404.
#[tokio::test]
async fn get_notification_group_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/notification-groups/nope", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// AuthZ: a member cannot create a notification group → 403.
#[tokio::test]
async fn member_cannot_create_notification_group() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "grp-member", "member").await;

    let resp = member
        .post(format!("{}/api/notification-groups", base_url))
        .json(&json!({ "name": "x", "notification_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// ───────────────────────── Incidents (admin-only) ─────────────────────────

// Happy path: create an incident, update it, add an update, then delete it.
#[tokio::test]
async fn incident_crud_lifecycle() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    // A real server id makes the incident's server scoping realistic.
    let server_id = create_server(&client, &base_url, "incident-srv").await;

    let created = client
        .post(format!("{}/api/incidents", base_url))
        .json(&json!({
            "title": "API outage",
            "status": "investigating",
            "severity": "major",
            "server_ids_json": [server_id],
            "is_public": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);
    let created_body: Value = created.json().await.unwrap();
    let id = created_body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created_body["data"]["status"].as_str(), Some("investigating"));

    // List filtered by status returns the created incident.
    let list: Value = client
        .get(format!("{}/api/incidents?status=investigating", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list["data"].as_array().unwrap().iter().any(|i| i["id"] == id));

    // Update the incident severity.
    let updated = client
        .put(format!("{}/api/incidents/{}", base_url, id))
        .json(&json!({ "severity": "critical" }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let updated_body: Value = updated.json().await.unwrap();
    assert_eq!(updated_body["data"]["severity"].as_str(), Some("critical"));

    // Add an update; the incident status should sync to "identified".
    let upd = client
        .post(format!("{}/api/incidents/{}/updates", base_url, id))
        .json(&json!({ "status": "identified", "message": "root cause found" }))
        .send()
        .await
        .unwrap();
    assert_eq!(upd.status(), 200);
    let upd_body: Value = upd.json().await.unwrap();
    assert_eq!(upd_body["data"]["status"].as_str(), Some("identified"));
    assert_eq!(upd_body["data"]["incident_id"].as_str(), Some(id.as_str()));

    // Delete the incident.
    let del = client
        .delete(format!("{}/api/incidents/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);
}

// Validation: an invalid status on create → 422 (AppError::Validation).
#[tokio::test]
async fn create_incident_invalid_status_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/incidents", base_url))
        .json(&json!({ "title": "bad", "status": "bogus" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "invalid incident status → Validation/422");
}

// Validation: an invalid severity on create → 422.
#[tokio::test]
async fn create_incident_invalid_severity_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/incidents", base_url))
        .json(&json!({ "title": "bad", "severity": "apocalyptic" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

// Not found: updating an unknown incident id → 404.
#[tokio::test]
async fn update_incident_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .put(format!("{}/api/incidents/missing", base_url))
        .json(&json!({ "title": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// Not found: adding an update to an unknown incident id → 404.
#[tokio::test]
async fn add_incident_update_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/incidents/missing/updates", base_url))
        .json(&json!({ "status": "investigating", "message": "m" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// AuthZ: a member cannot create an incident → 403.
#[tokio::test]
async fn member_cannot_create_incident() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "incident-member", "member").await;

    let resp = member
        .post(format!("{}/api/incidents", base_url))
        .json(&json!({ "title": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// Unauthenticated: listing incidents without a session → 401.
#[tokio::test]
async fn unauthenticated_list_incidents_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/incidents", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ───────────────────────── Maintenances (admin-only) ─────────────────────────

// Happy path: create a maintenance window, list it, update it, then delete it.
#[tokio::test]
async fn maintenance_crud_lifecycle() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created = client
        .post(format!("{}/api/maintenances", base_url))
        .json(&json!({
            "title": "DB upgrade",
            "description": "rolling restart",
            "start_at": "2026-01-01T00:00:00Z",
            "end_at": "2026-01-01T02:00:00Z",
            "is_public": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);
    let created_body: Value = created.json().await.unwrap();
    let id = created_body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created_body["data"]["title"].as_str(), Some("DB upgrade"));
    assert_eq!(created_body["data"]["active"].as_bool(), Some(true));

    // List contains the new window.
    let list: Value = client
        .get(format!("{}/api/maintenances", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list["data"].as_array().unwrap().iter().any(|m| m["id"] == id));

    // Update: rename + deactivate.
    let updated = client
        .put(format!("{}/api/maintenances/{}", base_url, id))
        .json(&json!({ "title": "DB upgrade v2", "active": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let updated_body: Value = updated.json().await.unwrap();
    assert_eq!(updated_body["data"]["title"].as_str(), Some("DB upgrade v2"));
    assert_eq!(updated_body["data"]["active"].as_bool(), Some(false));

    // Delete, then 404 on update of the gone id.
    let del = client
        .delete(format!("{}/api/maintenances/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);
    let gone = client
        .put(format!("{}/api/maintenances/{}", base_url, id))
        .json(&json!({ "title": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
}

// Validation: end_at <= start_at → 422 (AppError::Validation).
#[tokio::test]
async fn create_maintenance_bad_window_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/maintenances", base_url))
        .json(&json!({
            "title": "bad window",
            "start_at": "2026-01-01T02:00:00Z",
            "end_at": "2026-01-01T00:00:00Z"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "end_at <= start_at → Validation/422");
}

// Not found: deleting an unknown maintenance id → 404.
#[tokio::test]
async fn delete_maintenance_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .delete(format!("{}/api/maintenances/missing", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// AuthZ: a member cannot create a maintenance window → 403.
#[tokio::test]
async fn member_cannot_create_maintenance() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "maint-member", "member").await;

    let resp = member
        .post(format!("{}/api/maintenances", base_url))
        .json(&json!({
            "title": "x",
            "start_at": "2026-01-01T00:00:00Z",
            "end_at": "2026-01-01T01:00:00Z"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// Unauthenticated: listing maintenances without a session → 401.
#[tokio::test]
async fn unauthenticated_list_maintenances_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/maintenances", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
