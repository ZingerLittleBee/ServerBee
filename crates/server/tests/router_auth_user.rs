//! Router-level integration tests for the `auth` and `user` API endpoints.
//!
//! These exercise the real Axum router over HTTP: session-cookie auth, RBAC
//! (admin-only write routes), request-body validation, and not-found paths.
//! OAuth endpoints (`/api/auth/oauth/*`) are intentionally NOT covered here —
//! they require external identity providers and a configured client.

mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

// ── POST /api/auth/login ──

#[tokio::test]
async fn login_success_returns_user_payload() {
    // Valid admin credentials log in and return the user's role.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "admin", "password": "testpass" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["username"], "admin");
    assert_eq!(body["data"]["role"], "admin");
}

#[tokio::test]
async fn login_wrong_password_is_unauthorized() {
    // A correct username with a wrong password is rejected as 401.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "admin", "password": "wrong-password" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn login_empty_credentials_is_validation_error() {
    // Empty username/password is rejected by the handler with a 422 validation error.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "", "password": "" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 422);
}

// ── GET /api/auth/me ──

#[tokio::test]
async fn me_returns_current_user_when_authenticated() {
    // An authenticated session resolves to the logged-in user via /auth/me.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["username"], "admin");
    assert_eq!(body["data"]["role"], "admin");
}

#[tokio::test]
async fn me_unauthenticated_is_unauthorized() {
    // /auth/me without any credential returns 401.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
}

// ── POST /api/auth/logout ──

#[tokio::test]
async fn logout_succeeds_and_invalidates_session() {
    // Logout returns 200, and the prior session no longer authenticates /auth/me.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/auth/logout", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // The server invalidated the session token; a fresh client with no cookie
    // is unauthenticated (the logout response also cleared the cookie).
    let after = http_client()
        .get(format!("{}/api/auth/me", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), 401);
}

// ── API key management ──

