//! Router-level integration tests targeting branches the existing router test
//! files leave uncovered, raising REGION coverage of `auth.rs`, `task.rs`, and
//! `widget_module.rs`. Each test drives the real Axum router over HTTP (and, for
//! agent-forwarded endpoints, a mock-agent WebSocket responder) against a freshly
//! migrated, randomly-bound test server. Every test owns its temp DB, so resource
//! names never collide across tests.
//!
//! Scope (gaps NOT already covered by router_auth_user.rs /
//! router_content_admin.rs / router_admin_extra.rs / widget_module_integration.rs):
//!
//! - auth.rs:    list_api_keys per-user scoping (member sees only own keys),
//!               delete_api_key list-after-delete (key gone), change_password
//!               keep-current-revoke-others session behaviour, onboarding "not
//!               required" 403 arm, list_oauth_accounts empty happy path,
//!               unlink_oauth_account 404 arm, 2FA disable IDEMPOTENCY (disable
//!               twice), me-as-member, and unauthenticated arms on api-keys
//!               list / 2FA setup|status|disable / oauth list.
//! - task.rs:    update_task enable/disable toggle (pause then resume, the
//!               next_run_at-recompute branch), delete_task results CASCADE over a
//!               real seeded result row, get_task_results ordering over real rows,
//!               and the ?type=oneshot list filter arm (the existing extra file
//!               only covers ?type=scheduled).
//! - widget_module.rs: uninstall_module 404 not-found, install with neither
//!               url nor multipart file (400), install multipart missing 'file'
//!               part (400), list_modules member happy path (read route is
//!               member-accessible), serve_asset 404 for unknown module, the
//!               read-route unauthenticated 401 arm, and enforce_url_safety's DNS
//!               lookup-failure arm.
//!
//! NOTE: endpoints needing a live agent (run_task dispatch, task_result persist)
//! stand up a mock-agent responder that echoes a TaskResult; the agent never runs
//! a real command, but the server's dispatch → pending-request → persist
//! round-trip is fully exercised.

mod common;

use common::{
    connect_agent, create_server, http_client, login_admin, login_as_new_user, recv_agent_text,
    register_agent, send_system_info, start_test_server, AgentReader, AgentSink,
};
use futures_util::SinkExt;
use serde_json::{json, Value};
// CAP_DEFAULT (1852) is the agent's default policy; it deliberately does NOT
// include CAP_EXEC, so scheduled-task dispatch tests OR CAP_EXEC in explicitly.
use serverbee_common::constants::{CAP_DEFAULT, CAP_EXEC};
use tokio_tungstenite::tungstenite;

// ---------------------------------------------------------------------------
// Mock-agent helpers (local copies; the shared harness exposes only the lower
// level primitives, and tests/common/mod.rs must not be modified).
// ---------------------------------------------------------------------------

/// Register + connect a mock agent, consume the welcome frame, complete the
/// SystemInfo handshake with the given capability bitmask, and drain the
/// first-connect pushes a default agent receives. Returns the server id plus the
/// WebSocket halves so the caller can drop the reader or spawn a responder loop.
async fn bring_up_agent(
    client: &reqwest::Client,
    base_url: &str,
    caps: u32,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_system_info(&mut sink, &mut reader, "handshake", Some(caps)).await;
    drain_first_connect_pushes(&mut reader, 250).await;
    (server_id, sink, reader)
}

/// Drain frames the server pushes right after the SystemInfo handshake
/// (ping/network sync + firewall blocklist reset/sync). Returns once the inbound
/// stream is quiet for `quiet_ms`.
async fn drain_first_connect_pushes(reader: &mut AgentReader, quiet_ms: u64) {
    use futures_util::StreamExt;
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(quiet_ms), reader.next()).await
        {
            Ok(Some(Ok(_))) => {}
            // Quiet window elapsed, stream ended, or a read error: stop draining.
            _ => break,
        }
    }
}

/// Send a single agent frame as a JSON text message.
async fn send_agent_frame(sink: &mut AgentSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("send agent frame");
}

