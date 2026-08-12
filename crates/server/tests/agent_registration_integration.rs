mod common;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite;

use common::{
    connect_agent, http_client, login_admin, register_agent, send_system_info, start_test_server,
};

fn onboarding_body(request_id: &str, name: &str) -> Value {
    json!({
        "onboarding_request_id": request_id,
        "name": name
    })
}

async fn onboard(client: &reqwest::Client, base_url: &str, request_id: &str, name: &str) -> Value {
    let response = client
        .post(format!("{base_url}/api/servers"))
        .json(&onboarding_body(request_id, name))
        .send()
        .await
        .expect("onboard request");
    assert_eq!(response.status(), 200);
    response.json().await.expect("onboard response")
}

async fn claim(
    client: &reqwest::Client,
    base_url: &str,
    code: &str,
    token: &str,
) -> reqwest::Response {
    client
        .post(format!("{base_url}/api/agent/register"))
        .bearer_auth(code)
        .json(&json!({ "proposed_run_token": token }))
        .send()
        .await
        .expect("claim request")
}

async fn assert_ws_handshake_unauthorized(url: String) {
    let error = tokio_tungstenite::connect_async(url)
        .await
        .expect_err("credential must be rejected");
    assert!(matches!(
        error,
        tungstenite::Error::Http(response) if response.status().as_u16() == 401
    ));
}

async fn assert_ws_unauthorized(base_url: &str, token: &str) {
    assert_ws_handshake_unauthorized(format!(
        "{}/api/agent/ws?token={token}",
        base_url.replace("http://", "ws://")
    ))
    .await;
}

#[tokio::test]
async fn onboarding_request_replay_returns_same_server_without_plaintext_code() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let first = onboard(&client, &base_url, "request-1", " Server One ").await;
    let second = onboard(&client, &base_url, "request-1", "Server One").await;

    assert_eq!(first["data"]["replayed"], false);
    assert!(first["data"]["enrollment"]["code"].is_string());
    assert_eq!(second["data"]["replayed"], true);
    assert_eq!(second["data"]["server_id"], first["data"]["server_id"]);
    assert!(second["data"]["enrollment"].is_null());
    assert_eq!(
        second["data"]["outstanding_offer"]["id"],
        first["data"]["enrollment"]["id"]
    );

    let conflict = client
        .post(format!("{base_url}/api/servers"))
        .json(&onboarding_body("request-1", "Different Server"))
        .send()
        .await
        .expect("conflicting replay");
    assert_eq!(conflict.status(), 409);
}

#[tokio::test]
async fn claim_uses_agent_proposed_token_and_returns_no_secret() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let code = created["data"]["enrollment"]["code"]
        .as_str()
        .expect("code");
    let server_id = created["data"]["server_id"].as_str().expect("server id");
    let token = "agent-proposed-token-0123456789abcdef";

    let response = claim(&client, &base_url, code, token).await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("claim response");
    assert_eq!(body["data"]["server_id"], server_id);
    assert!(body["data"].get("token").is_none());

    let (mut sink, mut reader) = connect_agent(&base_url, token).await;
    let welcome = reader
        .next()
        .await
        .expect("welcome frame")
        .expect("welcome read");
    assert!(matches!(welcome, tungstenite::Message::Text(_)));
    sink.close().await.expect("close socket");

    let replay = claim(&client, &base_url, code, token).await;
    assert_eq!(replay.status(), 401);
}

#[tokio::test]
async fn registration_requires_a_proposed_run_token_body() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let code = created["data"]["enrollment"]["code"]
        .as_str()
        .expect("code");

    let missing = client
        .post(format!("{base_url}/api/agent/register"))
        .bearer_auth(code)
        .send()
        .await
        .expect("missing-body request");
    assert!(!missing.status().is_success());

    let malformed = claim(&client, &base_url, code, "too-short").await;
    assert_eq!(malformed.status(), 400);

    let valid = claim(
        &client,
        &base_url,
        code,
        "valid-token-0123456789abcdefghijkl",
    )
    .await;
    assert_eq!(
        valid.status(),
        200,
        "invalid bodies must not consume the offer"
    );
}

