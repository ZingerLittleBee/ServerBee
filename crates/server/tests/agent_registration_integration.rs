mod common;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite;

use common::{connect_agent, http_client, login_admin, start_test_server};

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

async fn assert_ws_unauthorized(base_url: &str, token: &str) {
    let url = format!(
        "{}/api/agent/ws?token={token}",
        base_url.replace("http://", "ws://")
    );
    let error = tokio_tungstenite::connect_async(url)
        .await
        .expect_err("credential must be rejected");
    assert!(matches!(
        error,
        tungstenite::Error::Http(response) if response.status().as_u16() == 401
    ));
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