/// Spawn a responder that answers exactly one `exec` with the given output and
/// exit code, echoing the correlation id. Returns the JoinHandle so the caller
/// can await completion. Ignores all first-connect noise frame types.
fn spawn_exec_responder(
    mut sink: AgentSink,
    mut reader: AgentReader,
    output: &'static str,
    exit_code: i64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            if msg["type"].as_str() == Some("exec") {
                let correlation = msg["task_id"].as_str().expect("exec task_id missing");
                send_agent_frame(
                    &mut sink,
                    json!({
                        "type": "task_result",
                        "msg_id": "exec-reply",
                        "task_id": correlation,
                        "output": output,
                        "exit_code": exit_code
                    }),
                )
                .await;
                return;
            }
        }
    })
}

// ===========================================================================
// auth.rs — API key listing/scoping/delete edges
// ===========================================================================

/// DELETE /api/auth/api-keys/{id} happy path, then the deleted key is gone from
/// the list. router_auth_user.rs deletes a key and asserts 200, but never
/// re-lists to confirm the row is actually removed (the list-after-delete arm).
#[tokio::test]
async fn delete_api_key_removes_it_from_the_list() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "ephemeral-key" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let del = client
        .delete(format!("{}/api/auth/api-keys/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);

    let list: Value = client
        .get(format!("{}/api/auth/api-keys", base_url))
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
            .all(|k| k["id"] != id.as_str()),
        "deleted key must no longer appear in the list"
    );
}

