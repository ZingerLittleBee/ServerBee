//! Router-level integration tests for assorted offline router branches that are
//! NOT already exercised by `router_alerting.rs` / `router_monitoring.rs`:
//!
//! - OAuth: public providers list, authorize for a disabled/unknown provider,
//!   callback with missing/invalid state.
//! - Network probes: setting `packet_count` range guard, target-update
//!   `probe_type` validation, deleting a preset / missing target.
//! - Traceroute: deleting a missing record, snapshot cross-server isolation.
//! - Alert rules: action validation branches and `update`-path validation.
//!
//! Status-code expectations follow `crate::error::AppError`'s `IntoResponse`
//! mapping: BadRequest → 400, Validation → 422, Unauthorized → 401,
//! Forbidden → 403, NotFound → 404. Axum's `Query`/`Json` extractors return a
//! 4xx (400 for query, 422 for body) when required fields are missing.
//!
//! NOTE: The full OAuth success flow (authorize → provider → callback) requires
//! a live external IdP and a valid PKCE/state round-trip, which cannot be
//! reproduced deterministically here. The default test config has no OAuth
//! provider configured, so only the disabled-provider and bad-state error
//! branches are asserted. The traceroute snapshot DB-hit / cache-hit success
//! paths and the network-probe agent sync side effects require a connected
//! agent and a recorded run; those are covered for their auth/validation/
//! not-found outcomes only.
mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ===========================================================================
// OAuth (public, no auth required)
// ===========================================================================

// The providers list is a public route (merged before the auth layer) and, with
// no provider configured in the default test config, returns an empty array.
#[tokio::test]
async fn oauth_providers_list_public_and_empty() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/auth/oauth/providers", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "providers list is public");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["providers"].as_array().map(|a| a.len()),
        Some(0),
        "no provider configured in the default test config"
    );
}

// Authorize for a known-but-unconfigured provider → 400 (BadRequest:
// "OAuth provider 'github' is not configured").
#[tokio::test]
async fn oauth_authorize_disabled_provider_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/auth/oauth/github", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        400,
        "authorize on an unconfigured provider is rejected before any redirect"
    );
}

// Authorize for an entirely unknown provider name → 400 (is_configured → false).
#[tokio::test]
async fn oauth_authorize_unknown_provider_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/auth/oauth/totally-made-up", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// Callback with a code+state that match no stored flow state → 400
// (validate_and_consume_state: "Invalid or expired OAuth state"). The state map
// is empty because no authorize step populated it.
#[tokio::test]
async fn oauth_callback_unknown_state_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!(
            "{}/api/auth/oauth/github/callback?code=abc&state=never-issued",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        400,
        "callback with an unknown state is a BadRequest before any token exchange"
    );
}

// Callback missing the required `state` query param → 400 (axum Query rejection).
#[tokio::test]
async fn oauth_callback_missing_state_param_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!(
            "{}/api/auth/oauth/github/callback?code=abc",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        400,
        "missing required `state` query param is rejected by the Query extractor"
    );
}

// Callback missing the required `code` query param → 400 (axum Query rejection).
#[tokio::test]
async fn oauth_callback_missing_code_param_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!(
            "{}/api/auth/oauth/github/callback?state=xyz",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// ===========================================================================
// Network probes — gaps not covered by router_monitoring.rs
// ===========================================================================

// The setting update guards `packet_count` to [5, 20]; an out-of-range value is
// a BadRequest → 400 (distinct from the already-tested interval guard).
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
    assert_eq!(resp.status(), 400, "packet_count above 20 is a BadRequest");
}

