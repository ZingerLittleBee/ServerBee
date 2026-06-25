//! Integration tests for the mobile auth API (`router/api/mobile.rs`).
//!
//! Covers the offline mobile auth lifecycle exposed to the iOS client:
//! device login (username/password -> access/refresh tokens), token refresh,
//! logout, device listing/revocation, the desktop-to-mobile pairing flow, and
//! push-token registration. Every test drives the real HTTP stack stood up by
//! the shared harness; nothing reaches out to the network.

mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ── Helpers ──────────────────────────────────────────────────────────────────

const INST_ID: &str = "inst-1234";
const DEVICE_NAME: &str = "Test iPhone";

/// Perform a mobile login as the seeded admin and return the parsed
/// `{ "data": MobileTokenResponse }` body.
async fn mobile_login(
    client: &reqwest::Client,
    base_url: &str,
    username: &str,
    password: &str,
    installation_id: &str,
) -> reqwest::Response {
    client
        .post(format!("{}/api/mobile/auth/login", base_url))
        .json(&json!({
            "username": username,
            "password": password,
            "installation_id": installation_id,
            "device_name": DEVICE_NAME,
        }))
        .send()
        .await
        .expect("mobile login request failed")
}

/// Log in as admin over the mobile endpoint and return the bearer access token.
async fn mobile_admin_token(client: &reqwest::Client, base_url: &str, installation_id: &str) -> Value {
    let resp = mobile_login(client, base_url, "admin", "testpass", installation_id).await;
    assert_eq!(resp.status(), 200, "admin mobile login should succeed");
    resp.json::<Value>().await.expect("parse mobile login body")
}

// ── Login ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn mobile_login_happy_path_returns_tokens() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = mobile_login(&client, &base_url, "admin", "testpass", INST_ID).await;
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let data = &body["data"];
    assert!(
        data["access_token"].as_str().is_some_and(|t| !t.is_empty()),
        "access_token must be present"
    );
    assert!(
        data["refresh_token"].as_str().is_some_and(|t| !t.is_empty()),
        "refresh_token must be present"
    );
    assert_eq!(data["token_type"], "Bearer");
    assert!(data["access_expires_in_secs"].as_i64().is_some());
    assert!(data["refresh_expires_in_secs"].as_i64().is_some());
    assert_eq!(data["user"]["username"], "admin");
    assert_eq!(data["user"]["role"], "admin");
}

#[tokio::test]
async fn mobile_login_wrong_password_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = mobile_login(&client, &base_url, "admin", "wrong-password", INST_ID).await;
    assert_eq!(resp.status(), 401, "wrong password must be unauthorized");
}

#[tokio::test]
async fn mobile_login_unknown_user_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = mobile_login(&client, &base_url, "nobody", "whatever", INST_ID).await;
    assert_eq!(resp.status(), 401, "unknown user must be unauthorized");
}

#[tokio::test]
async fn mobile_login_missing_credentials_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    // Empty username/password trips the Validation guard (422).
    let resp = client
        .post(format!("{}/api/mobile/auth/login", base_url))
        .json(&json!({
            "username": "",
            "password": "",
            "installation_id": INST_ID,
            "device_name": DEVICE_NAME,
        }))
        .send()
        .await
        .expect("login request failed");
    assert_eq!(resp.status(), 422, "empty credentials must be a validation error");
}

#[tokio::test]
async fn mobile_login_missing_installation_id_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/auth/login", base_url))
        .json(&json!({
            "username": "admin",
            "password": "testpass",
            "installation_id": "",
            "device_name": "",
        }))
        .send()
        .await
        .expect("login request failed");
    assert_eq!(
        resp.status(),
        422,
        "empty installation_id/device_name must be a validation error"
    );
}

// ── Refresh ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn mobile_refresh_happy_path_rotates_tokens() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let login = mobile_admin_token(&client, &base_url, INST_ID).await;
    let refresh_token = login["data"]["refresh_token"].as_str().unwrap().to_string();

    let resp = client
        .post(format!("{}/api/mobile/auth/refresh", base_url))
        .json(&json!({
            "refresh_token": refresh_token,
            "installation_id": INST_ID,
        }))
        .send()
        .await
        .expect("refresh request failed");
    assert_eq!(resp.status(), 200, "valid refresh should succeed");

    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["access_token"].as_str().is_some_and(|t| !t.is_empty()),
        "refresh must return a new access_token"
    );
    assert!(
        body["data"]["refresh_token"].as_str().is_some_and(|t| !t.is_empty()),
        "refresh must return a rotated refresh_token"
    );
}