#[tokio::test]
async fn graceful_reenrollment_preserves_old_authority_until_new_claim() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");
    let initial_code = created["data"]["enrollment"]["code"]
        .as_str()
        .expect("code");
    let old_token = "old-agent-token-0123456789abcdefghijk";
    assert_eq!(
        claim(&client, &base_url, initial_code, old_token)
            .await
            .status(),
        200
    );

    let response = client
        .post(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/re-enrollment"
        ))
        .json(&json!({ "mode": "graceful" }))
        .send()
        .await
        .expect("begin graceful");
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("graceful response");
    let new_code = body["data"]["enrollment"]["code"]
        .as_str()
        .expect("new code");

    let state: Value = client
        .get(format!(
            "{base_url}/api/servers/{server_id}/agent-authority"
        ))
        .send()
        .await
        .expect("authority state")
        .json()
        .await
        .expect("state body");
    assert_eq!(state["data"]["status"], "claimed");
    let (mut old_sink, _) = connect_agent(&base_url, old_token).await;
    old_sink.close().await.expect("close old socket");

    let new_token = "new-agent-token-0123456789abcdefghijk";
    assert_eq!(
        claim(&client, &base_url, new_code, new_token)
            .await
            .status(),
        200
    );
    assert_ws_unauthorized(&base_url, old_token).await;
    let (mut new_sink, _) = connect_agent(&base_url, new_token).await;
    new_sink.close().await.expect("close new socket");
}

#[tokio::test]
async fn emergency_reenrollment_revokes_old_authority_immediately() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");
    let code = created["data"]["enrollment"]["code"]
        .as_str()
        .expect("code");
    let token = "old-agent-token-0123456789abcdefghijk";
    assert_eq!(claim(&client, &base_url, code, token).await.status(), 200);

    let response = client
        .post(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/re-enrollment"
        ))
        .json(&json!({ "mode": "emergency" }))
        .send()
        .await
        .expect("begin emergency");
    assert_eq!(response.status(), 200);
    assert_ws_unauthorized(&base_url, token).await;

    let state: Value = client
        .get(format!(
            "{base_url}/api/servers/{server_id}/agent-authority"
        ))
        .send()
        .await
        .expect("authority state")
        .json()
        .await
        .expect("state body");
    assert_eq!(state["data"]["status"], "unclaimed");
    assert!(state["data"]["outstanding_offer"].is_object());
}

#[tokio::test]
async fn offer_replacement_is_exact_and_revocation_is_idempotent() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");
    let offer_id = created["data"]["enrollment"]["id"]
        .as_str()
        .expect("offer id");

    let missing = client
        .post(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/offers/missing/replace"
        ))
        .send()
        .await
        .expect("missing replacement");
    assert_eq!(missing.status(), 404);

    let replacement = client
        .post(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/offers/{offer_id}/replace"
        ))
        .send()
        .await
        .expect("replace offer");
    assert_eq!(replacement.status(), 200);
    let replacement: Value = replacement.json().await.expect("replacement body");
    let new_offer_id = replacement["data"]["enrollment"]["id"]
        .as_str()
        .expect("new offer id");

    let stale = client
        .post(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/offers/{offer_id}/replace"
        ))
        .send()
        .await
        .expect("stale replacement");
    assert_eq!(stale.status(), 409);

    for expected_already_revoked in [false, true] {
        let revoked: Value = client
            .delete(format!(
                "{base_url}/api/servers/{server_id}/agent-authority/offers/{new_offer_id}"
            ))
            .send()
            .await
            .expect("revoke offer")
            .json()
            .await
            .expect("revoke body");
        assert_eq!(revoked["data"]["already_revoked"], expected_already_revoked);
    }
}

#[tokio::test]
async fn authority_revocation_creates_no_offer_and_events_survive_server_delete() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");
    let code = created["data"]["enrollment"]["code"]
        .as_str()
        .expect("code");
    let token = "agent-token-0123456789abcdefghijklmnop";
    assert_eq!(claim(&client, &base_url, code, token).await.status(), 200);

    let revoked = client
        .delete(format!(
            "{base_url}/api/servers/{server_id}/agent-authority"
        ))
        .send()
        .await
        .expect("revoke authority");
    assert_eq!(revoked.status(), 200);
    assert_ws_unauthorized(&base_url, token).await;

    let state: Value = client
        .get(format!(
            "{base_url}/api/servers/{server_id}/agent-authority"
        ))
        .send()
        .await
        .expect("authority state")
        .json()
        .await
        .expect("state body");
    assert_eq!(state["data"]["status"], "unclaimed");
    assert!(state["data"]["outstanding_offer"].is_null());

    assert_eq!(
        client
            .delete(format!("{base_url}/api/servers/{server_id}"))
            .send()
            .await
            .expect("delete server")
            .status(),
        200
    );
    let history: Value = client
        .get(format!(
            "{base_url}/api/agent-authority/events?server_id={server_id}"
        ))
        .send()
        .await
        .expect("history")
        .json()
        .await
        .expect("history body");
    let transitions: Vec<&str> = history["data"]
        .as_array()
        .expect("events")
        .iter()
        .filter_map(|event| event["transition"].as_str())
        .collect();
    assert!(transitions.contains(&"authority_revoked"));
    assert!(transitions.contains(&"server_deleted"));
}