// Updating an existing custom target with an invalid `probe_type` is a
// Validation error → 422 (the update path re-validates probe_type).
#[tokio::test]
async fn network_probe_update_target_invalid_probe_type_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Seed a valid custom target first.
    let id = {
        let body: Value = admin
            .post(format!("{}/api/network-probes/targets", base_url))
            .json(&json!({
                "name": "upd-bad-type", "provider": "P", "location": "L",
                "target": "8.8.8.8", "probe_type": "icmp"
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
        .put(format!("{}/api/network-probes/targets/{}", base_url, id))
        .json(&json!({ "probe_type": "udp" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "udp is not a valid probe_type on update");
}

// Deleting a non-existent custom target id → 404 (existence check in the service).
#[tokio::test]
async fn network_probe_delete_target_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .delete(format!("{}/api/network-probes/targets/ghost-target", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// A member (read-only) cannot update the global probe setting → 403.
#[tokio::test]
async fn network_probe_setting_update_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "np_setting_member", "member").await;

    let resp = member
        .put(format!("{}/api/network-probes/setting", base_url))
        .json(&json!({ "interval": 60, "packet_count": 10, "default_target_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "setting update is admin-only");
}

// The per-server overview/setting reads are still gated by auth → 401 for anon.
#[tokio::test]
async fn network_probe_overview_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{}/api/network-probes/overview", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ===========================================================================
// Traceroute — gaps not covered by router_monitoring.rs
// ===========================================================================

// Deleting a traceroute record that does not exist → 404
// (service `delete_record` returns NotFound when rows_affected == 0).
#[tokio::test]
async fn traceroute_delete_missing_record_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-del-srv").await;

    let resp = admin
        .delete(format!(
            "{}/api/servers/{}/traceroute/no-such-request",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "deleting an unknown record is a 404");
}

// A member can list traceroute history but cannot delete a record (admin write).
#[tokio::test]
async fn traceroute_delete_record_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-del-member-srv").await;
    let member = login_as_new_user(&admin, &base_url, "tr_del_member", "member").await;

    let resp = member
        .delete(format!(
            "{}/api/servers/{}/traceroute/whatever",
            base_url, server_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "deleting a record is admin-only");
}

// A member cannot clear the whole traceroute history for a server → 403.
#[tokio::test]
async fn traceroute_clear_history_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "tr-clear-member-srv").await;
    let member = login_as_new_user(&admin, &base_url, "tr_clear_member", "member").await;

    let resp = member
        .delete(format!("{}/api/servers/{}/traceroute", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// Unauthenticated trigger (write) on a traceroute run is rejected at the auth
// layer with 401 — before the target-validation / agent-dispatch logic runs.
#[tokio::test]
async fn traceroute_trigger_unauthenticated_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .post(format!("{}/api/servers/any/traceroute", base_url))
        .json(&json!({ "target": "1.1.1.1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ===========================================================================
// Alert rules — action / update validation branches
// ===========================================================================

// `validate_actions` rejects more than one action per rule with Validation → 422.
#[tokio::test]
async fn alert_rule_multiple_actions_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "two-actions",
            "rules": [{ "rule_type": "ssh_brute_force_detected" }],
            "actions": [
                { "type": "block_source_ip" },
                { "type": "block_source_ip" }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "at most one action per alert_rule");
}

// `block_source_ip` is only valid on source-ip-bearing security rules; attaching
// it to a metric rule is a Validation error → 422.
#[tokio::test]
async fn alert_rule_block_action_on_metric_rule_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "bad-action-target",
            "rules": [{ "rule_type": "cpu", "min": 90.0 }],
            "actions": [{ "type": "block_source_ip" }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        422,
        "block_source_ip is only allowed on ssh_brute_force_detected / port_scan_detected"
    );
}

// `block_source_ip` with an invalid `cover_type` is a Validation error → 422,
// even when the underlying rule type is a valid source-ip security rule.
#[tokio::test]
async fn alert_rule_block_action_invalid_cover_type_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "bad-action-cover",
            "rules": [{ "rule_type": "ssh_brute_force_detected" }],
            "actions": [{ "type": "block_source_ip", "cover_type": "bogus" }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "invalid action cover_type is rejected");
}

// Happy path for the action branch: a single `block_source_ip` action on a valid
// security rule is accepted (200) and the rule is persisted with the action.
#[tokio::test]
async fn alert_rule_valid_block_action_is_created() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!("{}/api/alert-rules", base_url))
        .json(&json!({
            "name": "auto-block",
            "rules": [{ "rule_type": "ssh_brute_force_detected" }],
            "actions": [{ "type": "block_source_ip", "cover_type": "all" }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "valid block_source_ip action is accepted");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["name"].as_str(), Some("auto-block"));
}

// Updating a rule with an invalid `cover_type` re-runs validation → 422.
#[tokio::test]
async fn alert_rule_update_invalid_cover_type_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Seed a valid rule.
    let id = {
        let body: Value = admin
            .post(format!("{}/api/alert-rules", base_url))
            .json(&json!({ "name": "upd-cover", "rules": [{ "rule_type": "cpu", "min": 1.0 }] }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    let resp = admin
        .put(format!("{}/api/alert-rules/{}", base_url, id))
        .json(&json!({ "cover_type": "nonsense" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "invalid cover_type on update → Validation/422");
}

// Updating a rule with mixed security + metric items re-runs item validation,
// which is a BadRequest → 400.
#[tokio::test]
async fn alert_rule_update_mixed_security_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let id = {
        let body: Value = admin
            .post(format!("{}/api/alert-rules", base_url))
            .json(&json!({ "name": "upd-mix", "rules": [{ "rule_type": "cpu", "min": 1.0 }] }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        body["data"]["id"].as_str().unwrap().to_string()
    };

    let resp = admin
        .put(format!("{}/api/alert-rules/{}", base_url, id))
        .json(&json!({
            "rules": [
                { "rule_type": "port_scan_detected" },
                { "rule_type": "memory", "min": 50.0 }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "mixing security + metric on update → 400");
}

// Updating an unknown alert-rule id → 404 (the update path loads the rule first).
#[tokio::test]
async fn alert_rule_update_missing_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .put(format!("{}/api/alert-rules/does-not-exist", base_url))
        .json(&json!({ "name": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// Deleting an unknown alert-rule id → 404 (delete returns NotFound when absent).
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

// A member cannot read the states list for a rule because the alert-rule routes
// (incl. `/states`) live behind the admin layer → 403.
#[tokio::test]
async fn alert_rule_states_member_forbidden() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "alert_states_member", "member").await;

    let resp = member
        .get(format!("{}/api/alert-rules/any-id/states", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "alert-rule routes are admin-only");
}