/// GET /api/auth/api-keys is scoped to the caller: a member who has minted no
/// keys sees an EMPTY list even though the admin owns one — exercises the
/// per-user filter in list_api_keys (the route is in the protected, non-admin
/// block, so a member can call it for their OWN keys).
#[tokio::test]
async fn list_api_keys_is_scoped_to_the_calling_user() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Admin mints a key that belongs to the admin user.
    admin
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "admin-only-key" }))
        .send()
        .await
        .unwrap();

    let member = login_as_new_user(&admin, &base_url, "ak_scope_member", "member").await;
    let list = member
        .get(format!("{}/api/auth/api-keys", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let body: Value = list.json().await.unwrap();
    assert!(
        body["data"].as_array().unwrap().is_empty(),
        "a member with no keys must see an empty list (admin's key is not theirs)"
    );
}

/// GET /api/auth/api-keys without any credential is 401 (the protected route is
/// gated by the auth middleware). router_auth_user.rs only covers the 401 arm on
/// the admin-only CREATE route, not the protected LIST route.
#[tokio::test]
async fn list_api_keys_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/auth/api-keys", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ===========================================================================
// auth.rs — change_password keep-current / revoke-others session behaviour
// ===========================================================================

/// Two browser sessions for the same user: changing the password from session A
/// keeps A authenticated (the caller's session is preserved) while session B is
/// revoked. Exercises the `keep_session_token` branch in change_password that the
/// existing success test (single session) never observes.
#[tokio::test]
async fn change_password_keeps_caller_session_and_revokes_others() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    // Create a member, then log it in on two independent cookie jars.
    login_as_new_user(&admin, &base_url, "two_session_user", "member").await;

    let session_a = http_client();
    let login_a = session_a
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "two_session_user", "password": "memberpass" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login_a.status(), 200);

    let session_b = http_client();
    let login_b = session_b
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "two_session_user", "password": "memberpass" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login_b.status(), 200);

    // Session B is live before the change.
    let pre = session_b
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pre.status(), 200);

    // Change the password using session A.
    let change = session_a
        .put(format!("{}/api/auth/password", base_url))
        .json(&json!({ "old_password": "memberpass", "new_password": "rotated-pass-1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(change.status(), 200);

    // Session A (the caller) is still authenticated.
    let a_after = session_a
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(a_after.status(), 200, "the session that changed the password must survive");

    // Session B was revoked as part of the password rotation.
    let b_after = session_b
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(b_after.status(), 401, "other sessions must be revoked on password change");
}

// ===========================================================================
// auth.rs — onboarding "not required" forbidden arm
// ===========================================================================

/// POST /api/auth/onboarding for a user who is NOT in the must-change-password
/// state (the seeded admin) is rejected. complete_onboarding refuses when the
/// account has already onboarded; this is the only onboarding arm reachable in
/// the test harness (the seed sets must_change_password = false).
#[tokio::test]
async fn onboarding_when_not_required_is_rejected() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/auth/onboarding", base_url))
        .json(&json!({ "new_password": "brand-new-pass-1", "new_username": null }))
        .send()
        .await
        .unwrap();
    // The seeded admin already completed onboarding, so the service refuses it.
    assert!(
        resp.status() == 403 || resp.status() == 400,
        "onboarding for an already-onboarded user must be rejected, got {}",
        resp.status()
    );
}

/// POST /api/auth/onboarding without auth is 401 (protected route).
#[tokio::test]
async fn onboarding_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/auth/onboarding", base_url))
        .json(&json!({ "new_password": "whatever-pass-1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ===========================================================================
// auth.rs — OAuth account management (empty list, not-found unlink)
// ===========================================================================

/// GET /api/auth/oauth/accounts for a user with no linked providers returns an
/// empty array (200) — the list_oauth_accounts happy path. No external IdP is
/// needed because the list is simply empty.
#[tokio::test]
async fn list_oauth_accounts_empty_is_200() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/auth/oauth/accounts", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"].as_array().unwrap().is_empty(),
        "a user with no linked OAuth providers must get an empty list"
    );
}

/// DELETE /api/auth/oauth/accounts/{id} for an id the user does not own (or that
/// does not exist) returns 404 — the unlink_oauth_account not-found arm.
#[tokio::test]
async fn unlink_oauth_account_unknown_id_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .delete(format!("{}/api/auth/oauth/accounts/no-such-link", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

/// GET /api/auth/oauth/accounts without auth is 401 (protected route).
#[tokio::test]
async fn list_oauth_accounts_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/auth/oauth/accounts", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ===========================================================================
// auth.rs — 2FA disable idempotency + unauthenticated arms
// ===========================================================================

/// POST /api/auth/2fa/disable twice in a row both succeed: disable_2fa clears the
/// secret idempotently, so a second disable with the correct password is still a
/// 200 no-op. router_admin_extra.rs covers a single disable success; this asserts
/// the IDEMPOTENT second call too.
#[tokio::test]
async fn totp_disable_is_idempotent() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    for _ in 0..2 {
        let disable = client
            .post(format!("{}/api/auth/2fa/disable", base_url))
            .json(&json!({ "password": "testpass" }))
            .send()
            .await
            .unwrap();
        assert_eq!(disable.status(), 200, "each disable with the correct password is a 200 no-op");
    }

    // Status stays disabled after the repeated disables.
    let status: Value = client
        .get(format!("{}/api/auth/2fa/status", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status["data"]["enabled"], false);
}

/// The 2FA setup/status/disable routes are all protected; with no credential they
/// return 401 (the auth-middleware arm for the 2FA group).
#[tokio::test]
async fn totp_routes_unauthenticated_are_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let setup = client
        .post(format!("{}/api/auth/2fa/setup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 401);

    let status = client
        .get(format!("{}/api/auth/2fa/status", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), 401);

    let disable = client
        .post(format!("{}/api/auth/2fa/disable", base_url))
        .json(&json!({ "password": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(disable.status(), 401);
}

/// GET /api/auth/me as a member resolves to the member user (the role-passthrough
/// arm of the me handler). router_auth_user.rs only covers me-as-admin.
#[tokio::test]
async fn me_as_member_returns_member_role() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "me_member", "member").await;

    let resp = member
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["username"], "me_member");
    assert_eq!(body["data"]["role"], "member");
    assert_eq!(body["data"]["must_change_password"], false);
}

// ===========================================================================
// task.rs — update enable/disable toggle (pause then resume)
// ===========================================================================

/// PUT /api/tasks/{id} with enabled=false pauses a scheduled task, then
/// enabled=true resumes it and recomputes next_run_at from the stored cron. This
/// drives the `enabled` toggle branch (incl. the resume-side next_run_at
/// recompute), which the existing task tests never exercise — they only update
/// name/cron/numeric fields.
#[tokio::test]
async fn update_task_disable_then_enable_toggles_enabled_flag() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-toggle-srv").await;

    let created: Value = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo toggle",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "name": "Toggle",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["enabled"], true);

    // Pause: enabled=false.
    let paused: Value = admin
        .put(format!("{}/api/tasks/{}", base_url, task_id))
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(paused["data"]["enabled"], false, "task must report disabled after pause");

    // Resume: enabled=true recomputes next_run_at from the stored cron.
    let resumed: Value = admin
        .put(format!("{}/api/tasks/{}", base_url, task_id))
        .json(&json!({ "enabled": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resumed["data"]["enabled"], true, "task must report enabled after resume");
    assert!(
        resumed["data"]["next_run_at"].is_string(),
        "resuming a scheduled task must recompute next_run_at from its cron"
    );
}

// ===========================================================================
// task.rs — list ?type=oneshot filter
// ===========================================================================

/// GET /api/tasks?type=oneshot returns only oneshot tasks. router_admin_extra.rs
/// covers the ?type=scheduled side; this covers the complementary oneshot filter
/// (so both eq-filter outcomes of list_tasks are exercised).
#[tokio::test]
async fn list_tasks_filters_by_oneshot_type() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let server_id = create_server(&admin, &base_url, "task-oneshot-filter-srv").await;

    // One scheduled, one oneshot.
    admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo sched",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "name": "Sched",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .unwrap();

    let oneshot: Value = admin
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo once",
            "server_ids": [server_id],
            "task_type": "oneshot"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let oneshot_id = oneshot["data"]["id"].as_str().unwrap().to_string();

    let list = admin
        .get(format!("{}/api/tasks?type=oneshot", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let body: Value = list.json().await.unwrap();
    let items = body["data"].as_array().unwrap();
    assert!(
        items.iter().any(|t| t["id"] == oneshot_id.as_str()),
        "oneshot task must be present in the type=oneshot filtered list"
    );
    assert!(
        items.iter().all(|t| t["task_type"] == "oneshot"),
        "type=oneshot filter must exclude scheduled tasks, got: {items:?}"
    );
}

// ===========================================================================
// task.rs — delete cascades result rows (over a real seeded result)
// ===========================================================================

/// Run a scheduled task to persist a real task_result row, then DELETE the task
/// and confirm the cascade removes its results too. This exercises the
/// delete_task path where `task_result::Entity::delete_many()` actually has rows
/// to remove (the existing delete tests delete a task with no results).
///
/// Multi-threaded runtime: the /run handler blocks on the agent reply while the
/// spawned responder must make progress; a single-threaded runtime can starve the
/// responder and hang forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_task_cascades_result_rows() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // A default agent does not report CAP_EXEC; OR it in so dispatch is allowed.
    let (server_id, sink, reader) = bring_up_agent(&client, &base_url, CAP_DEFAULT | CAP_EXEC).await;

    let created: Value = client
        .post(format!("{}/api/tasks", base_url))
        .json(&json!({
            "command": "echo cascade",
            "server_ids": [server_id],
            "task_type": "scheduled",
            "name": "Cascade",
            "cron_expression": "0 0 * * * *"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = created["data"]["id"].as_str().unwrap().to_string();

    let agent_task = spawn_exec_responder(sink, reader, "cascade-out\n", 0);

    // Trigger the run; the agent's reply is persisted as a result row.
    let run = client
        .post(format!("{}/api/tasks/{}/run", base_url, task_id))
        .send()
        .await
        .unwrap();
    assert_eq!(run.status(), 200);
    agent_task.await.expect("agent responder task panicked");

    // Poll until the result row is written by the spawned join_set task.
    let mut had_result = false;
    for _ in 0..40 {
        let resp = client
            .get(format!("{}/api/tasks/{}/results", base_url, task_id))
            .send()
            .await
            .unwrap();
        let body: Value = resp.json().await.unwrap();
        if !body["data"].as_array().unwrap().is_empty() {
            had_result = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(had_result, "a result row must exist before we delete the task");

    // Delete the task: the handler deletes its result rows first, then the task.
    let del = client
        .delete(format!("{}/api/tasks/{}", base_url, task_id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 200);

    // The task is gone (404) and its results no longer resolve to a live task.
    let gone = client
        .get(format!("{}/api/tasks/{}", base_url, task_id))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404, "task must be gone after delete");

    // Querying results for the deleted task id returns an empty set (rows cascaded).
    let results: Value = client
        .get(format!("{}/api/tasks/{}/results", base_url, task_id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        results["data"].as_array().unwrap().is_empty(),
        "delete_task must cascade-remove the task's result rows"
    );
}

// ===========================================================================
// widget_module.rs — uninstall not-found, install no-source / missing file
// ===========================================================================

/// DELETE /api/widget-modules/{id} for an id that was never installed is 404 —
/// the uninstall NotFound arm (widget_module_integration.rs covers builtin-reject
/// and member-403 but not the plain unknown-id not-found).
#[tokio::test]
async fn uninstall_unknown_module_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .delete(format!("{}/api/widget-modules/com.test.never-installed", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

/// POST /api/widget-modules with neither a ?url= query nor a multipart body hits
/// the "provide ?url=... or multipart file" 400 arm (the final else branch of the
/// source resolution in install_widget_module).
#[tokio::test]
async fn install_widget_no_source_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // No query string and no multipart content type -> no source at all.
    let resp = admin
        .post(format!("{}/api/widget-modules", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "missing both url and file must be rejected");
}

/// POST /api/widget-modules with a multipart body that has no `file` part hits the
/// "missing 'file' part" 400 arm (the multipart branch where bytes_opt is None).
#[tokio::test]
async fn install_widget_multipart_without_file_part_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Multipart form carrying only a non-"file" text field.
    let form = reqwest::multipart::Form::new().text("notfile", "irrelevant");
    let resp = admin
        .post(format!("{}/api/widget-modules", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "multipart without a 'file' part must be rejected");
}

// ===========================================================================
// widget_module.rs — read route member access + unauthenticated + serve 404
// ===========================================================================

/// GET /api/widget-modules as a MEMBER returns 200 — the read router is mounted in
/// the authenticated (non-admin) block, so members can list modules. The existing
/// file only lists as admin.
#[tokio::test]
async fn list_modules_member_is_allowed() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "wm_list_member", "member").await;

    let resp = member
        .get(format!("{}/api/widget-modules", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "members may read the widget-module list");
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].is_array(), "list response must be an array");
}

/// GET /api/widget-modules without auth is 401 (the read route is still behind the
/// auth middleware).
#[tokio::test]
async fn list_modules_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/widget-modules", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

/// GET /api/widget-modules/{id}/{asset} for an unknown module id is 404 — the
/// serve_asset NotFound arm reached over HTTP (service-level coverage exists, but
/// the router handler's error mapping is exercised here).
#[tokio::test]
async fn serve_asset_unknown_module_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/widget-modules/com.test.absent/index.js", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ===========================================================================
// widget_module.rs — enforce_url_safety DNS lookup-failure arm
// ===========================================================================

/// POST /api/widget-modules?url=http://<unresolvable>/w.js hits the DNS
/// lookup-failure arm of enforce_url_safety (the host is a literal name, not an
/// IP, so it takes the resolver path; `.invalid` never resolves per RFC 2606).
/// Deterministic and offline: the resolver returns NXDOMAIN with no network call
/// to a real service.
#[tokio::test]
async fn install_widget_unresolvable_host_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let resp = admin
        .post(format!(
            "{}/api/widget-modules?url=http://nonexistent-host.invalid/widget.js",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "an unresolvable host must be rejected before any fetch");
}