// ===========================================================================
// Authority offer error mapping — `map_issue_offer_error`,
// `map_revoke_offer_error` and `map_reenrollment_error` in
// `router/api/server.rs`. The success arms are covered above; these tests pin
// the HTTP contract (status + machine-readable `error.code` + `details`) that
// the web/iOS clients branch on.
// ===========================================================================

/// Parse an error response into `(status, error.code, error.details)`.
async fn error_parts(response: reqwest::Response) -> (u16, String, Value) {
    let status = response.status().as_u16();
    let body: Value = response.json().await.expect("error body");
    let code = body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let details = body["error"]["details"].clone();
    (status, code, details)
}

// Issuing an offer for a server id that does not exist → 404
// (`IssueOfferError::NotFound`). The id is well-formed, so it gets past
// `parse_server_id` and reaches the store lookup.
#[tokio::test]
async fn issue_offer_unknown_server_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let response = client
        .post(format!(
            "{base_url}/api/servers/no-such-server/agent-authority/offers"
        ))
        .json(&json!({}))
        .send()
        .await
        .expect("issue offer");
    let (status, code, _) = error_parts(response).await;
    assert_eq!(status, 404);
    assert_eq!(code, "NOT_FOUND");
}

// Onboarding already mints an Outstanding offer, so an immediate issue-offer
// call collides → 409 `ENROLLMENT_OFFER_OUTSTANDING`, and the payload carries
// the current offer so the UI can show/replace it without a second round trip.
#[tokio::test]
async fn issue_offer_when_outstanding_exists_is_409_with_current_offer_details() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");
    let offer_id = created["data"]["enrollment"]["id"]
        .as_str()
        .expect("offer id");

    let response = client
        .post(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/offers"
        ))
        .json(&json!({}))
        .send()
        .await
        .expect("issue offer");
    let (status, code, details) = error_parts(response).await;
    assert_eq!(status, 409);
    assert_eq!(code, "ENROLLMENT_OFFER_OUTSTANDING");
    assert_eq!(details["current_offer"]["id"], offer_id);
    assert!(
        details["current_offer"]["code_prefix"].is_string(),
        "conflict details expose the offer prefix, never the plaintext code"
    );
    assert!(
        details["current_offer"].get("code").is_none(),
        "the plaintext enrollment code must never reappear in an error body"
    );
}

// Once an agent has claimed authority the offer is Consumed, so there is no
// Outstanding offer to collide with — the AlreadyClaimed guard fires first and
// steers the caller to re-enrollment instead.
#[tokio::test]
async fn issue_offer_after_agent_claimed_is_409_already_claimed() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    let (server_id, token) = register_agent(&client, &base_url).await;

    let (mut sink, mut reader) = connect_agent(&base_url, &token).await;
    send_system_info(&mut sink, &mut reader, "sysinfo-1", None).await;

    let response = client
        .post(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/offers"
        ))
        .json(&json!({}))
        .send()
        .await
        .expect("issue offer");
    let (status, code, details) = error_parts(response).await;
    assert_eq!(status, 409);
    assert_eq!(code, "AGENT_AUTHORITY_ALREADY_CLAIMED");
    assert!(
        details.is_null(),
        "already-claimed carries no offer details"
    );

    sink.close().await.expect("close socket");
}

// Both NotFound arms of `map_revoke_offer_error`: an unknown server id, and a
// real server with an offer id that belongs to no offer.
#[tokio::test]
async fn revoke_offer_unknown_server_and_unknown_offer_are_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");

    let unknown_server = client
        .delete(format!(
            "{base_url}/api/servers/no-such-server/agent-authority/offers/no-such-offer"
        ))
        .send()
        .await
        .expect("revoke on unknown server");
    let (status, code, _) = error_parts(unknown_server).await;
    assert_eq!(status, 404, "the server is checked before the offer");
    assert_eq!(code, "NOT_FOUND");

    let unknown_offer = client
        .delete(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/offers/no-such-offer"
        ))
        .send()
        .await
        .expect("revoke unknown offer");
    let (status, code, _) = error_parts(unknown_offer).await;
    assert_eq!(status, 404);
    assert_eq!(code, "NOT_FOUND");
}