#[tokio::test]
async fn mobile_refresh_invalid_token_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/auth/refresh", base_url))
        .json(&json!({
            "refresh_token": "sb_refresh_not_a_real_token",
            "installation_id": INST_ID,
        }))
        .send()
        .await
        .expect("refresh request failed");
    assert_eq!(resp.status(), 401, "bogus refresh token must be unauthorized");
}

#[tokio::test]
async fn mobile_refresh_wrong_installation_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let login = mobile_admin_token(&client, &base_url, INST_ID).await;
    let refresh_token = login["data"]["refresh_token"].as_str().unwrap().to_string();

    // A valid refresh token but for a different installation id must not match.
    let resp = client
        .post(format!("{}/api/mobile/auth/refresh", base_url))
        .json(&json!({
            "refresh_token": refresh_token,
            "installation_id": "some-other-installation",
        }))
        .send()
        .await
        .expect("refresh request failed");
    assert_eq!(
        resp.status(),
        401,
        "refresh token must be bound to its installation id"
    );
}

#[tokio::test]
async fn mobile_refresh_missing_fields_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/auth/refresh", base_url))
        .json(&json!({ "refresh_token": "", "installation_id": "" }))
        .send()
        .await
        .expect("refresh request failed");
    assert_eq!(resp.status(), 422, "empty refresh fields must be a validation error");
}

// ── Logout ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn mobile_logout_happy_path_invalidates_session() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let login = mobile_admin_token(&client, &base_url, INST_ID).await;
    let access_token = login["data"]["access_token"].as_str().unwrap().to_string();

    // A fresh client carries no session cookie, so only the bearer token authenticates.
    let bearer = http_client();
    let resp = bearer
        .post(format!("{}/api/mobile/auth/logout", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("logout request failed");
    assert_eq!(resp.status(), 200, "logout with a valid bearer token should succeed");

    // After logout the access token must no longer authenticate protected routes.
    let after = bearer
        .get(format!("{}/api/mobile/auth/devices", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("devices request failed");
    assert_eq!(after.status(), 401, "the token must be rejected after logout");
}

#[tokio::test]
async fn mobile_logout_without_token_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/auth/logout", base_url))
        .send()
        .await
        .expect("logout request failed");
    assert_eq!(resp.status(), 401, "logout without a bearer token must be unauthorized");
}

#[tokio::test]
async fn mobile_logout_bogus_token_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/auth/logout", base_url))
        .header("Authorization", "Bearer sb_not_a_session")
        .send()
        .await
        .expect("logout request failed");
    assert_eq!(resp.status(), 401, "unknown bearer token must be unauthorized");
}

// ── Device listing / revocation ───────────────────────────────────────────────

#[tokio::test]
async fn mobile_list_devices_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let login = mobile_admin_token(&client, &base_url, INST_ID).await;
    let access_token = login["data"]["access_token"].as_str().unwrap().to_string();

    let bearer = http_client();
    let resp = bearer
        .get(format!("{}/api/mobile/auth/devices", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("devices request failed");
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let devices = body["data"].as_array().expect("devices must be an array");
    assert_eq!(devices.len(), 1, "the just-logged-in device must be listed");
    assert_eq!(devices[0]["installation_id"], INST_ID);
    assert_eq!(devices[0]["device_name"], DEVICE_NAME);
    assert!(devices[0]["id"].as_str().is_some_and(|s| !s.is_empty()));
}

#[tokio::test]
async fn mobile_list_devices_accepts_session_cookie() {
    // list_devices is a generic protected route, so a browser session cookie
    // (stored on `client` by login_admin) is also accepted by auth_middleware.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/mobile/auth/devices", base_url))
        .send()
        .await
        .expect("devices request failed");
    assert_eq!(resp.status(), 200, "session cookie should authenticate list_devices");
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].is_array());
}

#[tokio::test]
async fn mobile_list_devices_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/mobile/auth/devices", base_url))
        .send()
        .await
        .expect("devices request failed");
    assert_eq!(resp.status(), 401, "unauthenticated device listing must be 401");
}