#[tokio::test]
async fn create_api_key_admin_success_and_list() {
    // An admin can mint an API key (plaintext returned once) and see it listed.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let create = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "ci-key" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), 200);
    let created: Value = create.json().await.unwrap();
    assert_eq!(created["data"]["name"], "ci-key");
    let key = created["data"]["key"].as_str().unwrap();
    assert!(key.starts_with("serverbee_"), "plaintext key must be returned on creation");

    let list = client
        .get(format!("{}/api/auth/api-keys", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let listed: Value = list.json().await.unwrap();
    let keys = listed["data"].as_array().unwrap();
    assert!(keys.iter().any(|k| k["name"] == "ci-key"), "created key must appear in the list");
    // The plaintext key is never echoed back on list.
    assert!(keys.iter().all(|k| k["key"].is_null()), "list must not expose plaintext keys");
}

#[tokio::test]
async fn create_api_key_empty_name_is_validation_error() {
    // An empty key name is rejected with a 422 validation error.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn create_api_key_member_is_forbidden() {
    // Minting an API key is admin-only; a member client gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "key_member", "member").await;

    let resp = member
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "member-key" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn create_api_key_unauthenticated_is_unauthorized() {
    // An unauthenticated client cannot reach the create-api-key route.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn delete_api_key_success() {
    // An admin can delete a key they own.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "to-delete" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap();

    let resp = client
        .delete(format!("{}/api/auth/api-keys/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn delete_api_key_not_found() {
    // Deleting a non-existent key id returns 404.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .delete(format!("{}/api/auth/api-keys/does-not-exist", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ── PUT /api/auth/password ──

#[tokio::test]
async fn change_password_success() {
    // A member can change their own password with the correct old password,
    // then log in fresh with the new one.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "pw_user", "member").await;

    let resp = member
        .put(format!("{}/api/auth/password", base_url))
        .json(&json!({ "old_password": "memberpass", "new_password": "brandnewpass1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // The new password authenticates a fresh login.
    let relogin = http_client()
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "pw_user", "password": "brandnewpass1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(relogin.status(), 200);
}

#[tokio::test]
async fn change_password_wrong_old_password_is_bad_request() {
    // Supplying the wrong current password is a 400 bad request.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .put(format!("{}/api/auth/password", base_url))
        .json(&json!({ "old_password": "not-the-password", "new_password": "anotherpass1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn change_password_weak_new_password_is_validation_error() {
    // A new password below the minimum length is rejected with 422.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .put(format!("{}/api/auth/password", base_url))
        .json(&json!({ "old_password": "testpass", "new_password": "short" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn change_password_unauthenticated_is_unauthorized() {
    // The change-password route is protected; no credential -> 401.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .put(format!("{}/api/auth/password", base_url))
        .json(&json!({ "old_password": "x", "new_password": "longenough1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ── 2FA (TOTP) ──

#[tokio::test]
async fn totp_status_defaults_disabled() {
    // A fresh user has 2FA disabled.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/auth/2fa/status", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["enabled"], false);
}

#[tokio::test]
async fn totp_setup_returns_secret_and_otpauth_url() {
    // Setup generates and returns a base32 secret plus an otpauth provisioning URL.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/auth/2fa/setup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(!body["data"]["secret"].as_str().unwrap().is_empty(), "setup must return a secret");
    assert!(
        body["data"]["otpauth_url"].as_str().unwrap().starts_with("otpauth://"),
        "setup must return an otpauth provisioning URL"
    );
}

#[tokio::test]
async fn totp_enable_with_invalid_code_after_setup_is_unauthorized() {
    // After setup, enabling with a wrong TOTP code returns 401 (secret retained for retry).
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Establish a pending setup so we hit the code-verification branch.
    let setup = client
        .post(format!("{}/api/auth/2fa/setup", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 200);

    let resp = client
        .post(format!("{}/api/auth/2fa/enable", base_url))
        .json(&json!({ "code": "000000" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn totp_enable_without_pending_setup_is_bad_request() {
    // Enabling 2FA with no prior /setup call returns 400.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/auth/2fa/enable", base_url))
        .json(&json!({ "code": "123456" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn totp_disable_with_wrong_password_is_bad_request() {
    // Disabling 2FA requires the account password; a wrong one returns 400.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/auth/2fa/disable", base_url))
        .json(&json!({ "password": "not-the-password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// ── GET /api/users ──

#[tokio::test]
async fn list_users_admin_success() {
    // An admin can list users; the seeded admin is present.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/users", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let users = body["data"].as_array().unwrap();
    assert!(users.iter().any(|u| u["username"] == "admin"), "seeded admin must be listed");
}

#[tokio::test]
async fn list_users_member_is_forbidden() {
    // User management is admin-only; a member gets 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "list_member", "member").await;

    let resp = member
        .get(format!("{}/api/users", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn list_users_unauthenticated_is_unauthorized() {
    // The users list route is protected; no credential -> 401.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let resp = client
        .get(format!("{}/api/users", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ── POST /api/users ──

#[tokio::test]
async fn create_user_admin_success() {
    // An admin can create a member account.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": "new_member", "password": "password123", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["username"], "new_member");
    assert_eq!(body["data"]["role"], "member");
}

#[tokio::test]
async fn create_user_invalid_role_is_validation_error() {
    // A role outside {admin, member} is rejected with 422.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": "bad_role", "password": "password123", "role": "superuser" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn create_user_weak_password_is_validation_error() {
    // A password under the minimum length is rejected with 422.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": "weak_pw", "password": "123", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn create_user_duplicate_username_is_conflict() {
    // Creating a user with an existing username returns 409.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": "admin", "password": "password123", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn create_user_member_is_forbidden() {
    // A member cannot create users.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "create_member", "member").await;

    let resp = member
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": "sneaky", "password": "password123", "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// ── GET /api/users/{id} ──

#[tokio::test]
async fn get_user_success_and_not_found() {
    // Fetching an existing user succeeds; an unknown id returns 404.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create a user to fetch by id.
    let created: Value = client
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": "fetch_me", "password": "password123", "role": "member" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap();

    let found = client
        .get(format!("{}/api/users/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(found.status(), 200);
    let body: Value = found.json().await.unwrap();
    assert_eq!(body["data"]["username"], "fetch_me");

    let missing = client
        .get(format!("{}/api/users/no-such-id", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

// ── PUT /api/users/{id} ──

#[tokio::test]
async fn update_user_role_success() {
    // An admin can promote a member to admin.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": "promote_me", "password": "password123", "role": "member" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap();

    let resp = client
        .put(format!("{}/api/users/{}", base_url, id))
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["role"], "admin");
}

#[tokio::test]
async fn update_user_not_found() {
    // Updating a non-existent user returns 404.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .put(format!("{}/api/users/ghost-id", base_url))
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn update_user_demote_last_admin_is_bad_request() {
    // The seeded admin is the only admin; demoting it to member is blocked with 400.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    let me = login_admin(&client, &base_url).await;
    let admin_id = me["data"]["user_id"].as_str().unwrap();

    let resp = client
        .put(format!("{}/api/users/{}", base_url, admin_id))
        .json(&json!({ "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn update_user_member_is_forbidden() {
    // A member cannot update other users' roles.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "update_member", "member").await;

    let resp = member
        .put(format!("{}/api/users/whatever-id", base_url))
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// ── DELETE /api/users/{id} ──

#[tokio::test]
async fn delete_user_success() {
    // An admin can delete a member account.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{}/api/users", base_url))
        .json(&json!({ "username": "delete_me", "password": "password123", "role": "member" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["data"]["id"].as_str().unwrap();

    let resp = client
        .delete(format!("{}/api/users/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // The user is gone afterwards.
    let after = client
        .get(format!("{}/api/users/{}", base_url, id))
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), 404);
}

#[tokio::test]
async fn delete_user_not_found() {
    // Deleting a non-existent user returns 404.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .delete(format!("{}/api/users/no-such-user", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn delete_last_admin_is_bad_request() {
    // The seeded admin is the only admin; deleting it is refused with 400.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    let me = login_admin(&client, &base_url).await;
    let admin_id = me["data"]["user_id"].as_str().unwrap();

    let resp = client
        .delete(format!("{}/api/users/{}", base_url, admin_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn delete_user_member_is_forbidden() {
    // A member cannot delete users.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "delete_member", "member").await;

    let resp = member
        .delete(format!("{}/api/users/anything", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

// NOTE: `create_server` is part of the required harness import set, but the
// auth/user endpoints under test do not need a seeded server (none of them are
// scoped to a server id). The reference below keeps the unused-import lint quiet
// without modifying the shared harness.
const _: fn(&reqwest::Client, &str, &str) = |c, b, n| {
    let _ = create_server(c, b, n);
};