// Revoking an offer the agent already consumed is a terminal-state conflict,
// not a no-op: `RevokeOfferError::Terminal` → 409 `ENROLLMENT_OFFER_TERMINAL`.
// (Only the Revoked outcome short-circuits to `already_revoked: true`; that
// idempotent path is asserted in `offer_replacement_is_exact_and_revocation_is_idempotent`.)
#[tokio::test]
async fn revoke_offer_after_code_consumed_is_409_terminal() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");
    let offer_id = created["data"]["enrollment"]["id"]
        .as_str()
        .expect("offer id");
    let code = created["data"]["enrollment"]["code"]
        .as_str()
        .expect("code");
    assert_eq!(
        claim(
            &client,
            &base_url,
            code,
            "agent-token-0123456789abcdefghijklmnop"
        )
        .await
        .status(),
        200
    );

    let response = client
        .delete(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/offers/{offer_id}"
        ))
        .send()
        .await
        .expect("revoke consumed offer");
    let (status, error_code, _) = error_parts(response).await;
    assert_eq!(status, 409);
    assert_eq!(error_code, "ENROLLMENT_OFFER_TERMINAL");
}

// Re-enrollment presupposes an existing authority to rotate. On a freshly
// onboarded (Unclaimed) server it is a 409 that names the right next step.
#[tokio::test]
async fn begin_reenrollment_on_unclaimed_server_is_409() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");

    let response = client
        .post(format!(
            "{base_url}/api/servers/{server_id}/agent-authority/re-enrollment"
        ))
        .json(&json!({ "mode": "graceful" }))
        .send()
        .await
        .expect("begin re-enrollment");
    let (status, code, details) = error_parts(response).await;
    assert_eq!(status, 409);
    assert_eq!(code, "AGENT_AUTHORITY_UNCLAIMED");
    assert!(details.is_null());
}

// Re-enrollment against a server id that does not exist → 404
// (`ReenrollmentError::NotFound`).
#[tokio::test]
async fn begin_reenrollment_unknown_server_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let response = client
        .post(format!(
            "{base_url}/api/servers/no-such-server/agent-authority/re-enrollment"
        ))
        .json(&json!({ "mode": "emergency" }))
        .send()
        .await
        .expect("begin re-enrollment");
    let (status, code, _) = error_parts(response).await;
    assert_eq!(status, 404);
    assert_eq!(code, "NOT_FOUND");
}

// ===========================================================================
// Adjacent create/read arms on the same router module.
// ===========================================================================

// `get_gpu_records` had no coverage at all. A server that never reported a GPU
// answers 200 with an empty array rather than 404 — the dashboard renders an
// empty chart instead of an error state.
#[tokio::test]
async fn gpu_records_empty_for_new_server_is_200() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let created = onboard(&client, &base_url, "request-1", "Server One").await;
    let server_id = created["data"]["server_id"].as_str().expect("server id");

    let response = client
        .get(format!(
            "{base_url}/api/servers/{server_id}/gpu-records?from=2026-01-01T00:00:00Z&to=2026-01-02T00:00:00Z"
        ))
        .send()
        .await
        .expect("gpu records");
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("gpu records body");
    assert_eq!(body["data"].as_array().map(Vec::len), Some(0));
}

// `create_server` rejects a malformed `onboarding_request_id` (whitespace is
// disallowed) and an out-of-range offer TTL before touching the database.
#[tokio::test]
async fn create_server_invalid_onboarding_request_id_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let bad_request_id = client
        .post(format!("{base_url}/api/servers"))
        .json(&json!({ "onboarding_request_id": "has space", "name": "Server One" }))
        .send()
        .await
        .expect("create with bad request id");
    let (status, code, _) = error_parts(bad_request_id).await;
    assert_eq!(status, 400);
    assert_eq!(code, "BAD_REQUEST");

    let bad_ttl = client
        .post(format!("{base_url}/api/servers"))
        .json(&json!({
            "onboarding_request_id": "request-ttl",
            "name": "Server One",
            "ttl_secs": 0
        }))
        .send()
        .await
        .expect("create with bad ttl");
    let (status, code, _) = error_parts(bad_ttl).await;
    assert_eq!(status, 400, "offer ttl must be 1..=86400 seconds");
    assert_eq!(code, "BAD_REQUEST");

    let listed: Value = client
        .get(format!("{base_url}/api/servers"))
        .send()
        .await
        .expect("list servers")
        .json()
        .await
        .expect("list body");
    assert_eq!(
        listed["data"].as_array().map(Vec::len),
        Some(0),
        "rejected onboarding requests must not create server rows"
    );
}

// The Agent WS rejects a request carrying no credential at all, and one whose
// credential cannot even be parsed, before any store lookup happens. Both must
// answer 401 — a parse failure leaking as a 500 would tell a prober that the
// endpoint reached the authority store.
#[tokio::test]
async fn agent_ws_rejects_missing_and_malformed_tokens() {
    let (base_url, _tmp) = start_test_server().await;

    assert_ws_handshake_unauthorized(format!(
        "{}/api/agent/ws",
        base_url.replace("http://", "ws://")
    ))
    .await;
    // Shorter than the 8-character minimum, so `PresentedRunToken::parse` fails.
    assert_ws_unauthorized(&base_url, "short").await;
}