#[tokio::test]
async fn mobile_revoke_device_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let login = mobile_admin_token(&client, &base_url, INST_ID).await;
    let access_token = login["data"]["access_token"].as_str().unwrap().to_string();

    let bearer = http_client();
    // Discover the mobile session id.
    let list: Value = bearer
        .get(format!("{}/api/mobile/auth/devices", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("devices request failed")
        .json()
        .await
        .unwrap();
    let device_id = list["data"][0]["id"].as_str().unwrap().to_string();

    let resp = bearer
        .delete(format!("{}/api/mobile/auth/devices/{device_id}", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("revoke request failed");
    assert_eq!(resp.status(), 200, "revoking an owned device should succeed");
}

#[tokio::test]
async fn mobile_revoke_device_not_found_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let login = mobile_admin_token(&client, &base_url, INST_ID).await;
    let access_token = login["data"]["access_token"].as_str().unwrap().to_string();

    let bearer = http_client();
    let resp = bearer
        .delete(format!("{}/api/mobile/auth/devices/does-not-exist", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("revoke request failed");
    assert_eq!(resp.status(), 404, "revoking a missing session must be 404");
}

#[tokio::test]
async fn mobile_revoke_device_other_user_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    // Create a second member account so we can take over its mobile session.
    login_as_new_user(&admin, &base_url, "bob", "member").await;

    // bob logs in over mobile and owns a device/session.
    let client = http_client();
    let bob_login = mobile_login(&client, &base_url, "bob", "memberpass", "inst-bob").await;
    assert_eq!(bob_login.status(), 200);
    let bob_token: Value = bob_login.json().await.unwrap();
    let bob_access = bob_token["data"]["access_token"].as_str().unwrap().to_string();

    let bob_bearer = http_client();
    let bob_devices: Value = bob_bearer
        .get(format!("{}/api/mobile/auth/devices", base_url))
        .header("Authorization", format!("Bearer {bob_access}"))
        .send()
        .await
        .expect("devices request failed")
        .json()
        .await
        .unwrap();
    let bob_session_id = bob_devices["data"][0]["id"].as_str().unwrap().to_string();

    // admin logs in over mobile and tries to revoke bob's session id.
    let admin_token = mobile_admin_token(&client, &base_url, "inst-admin").await;
    let admin_access = admin_token["data"]["access_token"].as_str().unwrap().to_string();

    let admin_bearer = http_client();
    let resp = admin_bearer
        .delete(format!("{}/api/mobile/auth/devices/{bob_session_id}", base_url))
        .header("Authorization", format!("Bearer {admin_access}"))
        .send()
        .await
        .expect("revoke request failed");
    assert_eq!(
        resp.status(),
        403,
        "revoking another user's device must be forbidden"
    );
}

#[tokio::test]
async fn mobile_revoke_device_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .delete(format!("{}/api/mobile/auth/devices/anything", base_url))
        .send()
        .await
        .expect("revoke request failed");
    assert_eq!(resp.status(), 401, "unauthenticated revoke must be 401");
}

// ── Pairing flow (desktop generates code, mobile redeems) ─────────────────────

#[tokio::test]
async fn mobile_pair_generate_and_redeem_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    // Generate a pairing code via the browser session (admin).
    login_admin(&client, &base_url).await;

    let pair_resp: Value = client
        .post(format!("{}/api/mobile/pair", base_url))
        .send()
        .await
        .expect("generate pair request failed")
        .json()
        .await
        .unwrap();
    let code = pair_resp["data"]["code"].as_str().expect("code missing").to_string();
    assert!(code.starts_with("sb_pair_"), "code must carry the pairing prefix");
    assert!(pair_resp["data"]["expires_in_secs"].as_i64().is_some());

    // Redeem the code from a fresh (unauthenticated) mobile client.
    let mobile = http_client();
    let resp = mobile
        .post(format!("{}/api/mobile/auth/pair", base_url))
        .json(&json!({
            "code": code,
            "installation_id": "inst-paired",
            "device_name": "Paired iPhone",
        }))
        .send()
        .await
        .expect("redeem request failed");
    assert_eq!(resp.status(), 200, "redeeming a fresh code should succeed");

    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["access_token"].as_str().is_some_and(|t| !t.is_empty()),
        "pairing must mint an access token"
    );
    assert_eq!(body["data"]["user"]["username"], "admin");
}

#[tokio::test]
async fn mobile_pair_generate_requires_auth_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/pair", base_url))
        .send()
        .await
        .expect("generate pair request failed");
    assert_eq!(resp.status(), 401, "generating a pair code requires authentication");
}

#[tokio::test]
async fn mobile_pair_redeem_invalid_code_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/auth/pair", base_url))
        .json(&json!({
            "code": "sb_pair_never_issued",
            "installation_id": "inst-x",
            "device_name": "Phone",
        }))
        .send()
        .await
        .expect("redeem request failed");
    assert_eq!(resp.status(), 400, "an unknown pairing code must be a bad request");
}

#[tokio::test]
async fn mobile_pair_redeem_missing_fields_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/auth/pair", base_url))
        .json(&json!({ "code": "", "installation_id": "", "device_name": "" }))
        .send()
        .await
        .expect("redeem request failed");
    assert_eq!(resp.status(), 422, "empty pairing fields must be a validation error");
}

