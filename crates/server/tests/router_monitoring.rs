//! Router-level integration tests for the monitoring API surface:
//! ping tasks, network probes, traceroute, and service monitors.
//!
//! Probe/traceroute *execution* dispatches to a live agent over WebSocket; no
//! agent is connected here, so those control-plane paths only cover auth,
//! validation, and not-found outcomes (see `// NOTE` comments).
mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// ping-tasks
// ---------------------------------------------------------------------------

// Admin can create a ping task, then list/get it; the wrapped data is returned.
#[tokio::test]
async fn ping_task_create_list_get_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/ping-tasks", base_url))
        .json(&json!({ "name": "cf-icmp", "probe_type": "icmp", "target": "1.1.1.1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "admin should create a ping task");
    let body: Value = resp.json().await.unwrap();
    let id = body["data"]["id"].as_str().expect("created id").to_string();
    assert_eq!(body["data"]["probe_type"].as_str(), Some("icmp"));

    // List includes the created task.
    let list: Value = admin
        .get(format!("{}/api/ping-tasks", base_url))
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
            .any(|t| t["id"].as_str() == Some(id.as_str()))
    );

    // Get by id returns the same record.
    let got = admin
        .get(format!("{}/api/ping-tasks/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(got.status(), 200);
    let got_body: Value = got.json().await.unwrap();
    assert_eq!(got_body["data"]["target"].as_str(), Some("1.1.1.1"));
}

// Admin can update then delete a ping task; deleting the same id twice 404s.
#[tokio::test]
async fn ping_task_update_and_delete() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let id = {
        let body: Value = admin
            .post(format!("{}/api/ping-tasks", base_url))
            .json(&json!({ "name": "to-edit", "probe_type": "tcp", "target": "1.1.1.1:53" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    let updated = admin
        .put(format!("{}/api/ping-tasks/{}", base_url, id))
        .json(&json!({ "name": "renamed", "interval": 120 }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let ubody: Value = updated.json().await.unwrap();
    assert_eq!(ubody["data"]["name"].as_str(), Some("renamed"));
    assert_eq!(ubody["data"]["interval"].as_i64(), Some(120));

    let del = admin
        .delete(format!("{}/api/ping-tasks/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);

    // Second delete on a now-missing id is 404.
    let del2 = admin
        .delete(format!("{}/api/ping-tasks/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del2.status(), 404);
}

// Invalid probe_type fails the service-level Validation check -> 422.
#[tokio::test]
async fn ping_task_create_invalid_probe_type_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/ping-tasks", base_url))
        .json(&json!({ "name": "bad", "probe_type": "udp", "target": "1.1.1.1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "unknown probe_type is a validation error");
}

// Getting a non-existent ping task id returns 404.
#[tokio::test]
async fn ping_task_get_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/ping-tasks/does-not-exist", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// The records endpoint requires `from`/`to`; omitting them is a 400 query rejection.
#[tokio::test]
async fn ping_task_records_missing_query_params_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/ping-tasks/some-id/records", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "missing required from/to query params");
}

// With a valid time range and no data, records returns an empty array (200).
#[tokio::test]
async fn ping_task_records_empty_with_valid_range() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!(
            "{}/api/ping-tasks/some-id/records?from=2026-01-01T00:00:00Z&to=2026-01-02T00:00:00Z",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"].as_array().map(|a| a.len()), Some(0));
}

// A member (read-only) may list ping tasks but is forbidden from creating them.
#[tokio::test]
async fn ping_task_member_read_ok_write_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "ping_member", "member").await;

    // Authenticated read works for members.
    let list = member
        .get(format!("{}/api/ping-tasks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);

    // Write is admin-only -> 403 for members.
    let create = member
        .post(format!("{}/api/ping-tasks", base_url))
        .json(&json!({ "name": "x", "probe_type": "icmp", "target": "1.1.1.1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), 403);
}

// Unauthenticated requests to a protected read route are rejected with 401.
#[tokio::test]
async fn ping_task_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/ping-tasks", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// network-probes
// ---------------------------------------------------------------------------

// Admin can create a custom probe target and read it back via the list.
#[tokio::test]
async fn network_probe_target_create_and_list() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/network-probes/targets", base_url))
        .json(&json!({
            "name": "custom-dns",
            "provider": "Cloudflare",
            "location": "Global",
            "target": "1.1.1.1",
            "probe_type": "icmp"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let id = body["data"]["id"].as_str().unwrap().to_string();

    let list: Value = admin
        .get(format!("{}/api/network-probes/targets", base_url))
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
            .any(|t| t["id"].as_str() == Some(id.as_str())),
        "created custom target appears in the merged preset+custom list"
    );
}

// Admin can update then delete a custom probe target.
#[tokio::test]
async fn network_probe_target_update_and_delete() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let id = {
        let body: Value = admin
            .post(format!("{}/api/network-probes/targets", base_url))
            .json(&json!({
                "name": "edit-me", "provider": "P", "location": "L",
                "target": "8.8.8.8", "probe_type": "tcp"
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    let updated = admin
        .put(format!("{}/api/network-probes/targets/{}", base_url, id))
        .json(&json!({ "name": "edited" }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let ubody: Value = updated.json().await.unwrap();
    assert_eq!(ubody["data"]["name"].as_str(), Some("edited"));

    let del = admin
        .delete(format!("{}/api/network-probes/targets/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);
}

// Invalid probe_type on a custom target is a validation error -> 422.
#[tokio::test]
async fn network_probe_target_invalid_probe_type_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/network-probes/targets", base_url))
        .json(&json!({
            "name": "bad", "provider": "P", "location": "L",
            "target": "8.8.8.8", "probe_type": "invalid"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

// Updating a non-existent custom target returns 404.
#[tokio::test]
async fn network_probe_target_update_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/network-probes/targets/nope-id", base_url))
        .json(&json!({ "name": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// The global setting endpoint returns the defaults when nothing is configured.
#[tokio::test]
async fn network_probe_setting_get_defaults() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/network-probes/setting", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["interval"].as_u64(), Some(60));
    assert_eq!(body["data"]["packet_count"].as_u64(), Some(10));
}

// Admin can update the global setting with valid values.
#[tokio::test]
async fn network_probe_setting_update_valid() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/network-probes/setting", base_url))
        .json(&json!({ "interval": 120, "packet_count": 8, "default_target_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["interval"].as_u64(), Some(120));
}

// An out-of-range interval fails the BadRequest validation -> 400.
#[tokio::test]
async fn network_probe_setting_update_out_of_range_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/network-probes/setting", base_url))
        .json(&json!({ "interval": 10, "packet_count": 8, "default_target_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "interval below 30 is a BadRequest");
}

// The overview endpoint returns an array; with a seeded (offline) server present.
#[tokio::test]
async fn network_probe_overview_includes_seeded_server() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "np-overview-srv").await;

    let resp = admin
        .get(format!("{}/api/network-probes/overview", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let arr = body["data"].as_array().unwrap();
    assert!(
        arr.iter().any(|o| o["server_id"].as_str() == Some(server_id.as_str())),
        "overview lists all servers including the offline seeded one"
    );
}

// Per-server probe targets read returns an empty list for a server with no config.
#[tokio::test]
async fn network_probe_server_targets_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "np-targets-srv").await;

    let resp = admin
        .get(format!(
            "{}/api/servers/{}/network-probes/targets",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"].as_array().map(|a| a.len()), Some(0));
}

// Per-server probe records read returns an empty list with a valid time range.
#[tokio::test]
async fn network_probe_server_records_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "np-records-srv").await;

    let resp = admin
        .get(format!(
            "{}/api/servers/{}/network-probes/records?from=2026-01-01T00:00:00Z&to=2026-01-01T12:00:00Z",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"].as_array().map(|a| a.len()), Some(0));
}

// Per-server probe summary resolves the server row and returns its name (offline).
#[tokio::test]
async fn network_probe_server_summary_ok() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "np-summary-srv").await;

    let resp = admin
        .get(format!(
            "{}/api/servers/{}/network-probes/summary",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["server_id"].as_str(), Some(server_id.as_str()));
    assert_eq!(body["data"]["online"].as_bool(), Some(false));
}

// Summary for an unknown server id returns 404 (server row lookup fails).
#[tokio::test]
async fn network_probe_server_summary_missing_server_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!(
            "{}/api/servers/ghost/network-probes/summary",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// A member can read the probe setting but cannot mutate targets (admin-only write).
#[tokio::test]
async fn network_probe_member_read_ok_write_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "np_member", "member").await;

    let read = member
        .get(format!("{}/api/network-probes/setting", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200);

    let write = member
        .post(format!("{}/api/network-probes/targets", base_url))
        .json(&json!({
            "name": "x", "provider": "P", "location": "L",
            "target": "8.8.8.8", "probe_type": "icmp"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(write.status(), 403);
}

// Unauthenticated access to a probe read route is rejected with 401.
#[tokio::test]
async fn network_probe_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/network-probes/targets", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// traceroute
// ---------------------------------------------------------------------------

// Listing traceroute records for a server with no history returns an empty array.
#[tokio::test]
async fn traceroute_list_records_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-list-srv").await;

    let resp = admin
        .get(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"].as_array().map(|a| a.len()), Some(0));
}

// Fetching a traceroute snapshot that exists in neither cache nor DB returns 404.
#[tokio::test]
async fn traceroute_snapshot_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-snap-srv").await;

    let resp = admin
        .get(format!(
            "{}/api/servers/{}/traceroute/no-such-request",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// Clearing history on a server with no records reports zero deleted (200).
#[tokio::test]
async fn traceroute_clear_history_zero_deleted() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-clear-srv").await;

    let resp = admin
        .delete(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["deleted"].as_u64(), Some(0));
}

// An empty target is rejected by the handler's char-allowlist guard -> 422.
#[tokio::test]
async fn traceroute_trigger_empty_target_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-empty-srv").await;

    let resp = admin
        .post(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .json(&json!({ "target": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "empty target fails validation");
}

// A target with disallowed characters is rejected before any agent dispatch -> 422.
#[tokio::test]
async fn traceroute_trigger_invalid_chars_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-chars-srv").await;

    let resp = admin
        .post(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .json(&json!({ "target": "bad target/with space" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

// NOTE: triggering a traceroute with a valid target dispatches to a live agent
// over WebSocket. No agent is connected, so `get_sender` is None and the handler
// returns NotFound ("Server is not online") -> 404. The 200 dispatch path cannot
// be exercised without a connected agent.
#[tokio::test]
async fn traceroute_trigger_valid_target_no_agent_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-offline-srv").await;

    let resp = admin
        .post(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .json(&json!({ "target": "1.1.1.1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "no online agent to dispatch to");
}

// A member can read traceroute history but cannot trigger a run (admin-only write).
#[tokio::test]
async fn traceroute_member_read_ok_write_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-member-srv").await;
    let member = login_as_new_user(&admin, &base_url, "tr_member", "member").await;

    let read = member
        .get(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200);

    let write = member
        .post(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .json(&json!({ "target": "1.1.1.1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(write.status(), 403);
}

// Unauthenticated access to the traceroute history route is rejected with 401.
#[tokio::test]
async fn traceroute_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/servers/any/traceroute", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ---------------------------------------------------------------------------
// service-monitors
// ---------------------------------------------------------------------------

// Admin can create an SSL monitor, then list and get it.
#[tokio::test]
async fn service_monitor_create_list_get() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/service-monitors", base_url))
        .json(&json!({ "name": "ssl-mon", "monitor_type": "ssl", "target": "example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let id = body["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["monitor_type"].as_str(), Some("ssl"));

    let list: Value = admin
        .get(format!("{}/api/service-monitors", base_url))
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
            .any(|m| m["id"].as_str() == Some(id.as_str()))
    );

    // Get returns the monitor flattened with its (null) latest record.
    let got = admin
        .get(format!("{}/api/service-monitors/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(got.status(), 200);
    let got_body: Value = got.json().await.unwrap();
    assert_eq!(got_body["data"]["target"].as_str(), Some("example.com"));
    assert!(got_body["data"]["latest_record"].is_null());
}

// Admin can update then delete a service monitor.
#[tokio::test]
async fn service_monitor_update_and_delete() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let id = {
        let body: Value = admin
            .post(format!("{}/api/service-monitors", base_url))
            .json(&json!({ "name": "tcp-mon", "monitor_type": "tcp", "target": "8.8.8.8:53" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    let updated = admin
        .put(format!("{}/api/service-monitors/{}", base_url, id))
        .json(&json!({ "name": "tcp-renamed", "interval": 600, "enabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), 200);
    let ubody: Value = updated.json().await.unwrap();
    assert_eq!(ubody["data"]["name"].as_str(), Some("tcp-renamed"));
    assert_eq!(ubody["data"]["interval"].as_i64(), Some(600));
    assert_eq!(ubody["data"]["enabled"].as_bool(), Some(false));

    let del = admin
        .delete(format!("{}/api/service-monitors/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);
}

// An unknown monitor_type is a validation error -> 422.
#[tokio::test]
async fn service_monitor_create_invalid_type_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/service-monitors", base_url))
        .json(&json!({ "name": "bad", "monitor_type": "ftp", "target": "example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

// A loopback target for a connect-based type is rejected at create time -> 422.
#[tokio::test]
async fn service_monitor_create_loopback_target_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/service-monitors", base_url))
        .json(&json!({ "name": "loop", "monitor_type": "tcp", "target": "127.0.0.1:80" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "loopback targets are blocked for monitoring");
}

// Getting a non-existent monitor returns 404.
#[tokio::test]
async fn service_monitor_get_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .get(format!("{}/api/service-monitors/ghost", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// The records endpoint accepts optional params and returns an empty array.
#[tokio::test]
async fn service_monitor_records_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let id = {
        let body: Value = admin
            .post(format!("{}/api/service-monitors", base_url))
            .json(&json!({ "name": "rec-mon", "monitor_type": "ssl", "target": "example.com" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    let resp = admin
        .get(format!("{}/api/service-monitors/{}/records", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"].as_array().map(|a| a.len()), Some(0));
}

// Triggering a check runs the server-side checker (no agent needed) and persists
// a record. The DNS check against a public hostname succeeds, so this asserts a
// 200 with a record id; the check outcome itself is environment-dependent so we
// only assert the record was created and stored.
#[tokio::test]
async fn service_monitor_trigger_check_creates_record() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let id = {
        let body: Value = admin
            .post(format!("{}/api/service-monitors", base_url))
            .json(&json!({
                "name": "dns-mon",
                "monitor_type": "dns",
                "target": "example.com",
                "config_json": { "record_type": "A" }
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    let resp = admin
        .post(format!("{}/api/service-monitors/{}/check", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "server-side check runs without an agent");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["monitor_id"].as_str(), Some(id.as_str()));

    // The check result is now retrievable as the latest record.
    let records = admin
        .get(format!("{}/api/service-monitors/{}/records", base_url, id))
        .send()
        .await
        .unwrap();
    let rbody: Value = records.json().await.unwrap();
    assert_eq!(rbody["data"].as_array().map(|a| a.len()), Some(1));
}

// Triggering a check on a non-existent monitor returns 404.
#[tokio::test]
async fn service_monitor_trigger_check_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/service-monitors/ghost/check", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// A member can list monitors but cannot create them (admin-only write).
#[tokio::test]
async fn service_monitor_member_read_ok_write_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "sm_member", "member").await;

    let read = member
        .get(format!("{}/api/service-monitors", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200);

    let write = member
        .post(format!("{}/api/service-monitors", base_url))
        .json(&json!({ "name": "x", "monitor_type": "ssl", "target": "example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(write.status(), 403);

    // Triggering a check is also an admin-only write route.
    let check = member
        .post(format!("{}/api/service-monitors/any/check", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(check.status(), 403);
}

// Unauthenticated access to a service-monitor read route is rejected with 401.
#[tokio::test]
async fn service_monitor_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/service-monitors", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