#[tokio::test]
async fn mobile_pair_code_is_single_use() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let pair_resp: Value = client
        .post(format!("{}/api/mobile/pair", base_url))
        .send()
        .await
        .expect("generate pair request failed")
        .json()
        .await
        .unwrap();
    let code = pair_resp["data"]["code"].as_str().unwrap().to_string();

    let mobile = http_client();
    let first = mobile
        .post(format!("{}/api/mobile/auth/pair", base_url))
        .json(&json!({ "code": code, "installation_id": "inst-a", "device_name": "A" }))
        .send()
        .await
        .expect("redeem request failed");
    assert_eq!(first.status(), 200, "first redemption should succeed");

    // The code is removed on first use, so a replay must fail.
    let second = mobile
        .post(format!("{}/api/mobile/auth/pair", base_url))
        .json(&json!({ "code": code, "installation_id": "inst-b", "device_name": "B" }))
        .send()
        .await
        .expect("redeem request failed");
    assert_eq!(second.status(), 400, "replaying a consumed code must fail");
}

// ── Push token registration ───────────────────────────────────────────────────
//
// push_register/unregister only persist (or delete) a device_token row keyed on
// the mobile session's installation id; they never contact APNs. Sending an
// actual APNs push (which requires real APNs credentials and reachability) is
// NOT exercised here — see the skipped note in the suite summary.

#[tokio::test]
async fn mobile_push_register_and_unregister_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let login = mobile_admin_token(&client, &base_url, INST_ID).await;
    let access_token = login["data"]["access_token"].as_str().unwrap().to_string();

    let bearer = http_client();
    let reg = bearer
        .post(format!("{}/api/mobile/push/register", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&json!({ "device_token": "apns-device-token-abc123" }))
        .send()
        .await
        .expect("push register request failed");
    assert_eq!(reg.status(), 200, "registering a device token should succeed");

    // Re-registering the same installation (upsert) must also succeed.
    let reg2 = bearer
        .post(format!("{}/api/mobile/push/register", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&json!({ "device_token": "apns-device-token-rotated" }))
        .send()
        .await
        .expect("push re-register request failed");
    assert_eq!(reg2.status(), 200, "re-registering (upsert) should succeed");

    let unreg = bearer
        .post(format!("{}/api/mobile/push/unregister", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .expect("push unregister request failed");
    assert_eq!(unreg.status(), 200, "unregistering a device token should succeed");
}

#[tokio::test]
async fn mobile_push_register_missing_token_is_422() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let login = mobile_admin_token(&client, &base_url, INST_ID).await;
    let access_token = login["data"]["access_token"].as_str().unwrap().to_string();

    let bearer = http_client();
    let resp = bearer
        .post(format!("{}/api/mobile/push/register", base_url))
        .header("Authorization", format!("Bearer {access_token}"))
        .json(&json!({ "device_token": "" }))
        .send()
        .await
        .expect("push register request failed");
    assert_eq!(resp.status(), 422, "empty device_token must be a validation error");
}

#[tokio::test]
async fn mobile_push_register_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/push/register", base_url))
        .json(&json!({ "device_token": "apns-token" }))
        .send()
        .await
        .expect("push register request failed");
    assert_eq!(resp.status(), 401, "unauthenticated push register must be 401");
}

#[tokio::test]
async fn mobile_push_unregister_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/mobile/push/unregister", base_url))
        .send()
        .await
        .expect("push unregister request failed");
    assert_eq!(resp.status(), 401, "unauthenticated push unregister must be 401");
}

#[tokio::test]
async fn mobile_push_register_with_api_key_is_401() {
    // push_register requires a *mobile* bearer session (it reads
    // session.mobile_session_id). An API-key authenticated request is not a
    // mobile session, so the endpoint rejects it as Unauthorized (401). A
    // pending server is seeded here purely to exercise a non-mobile
    // authenticated context end to end.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let _server_id = create_server(&client, &base_url, "push-ctx-server").await;

    let key_body: Value = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "mobile-test-key" }))
        .send()
        .await
        .expect("api-key request failed")
        .json()
        .await
        .unwrap();
    let api_key = key_body["data"]["key"].as_str().expect("api key missing").to_string();

    let fresh = http_client();
    let resp = fresh
        .post(format!("{}/api/mobile/push/register", base_url))
        .header("X-API-Key", api_key)
        .json(&json!({ "device_token": "apns-token" }))
        .send()
        .await
        .expect("push register request failed");
    assert_eq!(
        resp.status(),
        401,
        "an API-key request is not a mobile session, so push_register must 401"
    );
}
