mod common;
#[path = "mobile_push/security.rs"]
mod security_push;

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use chrono::{Duration as ChronoDuration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectOptions, ConnectionTrait, Database, EntityTrait,
    QueryFilter, Set,
};
use sea_orm_migration::MigratorTrait;

use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::entity::{device_token, mobile_session, session};
use serverbee_server::error::AppError;
use serverbee_server::migration::Migrator;
use serverbee_server::router::api::mobile::{PushRegisterRequest, push_register, push_unregister};
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

/// Build an `AppState` backed by a fresh migrated temp SQLite database.
async fn test_state() -> (Arc<AppState>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let data_dir = tmp.path().to_str().unwrap().to_string();

    let config = AppConfig {
        server: ServerConfig {
            listen: "127.0.0.1:0".to_string(),
            data_dir: data_dir.clone(),
            trusted_proxies: Vec::new(),
        },
        database: DatabaseConfig {
            path: "test.db".to_string(),
            max_connections: 5,
        },
        auth: AuthConfig {
            session_ttl: 86400,
            secure_cookie: false,
            max_servers: 0,
        },
        ..AppConfig::default()
    };

    let db_url = format!("sqlite://{}/test.db?mode=rwc", data_dir);
    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(5);
    opt.sqlx_logging(false);
    let db = Database::connect(opt).await.expect("connect test db");
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .unwrap();
    Migrator::up(&db, None).await.expect("migrations");

    let state = AppState::new(db, config).await.expect("app state");
    (state, tmp)
}

async fn seed_mobile_session(state: &AppState, id: &str, user_id: &str, installation_id: &str) {
    let now = Utc::now();
    mobile_session::ActiveModel {
        id: Set(id.to_string()),
        user_id: Set(user_id.to_string()),
        refresh_token_hash: Set(format!("hash-{id}")),
        revocation_token_hash: Set(None),
        installation_id: Set(installation_id.to_string()),
        device_name: Set("iPhone".to_string()),
        created_at: Set(now),
        expires_at: Set(now + ChronoDuration::days(30)),
        last_used_at: Set(now),
    }
    .insert(&state.db)
    .await
    .expect("seed mobile_session");
}

async fn seed_session(state: &AppState, id: &str, user_id: &str, token: &str, mobile_id: &str) {
    let now = Utc::now();
    session::ActiveModel {
        id: Set(id.to_string()),
        user_id: Set(user_id.to_string()),
        // Sessions store the token hash; the bearer header still carries plaintext.
        token: Set(AuthService::hash_session_token(token)),
        ip: Set("127.0.0.1".to_string()),
        user_agent: Set("test".to_string()),
        expires_at: Set(now + ChronoDuration::days(1)),
        created_at: Set(now),
        source: Set("mobile".to_string()),
        mobile_session_id: Set(Some(mobile_id.to_string())),
    }
    .insert(&state.db)
    .await
    .expect("seed session");
}

fn bearer(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
    headers
}

/// A member must not be able to overwrite another user's push registration
/// by reusing (forging) the victim's installation_id.
#[tokio::test]
async fn push_register_rejects_cross_user_overwrite() {
    let (state, _tmp) = test_state().await;

    let alice = AuthService::create_user(&state.db, "alice", "pw", "member")
        .await
        .expect("create alice");
    let bob = AuthService::create_user(&state.db, "bob", "pw", "member")
        .await
        .expect("create bob");

    let shared_installation = "inst-shared";

    // Alice owns the device registration for the shared installation id.
    seed_mobile_session(&state, "ms-alice", &alice.id, shared_installation).await;
    seed_session(&state, "s-alice", &alice.id, "tok-alice", "ms-alice").await;
    device_token::ActiveModel {
        id: Set("dt-alice".to_string()),
        user_id: Set(alice.id.clone()),
        mobile_session_id: Set("ms-alice".to_string()),
        installation_id: Set(shared_installation.to_string()),
        token: Set("apns-alice".to_string()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
    }
    .insert(&state.db)
    .await
    .expect("seed alice device_token");

    // Bob has a mobile session that forged Alice's installation id.
    seed_mobile_session(&state, "ms-bob", &bob.id, shared_installation).await;
    seed_session(&state, "s-bob", &bob.id, "tok-bob", "ms-bob").await;

    // Bob attempts to take over the registration.
    let res = push_register(
        State(state.clone()),
        bearer("tok-bob"),
        Json(PushRegisterRequest {
            device_token: "apns-bob".to_string(),
        }),
    )
    .await;

    assert!(
        matches!(res, Err(AppError::Forbidden(_))),
        "cross-user push_register must be rejected with Forbidden, got {res:?}"
    );

    // Alice's row is untouched.
    let row = device_token::Entity::find()
        .filter(device_token::Column::InstallationId.eq(shared_installation))
        .one(&state.db)
        .await
        .unwrap()
        .expect("alice row still present");
    assert_eq!(row.user_id, alice.id);
    assert_eq!(row.token, "apns-alice");

    // The legitimate owner can still refresh their own token.
    let ok = push_register(
        State(state.clone()),
        bearer("tok-alice"),
        Json(PushRegisterRequest {
            device_token: "apns-alice-2".to_string(),
        }),
    )
    .await;
    assert!(ok.is_ok(), "owner refresh should succeed, got {ok:?}");

    let row = device_token::Entity::find()
        .filter(device_token::Column::InstallationId.eq(shared_installation))
        .one(&state.db)
        .await
        .unwrap()
        .expect("row present");
    assert_eq!(row.user_id, alice.id);
    assert_eq!(row.token, "apns-alice-2");
}

/// A member must not be able to delete another user's push registration by
/// reusing (forging) the victim's installation_id via push_unregister.
#[tokio::test]
async fn push_unregister_rejects_cross_user_delete() {
    let (state, _tmp) = test_state().await;

    let alice = AuthService::create_user(&state.db, "alice", "pw", "member")
        .await
        .expect("create alice");
    let bob = AuthService::create_user(&state.db, "bob", "pw", "member")
        .await
        .expect("create bob");

    let shared_installation = "inst-shared";

    seed_mobile_session(&state, "ms-alice", &alice.id, shared_installation).await;
    seed_session(&state, "s-alice", &alice.id, "tok-alice", "ms-alice").await;
    device_token::ActiveModel {
        id: Set("dt-alice".to_string()),
        user_id: Set(alice.id.clone()),
        mobile_session_id: Set("ms-alice".to_string()),
        installation_id: Set(shared_installation.to_string()),
        token: Set("apns-alice".to_string()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
    }
    .insert(&state.db)
    .await
    .expect("seed alice device_token");

    seed_mobile_session(&state, "ms-bob", &bob.id, shared_installation).await;
    seed_session(&state, "s-bob", &bob.id, "tok-bob", "ms-bob").await;

    // Bob attempts to unregister using Alice's installation id.
    let _ = push_unregister(State(state.clone()), bearer("tok-bob"))
        .await
        .expect("handler returns ok even when nothing is deleted");

    // Alice's row must still be there.
    let row = device_token::Entity::find()
        .filter(device_token::Column::InstallationId.eq(shared_installation))
        .one(&state.db)
        .await
        .unwrap();
    assert!(
        row.is_some(),
        "Alice's device_token must not be deleted by Bob's forged unregister"
    );

    // The legitimate owner can still unregister their own device.
    let _ = push_unregister(State(state.clone()), bearer("tok-alice"))
        .await
        .expect("owner unregister ok");
    let row = device_token::Entity::find()
        .filter(device_token::Column::InstallationId.eq(shared_installation))
        .one(&state.db)
        .await
        .unwrap();
    assert!(
        row.is_none(),
        "owner unregister should delete their own row"
    );
}

// Verified setup uses the real HTTP router, authentication and migrated SQLite.
// Only the external Relay verification response is substituted.
async fn setup_http() -> (String, Arc<AppState>, tempfile::TempDir) {
    use axum::{Router, routing::post};
    let relay = Router::new().route(
        "/v1/grants/inspect",
        post(|headers: HeaderMap| async move {
            let secret = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            let (status, token, expires) = match secret {
                "Bearer verified-fixture" => (200, "a".repeat(64), Utc::now().timestamp() + 3600),
                "Bearer mismatched-fixture" => (200, "b".repeat(64), Utc::now().timestamp() + 3600),
                "Bearer expired-fixture" => (200, "a".repeat(64), Utc::now().timestamp() - 1),
                _ => (403, "a".repeat(64), 0),
            };
            (
                axum::http::StatusCode::from_u16(status).unwrap(),
                Json(serde_json::json!({
                    "grant_id": "fixture-grant", "key_id": "fixture-key", "device_token": token,
                    "environment": "sandbox", "expires_at": expires
                })),
            )
        }),
    );
    let relay_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_url = format!("http://{}", relay_listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(relay_listener, relay).await.unwrap();
    });
    let (initial, tmp) = test_state().await;
    let mut config = initial.config.clone();
    config.push_relay.url = relay_url;
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    AuthService::create_user(&state.db, "admin", "testpass", "admin")
        .await
        .unwrap();
    AuthService::create_user(&state.db, "member", "testpass", "member")
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = serverbee_server::router::create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (base, state, tmp)
}

async fn login_http(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    installation: &str,
) -> serde_json::Value {
    let res = client.post(format!("{base}/api/mobile/auth/login")).json(&serde_json::json!({
        "username": name, "password": "testpass", "installation_id": installation, "device_name": "iPhone"
    })).send().await.unwrap();
    assert_eq!(res.status(), 200);
    res.json::<serde_json::Value>().await.unwrap()["data"].clone()
}

fn intent(security: bool, enabled: bool) -> serde_json::Value {
    serde_json::json!({"enabled":enabled, "alerts":true, "security":security, "task_failure":true, "task_success":false})
}
async fn preferences_http(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    revision: i64,
    prefs: serde_json::Value,
) -> reqwest::Response {
    client
        .put(format!("{base}/api/mobile/push/settings"))
        .bearer_auth(access)
        .json(&serde_json::json!({"expected_revision":revision, "preferences":prefs}))
        .send()
        .await
        .unwrap()
}
async fn setup_http_request(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    revision: i64,
    grant: &str,
) -> reqwest::Response {
    client.post(format!("{base}/api/mobile/push/verified-register")).bearer_auth(access).json(&serde_json::json!({
        "expected_revision":revision, "device_token":"a".repeat(64), "environment":"sandbox", "key_id":"fixture-key", "grant_id":"fixture-grant", "grant_token":grant, "content_key_id":"22222222-2222-4222-8222-222222222222", "content_key":"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "deployment_id":"https://serverbee.test"
    })).send().await.unwrap()
}
async fn status_http(client: &reqwest::Client, base: &str, access: &str) -> serde_json::Value {
    let res = client
        .get(format!("{base}/api/mobile/push/settings"))
        .bearer_auth(access)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    res.json::<serde_json::Value>().await.unwrap()["data"].clone()
}

#[tokio::test]
async fn verified_setup_requires_explicit_intent_and_preserves_refresh_binding() {
    let (base, state, _tmp) = setup_http().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "admin", "verified-install").await;
    let access = login["access_token"].as_str().unwrap();
    let initial = status_http(&client, &base, access).await;
    assert_eq!(initial["preferences"]["enabled"], false);
    assert_eq!(initial["registered"], false);
    assert_eq!(
        setup_http_request(&client, &base, access, 0, "verified-fixture")
            .await
            .status(),
        403
    );
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(true, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        setup_http_request(&client, &base, access, 1, "verified-fixture")
            .await
            .status(),
        200
    );
    let confirmed = status_http(&client, &base, access).await;
    assert_eq!(confirmed["registered"], true);
    assert_eq!(confirmed["revision"], 2);
    assert_eq!(confirmed["delivery_available"], true);
    assert!(confirmed.get("grant_token").is_none());
    assert!(
        device_token::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .is_empty(),
        "relay installations never enter legacy APNs fan-out"
    );
    let refresh = client
        .post(format!("{base}/api/mobile/auth/refresh"))
        .json(&serde_json::json!({
            "installation_id":"verified-install", "refresh_token":login["refresh_token"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), 200);
    let rotated: serde_json::Value = refresh.json().await.unwrap();
    let rotated = rotated["data"]["access_token"].as_str().unwrap();
    let after = status_http(&client, &base, rotated).await;
    assert_eq!(after["registered"], true);
    assert_eq!(after["revision"], 2);
    let logout = client
        .post(format!("{base}/api/mobile/auth/logout"))
        .bearer_auth(rotated)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), 200);
    assert!(
        serverbee_server::entity::mobile_push_registration::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn verified_setup_rejects_bad_grants_and_stale_saves_without_false_confirmation() {
    let (base, _, _tmp) = setup_http().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "member-install").await;
    let access = login["access_token"].as_str().unwrap();
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(true, true))
            .await
            .status(),
        403
    );
    assert_eq!(status_http(&client, &base, access).await["revision"], 0);
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    for grant in [
        "unverified-fixture",
        "mismatched-fixture",
        "expired-fixture",
    ] {
        assert_eq!(
            setup_http_request(&client, &base, access, 1, grant)
                .await
                .status(),
            403
        );
        let status = status_http(&client, &base, access).await;
        assert_eq!(status["registered"], false);
        assert_eq!(status["revision"], 1);
    }
    assert_eq!(
        setup_http_request(&client, &base, access, 0, "verified-fixture")
            .await
            .status(),
        409
    );
    assert_eq!(
        setup_http_request(&client, &base, access, 1, "verified-fixture")
            .await
            .status(),
        200
    );
    assert_eq!(
        preferences_http(&client, &base, access, 1, intent(false, false))
            .await
            .status(),
        409
    );
    assert_eq!(
        preferences_http(&client, &base, access, 2, intent(false, false))
            .await
            .status(),
        200
    );
    assert_eq!(
        status_http(&client, &base, access).await["registered"],
        false
    );
    assert_eq!(
        setup_http_request(&client, &base, access, 3, "verified-fixture")
            .await
            .status(),
        403
    );
}

#[tokio::test]
async fn demoted_administrator_can_save_permitted_subscriptions_and_disable_setup() {
    use serverbee_server::entity::mobile_push_registration as registration;

    for enabled in [true, false] {
        let (base, state, _tmp) = setup_http().await;
        let client = reqwest::Client::new();
        AuthService::create_user(&state.db, "operator", "testpass", "admin")
            .await
            .unwrap();
        let operator = login_http(&client, &base, "operator", "operator-install").await;
        let login = login_http(&client, &base, "admin", "demoted-install").await;
        let access = login["access_token"].as_str().unwrap();
        assert_eq!(login["user"]["role"], "admin");
        assert_eq!(
            preferences_http(&client, &base, access, 0, intent(true, true))
                .await
                .status(),
            200
        );
        assert_eq!(
            setup_http_request(&client, &base, access, 1, "verified-fixture")
                .await
                .status(),
            200
        );
        let before = status_http(&client, &base, access).await;
        assert_eq!(before["security_allowed"], true);
        assert_eq!(before["preferences"]["security"], true);
        assert_eq!(before["registered"], true);

        // Change the real persisted role through the authenticated user API.
        let demotion = client
            .put(format!(
                "{base}/api/users/{}",
                login["user"]["id"].as_str().unwrap()
            ))
            .bearer_auth(operator["access_token"].as_str().unwrap())
            .json(&serde_json::json!({"role":"member"}))
            .send()
            .await
            .unwrap();
        assert_eq!(demotion.status(), 200);
        let after = status_http(&client, &base, access).await;
        assert_eq!(after["security_allowed"], false);
        assert_eq!(after["revision"], 2);
        assert_eq!(after["preferences"]["security"], true);
        assert_eq!(after["registered"], true);
        // A stale admin draft is still forbidden, even when disabling setup.
        assert_eq!(
            preferences_http(&client, &base, access, 2, intent(true, enabled))
                .await
                .status(),
            403
        );
        assert_eq!(status_http(&client, &base, access).await, after);

        let mut permitted = intent(false, enabled);
        permitted["alerts"] = serde_json::json!(false);
        permitted["task_success"] = serde_json::json!(true);
        let saved = preferences_http(&client, &base, access, 2, permitted.clone()).await;
        assert_eq!(saved.status(), 200);
        let saved = saved.json::<serde_json::Value>().await.unwrap()["data"].clone();
        assert_eq!(saved["preferences"], permitted);
        assert_eq!(saved["revision"], 3);
        assert_eq!(saved["security_allowed"], false);
        assert_eq!(saved["registered"], enabled);
        assert_eq!(saved["delivery_available"], false);
        assert_eq!(status_http(&client, &base, access).await, saved);
        let row = registration::Entity::find_by_id("demoted-install")
            .one(&state.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.user_id, login["user"]["id"].as_str().unwrap());
        assert_eq!(row.revision, 3);
        assert_eq!(row.enabled, enabled);
        assert!(!row.security);
        assert!(!row.alerts);
        assert!(row.task_failure);
        assert!(row.task_success);
        assert_eq!(row.grant_token.is_some(), enabled);
        assert_eq!(row.grant_id.is_some(), enabled);
        assert_eq!(row.grant_expires_at.is_some(), enabled);
    }
}

#[tokio::test]
async fn verified_setup_rejects_forged_installation_and_scopes_cleanup() {
    let (base, _, _tmp) = setup_http().await;
    let client = reqwest::Client::new();
    let owner = login_http(&client, &base, "admin", "shared-install").await;
    let access = owner["access_token"].as_str().unwrap();
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(true, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        setup_http_request(&client, &base, access, 1, "verified-fixture")
            .await
            .status(),
        200
    );
    let attacker = login_http(&client, &base, "member", "shared-install").await;
    let attacker = attacker["access_token"].as_str().unwrap();
    assert_eq!(
        preferences_http(&client, &base, attacker, 2, intent(false, true))
            .await
            .status(),
        403
    );
    assert_eq!(
        setup_http_request(&client, &base, attacker, 2, "verified-fixture")
            .await
            .status(),
        403
    );
    let unregister = client
        .post(format!("{base}/api/mobile/push/unregister"))
        .bearer_auth(attacker)
        .send()
        .await
        .unwrap();
    assert_eq!(unregister.status(), 200);
    assert_eq!(
        status_http(&client, &base, access).await["registered"],
        true
    );
}

#[tokio::test]
async fn verified_setup_survives_database_reopen_and_device_revocation_cascades() {
    let (base, state, tmp) = setup_http().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "admin", "persisted-install").await;
    let access = login["access_token"].as_str().unwrap();
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(true, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        setup_http_request(&client, &base, access, 1, "verified-fixture")
            .await
            .status(),
        200
    );
    let reopened = Database::connect(format!(
        "sqlite://{}/test.db?mode=rwc",
        tmp.path().display()
    ))
    .await
    .unwrap();
    let row =
        serverbee_server::entity::mobile_push_registration::Entity::find_by_id("persisted-install")
            .one(&reopened)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(row.environment.as_deref(), Some("sandbox"));
    assert_eq!(row.revision, 2);
    let res = client
        .delete(format!(
            "{base}/api/mobile/auth/devices/{}",
            row.mobile_session_id
        ))
        .bearer_auth(access)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(
        serverbee_server::entity::mobile_push_registration::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn verified_setup_revalidates_logout_after_delayed_relay_inspection() {
    use axum::{Router, routing::post};
    let (base, state, _tmp) = setup_http().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "delayed-install").await;
    let access = login["access_token"].as_str().unwrap().to_owned();
    assert_eq!(
        preferences_http(&client, &base, &access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let relay_started = started.clone();
    let relay_release = release.clone();
    let relay = Router::new().route("/v1/grants/inspect", post(move || {
        let started = relay_started.clone();
        let release = relay_release.clone();
        async move {
            started.notify_one();
            release.notified().await;
            Json(serde_json::json!({"grant_id":"fixture-grant", "key_id":"fixture-key", "device_token":"a".repeat(64), "environment":"sandbox", "expires_at":Utc::now().timestamp()+3600}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = state.config.clone();
    config.push_relay.url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, relay).await.unwrap();
    });
    let delayed_state = AppState::new(state.db.clone(), config).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let delayed_base = format!("http://{}", listener.local_addr().unwrap());
    let app = serverbee_server::router::create_router(delayed_state);
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let registering_client = client.clone();
    let registering_access = access.clone();
    let request = tokio::spawn(async move {
        setup_http_request(
            &registering_client,
            &delayed_base,
            &registering_access,
            1,
            "held-fixture",
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
        .await
        .unwrap();
    let logout = client
        .post(format!("{base}/api/mobile/auth/logout"))
        .bearer_auth(&access)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), 200);
    release.notify_one();
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), request)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), 401);
    assert!(
        serverbee_server::entity::mobile_push_registration::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn rotated_relay_grant_save_failure_remains_unconfirmed_after_restart() {
    use axum::{Router, routing::post};
    use std::sync::atomic::{AtomicBool, Ordering};
    let (_, initial, tmp) = setup_http().await;
    let rotated = Arc::new(AtomicBool::new(false));
    let relay_rotation = rotated.clone();
    let relay = Router::new().route("/v1/grants/inspect", post(move |headers: HeaderMap| {
        let rotated = relay_rotation.clone();
        async move {
            let is_new = rotated.load(Ordering::SeqCst);
            let expected = if is_new { "Bearer renewed-fixture" } else { "Bearer verified-fixture" };
            let status = if headers.get("authorization").and_then(|value| value.to_str().ok()) == Some(expected) {
                axum::http::StatusCode::OK
            } else { axum::http::StatusCode::FORBIDDEN };
            (status, Json(serde_json::json!({
                "grant_id":"fixture-grant", "key_id":"fixture-key", "device_token":"a".repeat(64),
                "environment":"sandbox", "expires_at":Utc::now().timestamp()+86400
            })))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = initial.config.clone();
    config.push_relay.url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, relay).await.unwrap();
    });
    let state = AppState::new(initial.db.clone(), config.clone())
        .await
        .unwrap();
    let base = serve_setup_state(state).await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "rotation-install").await;
    let access = login["access_token"].as_str().unwrap();
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        setup_http_request(&client, &base, access, 1, "verified-fixture")
            .await
            .status(),
        200
    );
    assert_eq!(
        status_http(&client, &base, access).await["registered"],
        true
    );
    // The external Relay rotates, then a stale Server revision rejects the save.
    rotated.store(true, Ordering::SeqCst);
    assert_eq!(
        setup_http_request(&client, &base, access, 1, "renewed-fixture")
            .await
            .status(),
        409
    );
    let failed = status_http(&client, &base, access).await;
    assert_eq!(failed["registered"], false);
    assert_eq!(failed["revision"], 2);
    assert_eq!(failed["preferences"]["enabled"], true);
    let db = Database::connect(format!(
        "sqlite://{}/test.db?mode=rwc",
        tmp.path().display()
    ))
    .await
    .unwrap();
    let restarted = AppState::new(db, config).await.unwrap();
    let base = serve_setup_state(restarted).await;
    assert_eq!(
        status_http(&client, &base, access).await["registered"],
        false
    );
    // Recovery reuses the rotated grant with the freshly confirmed revision.
    assert_eq!(
        setup_http_request(&client, &base, access, 2, "renewed-fixture")
            .await
            .status(),
        200
    );
    let recovered = status_http(&client, &base, access).await;
    assert_eq!(recovered["registered"], true);
    assert_eq!(recovered["revision"], 3);
}

async fn serve_setup_state(state: Arc<AppState>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let _worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    let app = serverbee_server::router::create_router(state);
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    base
}

#[tokio::test]
async fn one_server_and_relay_url_keep_sandbox_and_production_bindings_independent() {
    use axum::{Router, routing::post};
    let (_, initial, _tmp) = setup_http().await;
    let relay = Router::new().route("/v1/grants/inspect", post(|headers: HeaderMap| async move {
        let secret = headers.get("authorization").and_then(|value| value.to_str().ok());
        let environment = match secret {
            Some("Bearer sandbox-fixture") => "sandbox",
            Some("Bearer production-fixture") => "production",
            _ => return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({}))),
        };
        (axum::http::StatusCode::OK, Json(serde_json::json!({
            "grant_id":format!("{environment}-grant"), "key_id":format!("{environment}-key"),
            "device_token":if environment == "sandbox" {"a".repeat(64)} else {"b".repeat(64)},
            "environment":environment, "expires_at":Utc::now().timestamp()+86400
        })))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = initial.config.clone();
    config.push_relay.url = format!("http://{}", listener.local_addr().unwrap());
    let relay_url = config.push_relay.url.clone();
    tokio::spawn(async move {
        axum::serve(listener, relay).await.unwrap();
    });
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    let base = serve_setup_state(state.clone()).await;
    let client = reqwest::Client::new();
    let sandbox = login_http(&client, &base, "admin", "sandbox-install").await;
    let production = login_http(&client, &base, "admin", "production-install").await;
    let dev = sandbox["access_token"].as_str().unwrap();
    let prod = production["access_token"].as_str().unwrap();
    for access in [dev, prod] {
        assert_eq!(
            preferences_http(&client, &base, access, 0, intent(true, true))
                .await
                .status(),
            200
        );
        assert_eq!(
            status_http(&client, &base, access).await["relay_url"],
            relay_url
        );
    }
    let registration = |environment: &str| {
        serde_json::json!({
            "expected_revision":1, "device_token":if environment == "sandbox" {"a".repeat(64)} else {"b".repeat(64)},
            "environment":environment, "key_id":format!("{environment}-key"), "grant_id":format!("{environment}-grant"),
            "grant_token":format!("{environment}-fixture"), "content_key_id":"22222222-2222-4222-8222-222222222222", "content_key":"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "deployment_id":"https://serverbee.test"
        })
    };
    let mut wrong = registration("sandbox");
    wrong["grant_token"] = serde_json::json!("production-fixture");
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/verified-register"))
            .bearer_auth(dev)
            .json(&wrong)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    for (access, environment) in [(dev, "sandbox"), (prod, "production")] {
        assert_eq!(
            client
                .post(format!("{base}/api/mobile/push/verified-register"))
                .bearer_auth(access)
                .json(&registration(environment))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            status_http(&client, &base, access).await["registered"],
            true
        );
    }
    let rows = serverbee_server::entity::mobile_push_registration::Entity::find()
        .all(&state.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    for (installation, environment) in [
        ("sandbox-install", "sandbox"),
        ("production-install", "production"),
    ] {
        let row = rows
            .iter()
            .find(|row| row.installation_id == installation)
            .unwrap();
        assert_eq!(row.environment.as_deref(), Some(environment));
        assert_eq!(
            row.key_id.as_deref(),
            Some(format!("{environment}-key").as_str())
        );
        assert_eq!(row.revision, 2);
    }
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/auth/logout"))
            .bearer_auth(dev)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(status_http(&client, &base, prod).await["registered"], true);
    let rows = serverbee_server::entity::mobile_push_registration::Entity::find()
        .all(&state.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].environment.as_deref(), Some("production"));
}

struct DeliveryRelayFixture {
    child: std::process::Child,
    directory: tempfile::TempDir,
    ready: serde_json::Value,
}
impl Drop for DeliveryRelayFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl DeliveryRelayFixture {
    async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let child = std::process::Command::new("bun")
            .arg(root.join("apps/push-relay/tests/serve-delivery-fixture.ts"))
            .arg(directory.path())
            .current_dir(&root)
            .spawn()
            .expect("Bun fixture requires installed Relay dependencies and OpenSSL 3");
        let mut result = Self {
            child,
            directory,
            ready: serde_json::Value::Null,
        };
        for _ in 0..250 {
            if let Ok(bytes) = std::fs::read(result.path("ready.json")) {
                result.ready = serde_json::from_slice(&bytes).unwrap();
                return result;
            }
            assert!(
                result.child.try_wait().unwrap().is_none(),
                "Relay fixture exited before actual admission"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("Relay fixture admission timed out");
    }
    fn path(&self, file: &str) -> std::path::PathBuf {
        self.directory.path().join(file)
    }
    fn control(&self, value: serde_json::Value) {
        std::fs::write(
            self.path("provider.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
}

fn content_registration(grant: &serde_json::Value, revision: i64) -> serde_json::Value {
    serde_json::json!({"expected_revision":revision, "device_token":grant["device_token"], "environment":grant["environment"],
        "key_id":grant["key_id"], "grant_id":grant["grant_id"], "grant_token":grant["grant_token"],
        "content_key_id":"22222222-2222-4222-8222-222222222222", "content_key":"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=", "deployment_id":"https://serverbee.test"})
}
async fn enqueue_test(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    revision: i64,
    event: &str,
) -> reqwest::Response {
    client
        .post(format!("{base}/api/mobile/push/test"))
        .bearer_auth(access)
        .json(&serde_json::json!({"expected_revision":revision,"event_id":event}))
        .send()
        .await
        .unwrap()
}

async fn wait_test(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    event: &str,
    expected: &str,
) -> serde_json::Value {
    for _ in 0..400 {
        let response = client
            .get(format!("{base}/api/mobile/push/test/{event}"))
            .bearer_auth(access)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let data = response.json::<serde_json::Value>().await.unwrap()["data"].clone();
        if data["outcome"] == expected {
            return data;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("Delivery did not reach {expected}");
}

// Preserve the #199 tracer bullet through durable admission + production worker.
async fn post_test(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    revision: i64,
) -> reqwest::Response {
    let event = uuid::Uuid::new_v4().to_string();
    let queued = enqueue_test(client, base, access, revision, &event).await;
    if queued.status() != 200 {
        return queued;
    }
    assert_eq!(
        queued.json::<serde_json::Value>().await.unwrap()["data"]["outcome"],
        "pending"
    );
    for _ in 0..500 {
        let reply = client
            .get(format!("{base}/api/mobile/push/test/{event}"))
            .bearer_auth(access)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 200);
        let data = reply.json::<serde_json::Value>().await.unwrap();
        if data["data"]["outcome"] != "pending" {
            return client
                .get(format!("{base}/api/mobile/push/test/{event}"))
                .bearer_auth(access)
                .send()
                .await
                .unwrap();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("Production outbox worker did not deliver test");
}

#[tokio::test]
async fn encrypted_test_uses_real_relay_admission_transport_and_legacy_migration() {
    use serverbee_server::service::apns::ApnsService;
    let relay = DeliveryRelayFixture::start().await;
    let (_, initial, _tmp) = setup_http().await;
    let mut config = initial.config.clone();
    config.push_relay.url = relay.ready["url"].as_str().unwrap().to_owned();
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    let base = serve_setup_state(state.clone()).await;
    let client = reqwest::Client::new();
    let owner = login_http(&client, &base, "admin", "migrating-install").await;
    let other = login_http(&client, &base, "member", "legacy-install").await;
    let owner_other = login_http(&client, &base, "admin", "other-owner-install").await;
    let access = owner["access_token"].as_str().unwrap();
    // Seed through actual legacy HTTP registration, including another owner
    // and another installation of the same user, before verified migration.
    for (login, token) in [
        (&owner, "a".repeat(64)),
        (&other, "c".repeat(64)),
        (&owner_other, "d".repeat(64)),
    ] {
        assert_eq!(
            client
                .post(format!("{base}/api/mobile/push/register"))
                .bearer_auth(login["access_token"].as_str().unwrap())
                .json(&serde_json::json!({"device_token":token}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    assert_eq!(
        ApnsService::legacy_recipients(&state.db)
            .await
            .unwrap()
            .len(),
        3
    );
    assert_eq!(post_test(&client, &base, access, 0).await.status(), 403);
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(true, true))
            .await
            .status(),
        200
    );
    let registration = content_registration(&relay.ready["grant"], 1);
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/verified-register"))
            .bearer_auth(access)
            .json(&registration)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        status_http(&client, &base, access).await["test_available"],
        true
    );
    let selected = ApnsService::legacy_recipients(&state.db).await.unwrap();
    assert_eq!(selected.len(), 2);
    assert!(
        !selected
            .iter()
            .any(|row| row.installation_id == "migrating-install")
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/register"))
            .bearer_auth(access)
            .json(&serde_json::json!({"device_token":"a".repeat(64)}))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/test"))
            .bearer_auth(access)
            .json(&serde_json::json!({"expected_revision":2, "installation_id":"legacy-install"}))
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    assert_eq!(
        post_test(&client, &base, other["access_token"].as_str().unwrap(), 0)
            .await
            .status(),
        403
    );
    assert_eq!(post_test(&client, &base, access, 1).await.status(), 409);
    let sent = post_test(&client, &base, access, 2).await;
    assert_eq!(sent.status(), 200);
    let sent = sent.json::<serde_json::Value>().await.unwrap()["data"].clone();
    assert_eq!(sent["outcome"], "accepted");
    assert_eq!(sent["presentation"], "unobserved");
    let provider: serde_json::Value =
        serde_json::from_slice(&std::fs::read(relay.path("provider-request.json")).unwrap())
            .unwrap();
    assert_eq!(provider["token"], "a".repeat(64));
    assert_eq!(provider["environment"], "sandbox");
    let payload = provider["payload"].as_str().unwrap();
    for plaintext in [
        "deployment_id",
        "user_id",
        "installation_id",
        "content_key",
        "https://serverbee.test",
        "migrating-install",
    ] {
        assert!(!payload.contains(plaintext));
    }
    let payload: serde_json::Value = serde_json::from_str(payload).unwrap();
    assert_eq!(payload["aps"]["mutable-content"], 1);
    assert_eq!(provider["headers"]["apns-id"], sent["event_id"]);
    // Export only isolated fixture material for the actual Swift extension and
    // app tap tests. Nothing uses a live grant, Apple key or production service.
    if let Some(trace) = std::env::var_os("SERVERBEE_PUSH_TRACE_DIR") {
        std::fs::create_dir_all(&trace).unwrap();
        std::fs::write(std::path::Path::new(&trace).join("trace.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "payload":payload, "registration":registration, "user_id":owner["user"]["id"], "installation_id":"migrating-install", "event_id":sent["event_id"]
        })).unwrap()).unwrap();
    }
    let refreshed = client.post(format!("{base}/api/mobile/auth/refresh"))
        .json(&serde_json::json!({"installation_id":"migrating-install", "refresh_token":owner["refresh_token"]}))
        .send().await.unwrap();
    assert_eq!(refreshed.status(), 200);
    let refreshed: serde_json::Value = refreshed.json().await.unwrap();
    let access = refreshed["data"]["access_token"].as_str().unwrap();
    assert_eq!(
        post_test(&client, &base, access, 2)
            .await
            .json::<serde_json::Value>()
            .await
            .unwrap()["data"]["outcome"],
        "accepted"
    );
    relay.control(serde_json::json!({"status":400,"reason":"BadTopic"}));
    assert_eq!(
        post_test(&client, &base, access, 2)
            .await
            .json::<serde_json::Value>()
            .await
            .unwrap()["data"]["outcome"],
        "permanent"
    );
    assert_eq!(
        status_http(&client, &base, access).await["registered"],
        true
    );
    relay.control(serde_json::json!({"status":503}));
    assert_eq!(
        post_test(&client, &base, access, 2)
            .await
            .json::<serde_json::Value>()
            .await
            .unwrap()["data"]["outcome"],
        "retryable"
    );
    assert_eq!(
        status_http(&client, &base, access).await["registered"],
        true
    );
    assert_eq!(
        preferences_http(&client, &base, access, 2, intent(false, false))
            .await
            .status(),
        200
    );
    assert_eq!(post_test(&client, &base, access, 3).await.status(), 403);
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/register"))
            .bearer_auth(access)
            .json(&serde_json::json!({"device_token":"a".repeat(64)}))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/unregister"))
            .bearer_auth(access)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/register"))
            .bearer_auth(access)
            .json(&serde_json::json!({"device_token":"a".repeat(64)}))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    // Even a historical leftover restored to SQLite cannot enter the real
    // legacy selector. Legitimate other installations remain available.
    let restored_other = push_register(
        State(state.clone()),
        bearer(other["access_token"].as_str().unwrap()),
        Json(PushRegisterRequest {
            device_token: "e".repeat(64),
        }),
    )
    .await
    .unwrap();
    assert_eq!(restored_other.0.data, "ok");
    let migrated_session = mobile_session::Entity::find()
        .filter(mobile_session::Column::InstallationId.eq("migrating-install"))
        .filter(mobile_session::Column::UserId.eq(owner["user"]["id"].as_str().unwrap()))
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    device_token::ActiveModel {
        id: Set("restored-legacy".into()),
        user_id: Set(migrated_session.user_id.clone()),
        mobile_session_id: Set(migrated_session.id),
        installation_id: Set("migrating-install".into()),
        token: Set("a".repeat(64)),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
    }
    .insert(&state.db)
    .await
    .unwrap();
    assert_eq!(
        device_token::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .len(),
        3
    );
    let selected = ApnsService::legacy_recipients(&state.db).await.unwrap();
    assert_eq!(selected.len(), 2);
    assert!(selected.iter().any(|row| row.token == "e".repeat(64)));
    assert!(
        !selected
            .iter()
            .any(|row| row.installation_id == "migrating-install")
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/auth/logout"))
            .bearer_auth(access)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(post_test(&client, &base, access, 0).await.status(), 401);
    let relogin = login_http(&client, &base, "admin", "migrating-install").await;
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/register"))
            .bearer_auth(relogin["access_token"].as_str().unwrap())
            .json(&serde_json::json!({"device_token":"a".repeat(64)}))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
}

#[tokio::test]
async fn late_terminal_apns_response_cannot_invalidate_replacement_token_or_key() {
    let relay = DeliveryRelayFixture::start().await;
    let (_, initial, _tmp) = setup_http().await;
    let mut config = initial.config.clone();
    let relay_url = relay.ready["url"].as_str().unwrap();
    config.push_relay.url = relay_url.to_owned();
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    let base = serve_setup_state(state).await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "replacement-install").await;
    let access = login["access_token"].as_str().unwrap().to_owned();
    assert_eq!(
        preferences_http(&client, &base, &access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/verified-register"))
            .bearer_auth(&access)
            .json(&content_registration(&relay.ready["grant"], 1))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    relay
        .control(serde_json::json!({"status":410,"reason":"Unregistered","wait_for_release":true}));
    let sending = {
        let client = client.clone();
        let base = base.clone();
        let access = access.clone();
        tokio::spawn(async move { post_test(&client, &base, &access, 2).await })
    };
    for _ in 0..250 {
        if relay.path("provider-started.json").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(relay.path("provider-started.json").exists());
    let grant: serde_json::Value = client
        .post(format!("{relay_url}/fixture/renew"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut replacement = content_registration(&grant, 2);
    replacement["content_key_id"] = serde_json::json!("33333333-3333-4333-8333-333333333333");
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/verified-register"))
            .bearer_auth(&access)
            .json(&replacement)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    std::fs::write(relay.path("release"), "release").unwrap();
    assert_eq!(
        sending
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()["data"]["reason"],
        "Unregistered"
    );
    let after = status_http(&client, &base, &access).await;
    assert_eq!(after["registered"], true);
    assert_eq!(after["revision"], 3);
    relay.control(serde_json::json!({"status":410,"reason":"Unregistered"}));
    assert_eq!(
        post_test(&client, &base, &access, 3)
            .await
            .json::<serde_json::Value>()
            .await
            .unwrap()["data"]["outcome"],
        "permanent"
    );
    assert_eq!(
        status_http(&client, &base, &access).await["registered"],
        false
    );
    assert_eq!(post_test(&client, &base, &access, 4).await.status(), 403);
}

struct HeldLegacyApple {
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
    tokens: tokio::sync::Mutex<Vec<String>>,
}
#[async_trait::async_trait]
impl serverbee_server::service::apns::LegacyApnsTransport for HeldLegacyApple {
    async fn send(
        &self,
        payload: a2::request::payload::Payload<'_>,
    ) -> Result<a2::Response, a2::Error> {
        let first = {
            let mut tokens = self.tokens.lock().await;
            tokens.push(payload.device_token.to_owned());
            tokens.len() == 1
        };
        if first {
            self.started.notify_one();
            self.release.notified().await;
        }
        Ok(a2::Response {
            code: 200,
            error: None,
            apns_id: None,
        })
    }
}

#[tokio::test]
async fn legacy_dispatch_rechecks_cached_later_recipient_after_verified_migration() {
    use serverbee_server::service::apns::{ApnsConfig, ApnsService};
    let (base, state, _tmp) = setup_http().await;
    let client = reqwest::Client::new();
    let first = login_http(&client, &base, "member", "earlier-install").await;
    let migrating = login_http(&client, &base, "admin", "later-install").await;
    let first_access = first["access_token"].as_str().unwrap();
    let migrating_access = migrating["access_token"].as_str().unwrap();
    for (access, token) in [
        (first_access, "first-token"),
        (migrating_access, "later-token"),
    ] {
        assert_eq!(
            client
                .post(format!("{base}/api/mobile/push/register"))
                .bearer_auth(access)
                .json(&serde_json::json!({"device_token":token}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    // Another owner reuses that installation string. Migration must not silence
    // its legacy row or grant it access to the real owner's modern registration.
    let other = login_http(&client, &base, "member", "later-install").await;
    // The real HTTP API rejects an overwrite; an unrelated owner's marker
    // still cannot suppress the actual owner's selected row.
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/register"))
            .bearer_auth(other["access_token"].as_str().unwrap())
            .json(&serde_json::json!({"device_token":"attacker-token"}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let other_session = mobile_session::Entity::find()
        .filter(mobile_session::Column::UserId.eq(other["user"]["id"].as_str().unwrap()))
        .filter(mobile_session::Column::InstallationId.eq("later-install"))
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    // device_tokens has a unique installation index, so use a distinct legitimate
    // installation for the other owner while testing forged tombstones below.
    let third = login_http(&client, &base, "member", "unrelated-install").await;
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/register"))
            .bearer_auth(third["access_token"].as_str().unwrap())
            .json(&serde_json::json!({"device_token":"unrelated-token"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // A marker from the other user cannot suppress the actual owner.
    state
        .db
        .execute(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "INSERT INTO mobile_push_migrations (installation_id,user_id) VALUES (?,?)",
            ["later-install".into(), other_session.user_id.into()],
        ))
        .await
        .unwrap();
    assert_eq!(
        ApnsService::legacy_recipients(&state.db)
            .await
            .unwrap()
            .len(),
        3
    );
    let apple = Arc::new(HeldLegacyApple {
        started: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
        tokens: tokio::sync::Mutex::new(Vec::new()),
    });
    let sending = {
        let apple = apple.clone();
        let state = state.clone();
        tokio::spawn(async move {
            // Signing is irrelevant at this substituted Apple network boundary;
            // actual selection, loop, payload creation and revalidation run.
            let config = ApnsConfig {
                key_id: "fixture",
                team_id: "fixture",
                private_key: "fixture",
                bundle_id: "com.serverbee.mobile",
                sandbox: true,
            };
            ApnsService::send_push_with_transport(
                &state.db,
                &config,
                "Legacy event",
                "Body",
                None,
                None,
                apple.as_ref(),
            )
            .await
        })
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), apple.started.notified())
        .await
        .unwrap();
    assert_eq!(*apple.tokens.lock().await, ["first-token"]);
    assert_eq!(
        preferences_http(&client, &base, migrating_access, 0, intent(true, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        setup_http_request(&client, &base, migrating_access, 1, "verified-fixture")
            .await
            .status(),
        200
    );
    // This first request is already in flight. Migrating it now cannot retract
    // its provider receipt; the cached later request has not started and is skipped.
    assert_eq!(
        preferences_http(&client, &base, first_access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        setup_http_request(&client, &base, first_access, 1, "verified-fixture")
            .await
            .status(),
        200
    );
    apple.release.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(5), sending)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        *apple.tokens.lock().await,
        ["first-token", "unrelated-token"]
    );
    assert_eq!(
        ApnsService::legacy_recipients(&state.db)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn expired_mobile_session_cannot_send_a_registered_encrypted_test() {
    let relay = DeliveryRelayFixture::start().await;
    let (_, initial, _tmp) = setup_http().await;
    let mut config = initial.config.clone();
    config.push_relay.url = relay.ready["url"].as_str().unwrap().to_owned();
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    let base = serve_setup_state(state.clone()).await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "expired-install").await;
    let access = login["access_token"].as_str().unwrap();
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/verified-register"))
            .bearer_auth(access)
            .json(&content_registration(&relay.ready["grant"], 1))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // Advance only the persisted time boundary; real HTTP/session policy and
    // registration are not substituted. The access token remains unexpired.
    mobile_session::Entity::update_many()
        .col_expr(
            mobile_session::Column::ExpiresAt,
            sea_orm::sea_query::Expr::value(Utc::now() - ChronoDuration::seconds(1)),
        )
        .filter(mobile_session::Column::InstallationId.eq("expired-install"))
        .filter(mobile_session::Column::UserId.eq(login["user"]["id"].as_str().unwrap()))
        .exec(&state.db)
        .await
        .unwrap();
    assert_eq!(post_test(&client, &base, access, 2).await.status(), 401);
    assert!(!relay.path("provider-request.json").exists());
}

/// Substitute only the external Relay HTTP boundary. Internal policy, SQLite,
/// authentication, subscription writes, encryption and worker ownership are real.
struct OutboxRelay {
    url: String,
    status: Arc<std::sync::atomic::AtomicU16>,
    delay_ms: Arc<std::sync::atomic::AtomicU64>,
    requests: Arc<tokio::sync::Mutex<Vec<serde_json::Value>>>,
}
impl OutboxRelay {
    async fn start() -> Self {
        use axum::{Router, http::StatusCode, routing::post};
        let status = Arc::new(std::sync::atomic::AtomicU16::new(503));
        let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let delay_ms = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let send_delay = delay_ms.clone();
        let send_status = status.clone();
        let send_requests = requests.clone();
        let app = Router::new()
            .route("/v1/grants/inspect", post(|headers: HeaderMap| async move {
                let secret = headers.get("authorization").unwrap().to_str().unwrap();
                let token = if secret.ends_with("device-b") { "b" } else { "a" };
                Json(serde_json::json!({"grant_id":secret.trim_start_matches("Bearer "),"key_id":"fixture-key",
                    "device_token":token.repeat(64),"environment":"sandbox","expires_at":Utc::now().timestamp()+3600}))
            }))
            .route("/v1/send", post(move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                let status = send_status.clone(); let requests = send_requests.clone(); let delay = send_delay.clone();
                async move {
                    let grant = headers.get("authorization").unwrap().to_str().unwrap();
                    let code = if grant.ends_with("device-b") { 200 } else { status.load(std::sync::atomic::Ordering::SeqCst) };
                    requests.lock().await.push(body);
                    if !grant.ends_with("device-b") { tokio::time::sleep(std::time::Duration::from_millis(delay.load(std::sync::atomic::Ordering::SeqCst))).await; }
                    let outcome = if code == 200 { "accepted" } else { "retryable" };
                    (StatusCode::from_u16(code).unwrap(), Json(serde_json::json!({"outcome":outcome,"reason":"Accepted","device_invalid":false})))
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            url,
            status,
            delay_ms,
            requests,
        }
    }
    async fn requests(&self) -> Vec<serde_json::Value> {
        self.requests.lock().await.clone()
    }
}

async fn queued_setup() -> (String, Arc<AppState>, tempfile::TempDir, OutboxRelay) {
    let (_, initial, tmp) = setup_http().await;
    let relay = OutboxRelay::start().await;
    let mut config = initial.config.clone();
    config.push_relay.url = relay.url.clone();
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    let base = serve_outbox_http(state.clone()).await;
    (base, state, tmp, relay)
}

async fn serve_outbox_http(state: Arc<AppState>) -> String {
    // Deliberately start HTTP first. Production's same startup entry point is
    // owned by each test so it can stop/reopen a Server while work is pending.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = serverbee_server::router::create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    base
}

async fn queued_register(client: &reqwest::Client, base: &str, access: &str, device: &str) {
    assert_eq!(
        preferences_http(client, base, access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    let grant = serde_json::json!({"device_token":if device=="device-b" { "b".repeat(64) } else { "a".repeat(64) },
        "environment":"sandbox","key_id":"fixture-key","grant_id":device,"grant_token":device});
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/verified-register"))
            .bearer_auth(access)
            .json(&content_registration(&grant, 1))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}

async fn outbox_job(
    state: &AppState,
    event: &str,
    installation: &str,
) -> serverbee_server::entity::mobile_push_outbox::Model {
    serverbee_server::entity::mobile_push_outbox::Entity::find_by_id((
        event.to_owned(),
        installation.to_owned(),
    ))
    .one(&state.db)
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn durable_outbox_deduplicates_retries_and_resumes_after_database_reopen() {
    let (base, state, tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "durable-install").await;
    let access = login["access_token"].as_str().unwrap();
    queued_register(&client, &base, access, "device-a").await;
    let event = uuid::Uuid::new_v4().to_string();
    let first = enqueue_test(&client, &base, access, 2, &event)
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(first["data"]["outcome"], "pending");
    assert_eq!(first["data"]["presentation"], "unobserved");
    assert!(relay.requests().await.is_empty());
    // Concurrent retransmission exercises the actual transaction and uniqueness.
    let (a, b) = tokio::join!(
        enqueue_test(&client, &base, access, 2, &event),
        enqueue_test(&client, &base, access, 2, &event)
    );
    assert_eq!(a.status(), 200);
    assert_eq!(b.status(), 200);
    let original = outbox_job(&state, &event, "durable-install").await;
    assert_eq!(original.expires_at - original.created_at, 1800);
    for forbidden in [
        "content_key",
        "deployment_id",
        "https://serverbee.test",
        "durable-install",
        "grant_token",
    ] {
        assert!(!original.envelope.as_deref().unwrap().contains(forbidden));
    }
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    wait_test(&client, &base, access, &event, "retryable").await;
    worker.abort();
    let _ = worker.await;
    let before = relay.requests().await.len();
    assert_eq!(before, 1);
    // Reopen migrated SQLite and start the same entry point as production.
    let db = Database::connect(format!(
        "sqlite://{}/test.db?mode=rwc",
        tmp.path().display()
    ))
    .await
    .unwrap();
    let restarted = AppState::new(db, state.config.clone()).await.unwrap();
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let base = serve_outbox_http(restarted.clone()).await;
    let worker = serverbee_server::service::mobile_push_outbox::start(restarted.clone());
    wait_test(&client, &base, access, &event, "accepted").await;
    worker.abort();
    let _ = worker.await;
    let final_job = outbox_job(&restarted, &event, "durable-install").await;
    assert_eq!(final_job.created_at, original.created_at);
    assert_eq!(final_job.expires_at, original.expires_at);
    assert!(final_job.envelope.is_none());
    assert_eq!(relay.requests().await.len(), 2);
    for request in relay.requests().await {
        assert_eq!(request["event_id"], event);
        assert_eq!(request["expires_at"], original.expires_at);
    }
    assert_eq!(
        enqueue_test(&client, &base, access, 2, &event)
            .await
            .json::<serde_json::Value>()
            .await
            .unwrap()["data"]["outcome"],
        "accepted"
    );
    assert_eq!(
        enqueue_test(&client, &base, access, 3, &event)
            .await
            .status(),
        409
    );
    let other = login_http(&client, &base, "admin", "other-install").await;
    assert_eq!(
        client
            .get(format!("{base}/api/mobile/push/test/{event}"))
            .bearer_auth(other["access_token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

#[tokio::test]
async fn outbox_expiry_uses_original_creation_and_never_sends_stale_ciphertext() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "expiry-install").await;
    let access = login["access_token"].as_str().unwrap();
    queued_register(&client, &base, access, "device-a").await;
    let event = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        enqueue_test(&client, &base, access, 2, &event)
            .await
            .status(),
        200
    );
    // Advance the persisted clock boundary only, without substituting policy.
    state
        .db
        .execute(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "UPDATE mobile_push_outbox SET created_at=?,expires_at=? WHERE event_id=?",
            [
                (Utc::now().timestamp() - 1801).into(),
                (Utc::now().timestamp() - 1).into(),
                event.clone().into(),
            ],
        ))
        .await
        .unwrap();
    assert_eq!(
        wait_test(&client, &base, access, &event, "expired").await["reason"],
        "Expired"
    );
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    for _ in 0..100 {
        if outbox_job(&state, &event, "expiry-install")
            .await
            .envelope
            .is_none()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        outbox_job(&state, &event, "expiry-install")
            .await
            .envelope
            .is_none()
    );
    worker.abort();
    let _ = worker.await;
    assert!(relay.requests().await.is_empty());
}

#[tokio::test]
async fn queued_delivery_revalidates_disable_logout_revocation_expiry_role_password_and_user() {
    for action in [
        "disable",
        "logout",
        "device",
        "expiry",
        "role",
        "password",
        "user",
        "replacement",
        "grant_expiry",
    ] {
        let (base, state, _tmp, relay) = queued_setup().await;
        let client = reqwest::Client::new();
        let login = login_http(
            &client,
            &base,
            if action == "role" { "admin" } else { "member" },
            "revoked-install",
        )
        .await;
        let access = login["access_token"].as_str().unwrap();
        AuthService::create_user(&state.db, "queue-operator", "testpass", "admin")
            .await
            .unwrap();
        let operator = login_http(&client, &base, "queue-operator", "operator-install").await;
        let admin = operator["access_token"].as_str().unwrap();
        queued_register(&client, &base, access, "device-a").await;
        let event = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            enqueue_test(&client, &base, access, 2, &event)
                .await
                .status(),
            200
        );
        let initial_worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
        wait_test(&client, &base, access, &event, "retryable").await;
        initial_worker.abort();
        let _ = initial_worker.await;
        let user_id = login["user"]["id"].as_str().unwrap();
        let mobile = mobile_session::Entity::find()
            .filter(mobile_session::Column::UserId.eq(user_id))
            .filter(mobile_session::Column::InstallationId.eq("revoked-install"))
            .one(&state.db)
            .await
            .unwrap()
            .unwrap();
        match action {
            "disable" => assert_eq!(
                preferences_http(&client, &base, access, 2, intent(false, false))
                    .await
                    .status(),
                200
            ),
            "logout" => assert_eq!(
                client
                    .post(format!("{base}/api/mobile/auth/logout"))
                    .bearer_auth(access)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            ),
            "device" => assert_eq!(
                client
                    .delete(format!("{base}/api/mobile/auth/devices/{}", mobile.id))
                    .bearer_auth(access)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            ),
            "expiry" => {
                mobile_session::Entity::update_many()
                    .col_expr(
                        mobile_session::Column::ExpiresAt,
                        sea_orm::sea_query::Expr::value(Utc::now() - ChronoDuration::seconds(1)),
                    )
                    .filter(mobile_session::Column::Id.eq(mobile.id))
                    .exec(&state.db)
                    .await
                    .unwrap();
            }
            "role" | "password" => {
                let body = if action == "role" {
                    serde_json::json!({"role":"member"})
                } else {
                    serde_json::json!({"password":"replacement-testpass"})
                };
                assert_eq!(
                    client
                        .put(format!("{base}/api/users/{user_id}"))
                        .bearer_auth(admin)
                        .json(&body)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    200
                );
            }
            "user" => {
                let response = client
                    .delete(format!("{base}/api/users/{user_id}"))
                    .bearer_auth(admin)
                    .send()
                    .await
                    .unwrap();
                let status = response.status();
                let body = response.json::<serde_json::Value>().await.unwrap();
                assert_eq!(
                    status, 200,
                    "DELETE user error code={}, message={}",
                    body["error"]["code"], body["error"]["message"]
                );
            }
            "grant_expiry" => {
                use serverbee_server::entity::mobile_push_registration as registration;
                registration::Entity::update_many()
                    .col_expr(
                        registration::Column::GrantExpiresAt,
                        sea_orm::sea_query::Expr::value(Utc::now() - ChronoDuration::seconds(1)),
                    )
                    .filter(registration::Column::InstallationId.eq("revoked-install"))
                    .exec(&state.db)
                    .await
                    .unwrap();
            }
            "replacement" => {
                let grant = serde_json::json!({"device_token":"b".repeat(64),"environment":"sandbox","key_id":"fixture-key","grant_id":"device-b","grant_token":"device-b"});
                assert_eq!(
                    client
                        .post(format!("{base}/api/mobile/push/verified-register"))
                        .bearer_auth(access)
                        .json(&content_registration(&grant, 2))
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    200
                );
            }
            _ => unreachable!(),
        }
        let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
        for _ in 0..250 {
            if outbox_job(&state, &event, "revoked-install").await.outcome == "permanent" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let job = outbox_job(&state, &event, "revoked-install").await;
        assert_eq!(job.outcome, "permanent", "{action}");
        assert_eq!(job.reason, "Ineligible");
        assert!(job.envelope.is_none());
        worker.abort();
        let _ = worker.await;
        assert_eq!(relay.requests().await.len(), 1, "{action}");
    }
}

#[tokio::test]
async fn retrying_device_does_not_block_independent_installation_and_rate_limit_recovers() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let a = login_http(&client, &base, "member", "device-a-install").await;
    let b = login_http(&client, &base, "member", "device-b-install").await;
    let aa = a["access_token"].as_str().unwrap();
    let ba = b["access_token"].as_str().unwrap();
    queued_register(&client, &base, aa, "device-a").await;
    queued_register(&client, &base, ba, "device-b").await;
    let event = uuid::Uuid::new_v4().to_string();
    relay.status.store(429, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        enqueue_test(&client, &base, aa, 2, &event).await.status(),
        200
    );
    assert_eq!(
        enqueue_test(&client, &base, ba, 2, &event).await.status(),
        200
    );
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    wait_test(&client, &base, aa, &event, "retryable").await;
    wait_test(&client, &base, ba, &event, "accepted").await;
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    wait_test(&client, &base, aa, &event, "accepted").await;
    worker.abort();
    let _ = worker.await;
    assert_eq!(
        outbox_job(&state, &event, "device-a-install")
            .await
            .attempts,
        2
    );
    assert_eq!(
        outbox_job(&state, &event, "device-b-install")
            .await
            .attempts,
        1
    );
    assert_eq!(relay.requests().await.len(), 3);
}

#[tokio::test]
async fn bounded_network_timeout_keeps_http_admission_and_other_device_responsive() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let a = login_http(&client, &base, "member", "slow-install").await;
    let aa = a["access_token"].as_str().unwrap();
    let b = login_http(&client, &base, "member", "fast-install").await;
    let ba = b["access_token"].as_str().unwrap();
    queued_register(&client, &base, aa, "device-a").await;
    queued_register(&client, &base, ba, "device-b").await;
    relay
        .delay_ms
        .store(20_000, std::sync::atomic::Ordering::SeqCst);
    let slow = uuid::Uuid::new_v4().to_string();
    let fast = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        enqueue_test(&client, &base, aa, 2, &slow).await.status(),
        200
    );
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    for _ in 0..100 {
        if !relay.requests().await.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(relay.requests().await.len(), 1);
    let started = std::time::Instant::now();
    assert_eq!(
        enqueue_test(&client, &base, ba, 2, &fast).await.status(),
        200
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    wait_test(&client, &base, ba, &fast, "accepted").await;
    assert_eq!(
        outbox_job(&state, &slow, "slow-install").await.outcome,
        "pending"
    );
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if outbox_job(&state, &slow, "slow-install").await.outcome == "retryable" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    worker.abort();
    let _ = worker.await;
    assert_eq!(
        outbox_job(&state, &slow, "slow-install").await.reason,
        "RelayUnavailable"
    );
}

#[tokio::test]
async fn revoked_actual_relay_grant_stops_queued_delivery_without_erasing_device_registration() {
    let relay = DeliveryRelayFixture::start().await;
    let (_, initial, _tmp) = setup_http().await;
    let mut config = initial.config.clone();
    config.push_relay.url = relay.ready["url"].as_str().unwrap().to_owned();
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    let base = serve_outbox_http(state.clone()).await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "revoked-grant-install").await;
    let access = login["access_token"].as_str().unwrap();
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/verified-register"))
            .bearer_auth(access)
            .json(&content_registration(&relay.ready["grant"], 1))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let event = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        enqueue_test(&client, &base, access, 2, &event)
            .await
            .status(),
        200
    );
    // Real assertion-driven renewal revokes the old grant at Relay admission.
    assert_eq!(
        client
            .post(format!(
                "{}/fixture/renew",
                relay.ready["url"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    wait_test(&client, &base, access, &event, "permanent").await;
    worker.abort();
    let _ = worker.await;
    assert!(!relay.path("provider-request.json").exists());
    let row = serverbee_server::entity::mobile_push_registration::Entity::find_by_id(
        "revoked-grant-install",
    )
    .one(&state.db)
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.revision, 2);
    assert!(row.grant_token.is_some());
    assert_eq!(
        status_http(&client, &base, access).await["registered"],
        false
    );
}

#[tokio::test]
async fn abandoned_inflight_lease_recovers_after_restart_without_renewing_event_expiry() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "member", "crash-install").await;
    let access = login["access_token"].as_str().unwrap();
    queued_register(&client, &base, access, "device-a").await;
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    relay
        .delay_ms
        .store(20_000, std::sync::atomic::Ordering::SeqCst);
    let event = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        enqueue_test(&client, &base, access, 2, &event)
            .await
            .status(),
        200
    );
    let original = outbox_job(&state, &event, "crash-install").await;
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    for _ in 0..100 {
        if !relay.requests().await.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(relay.requests().await.len(), 1);
    worker.abort();
    let _ = worker.await;
    let abandoned = outbox_job(&state, &event, "crash-install").await;
    assert!(abandoned.lease_id.is_some());
    assert_eq!(abandoned.attempts, 1);
    relay.delay_ms.store(0, std::sync::atomic::Ordering::SeqCst);
    let restarted = AppState::new(state.db.clone(), state.config.clone())
        .await
        .unwrap();
    let worker = serverbee_server::service::mobile_push_outbox::start(restarted.clone());
    // No fixture edits to lease policy: wait for the real 30-second durable lease.
    tokio::time::timeout(std::time::Duration::from_secs(40), async {
        loop {
            if outbox_job(&restarted, &event, "crash-install")
                .await
                .outcome
                == "accepted"
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    worker.abort();
    let _ = worker.await;
    let delivered = outbox_job(&restarted, &event, "crash-install").await;
    assert_eq!(delivered.created_at, original.created_at);
    assert_eq!(delivered.expires_at, original.expires_at);
    assert_eq!(delivered.attempts, 2);
    assert_eq!(relay.requests().await.len(), 2);
}

#[tokio::test]
async fn user_mutations_wait_for_outbox_writer_before_reading_revocation_guards() {
    for operation in ["delete", "password", "role"] {
        let (base, state, _tmp, relay) = queued_setup().await;
        let client = reqwest::Client::new();
        let login = login_http(&client, &base, "member", "contended-install").await;
        let access = login["access_token"].as_str().unwrap();
        let admin = login_http(&client, &base, "admin", "admin-install").await;
        queued_register(&client, &base, access, "device-a").await;
        let event = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            enqueue_test(&client, &base, access, 2, &event)
                .await
                .status(),
            200
        );
        // Hold a real SQLite writer on the outbox, as an independent worker can.
        // No policy or persistence helper is substituted. The old deferred
        // transaction reads user guards then fails its upgrade with SQLITE_BUSY.
        use sea_orm::TransactionTrait;
        let writer = state.db.begin().await.unwrap();
        writer
            .execute_unprepared("UPDATE mobile_push_outbox SET attempts=attempts")
            .await
            .unwrap();
        let releasing = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            writer.commit().await.unwrap();
        });
        let endpoint = format!("{base}/api/users/{}", login["user"]["id"].as_str().unwrap());
        let request = if operation == "delete" {
            client.delete(endpoint)
        } else {
            client.put(endpoint).json(&if operation == "role" {
                serde_json::json!({"role":"admin"})
            } else {
                serde_json::json!({"password":"replacement-testpass"})
            })
        };
        let response = request
            .bearer_auth(admin["access_token"].as_str().unwrap())
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = response.json::<serde_json::Value>().await.unwrap();
        assert_eq!(
            status, 200,
            "{operation} error code={}, message={}",
            body["error"]["code"], body["error"]["message"]
        );
        releasing.await.unwrap();
        let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
        for _ in 0..100 {
            if outbox_job(&state, &event, "contended-install")
                .await
                .outcome
                == "permanent"
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        worker.abort();
        let _ = worker.await;
        assert_eq!(
            outbox_job(&state, &event, "contended-install").await.reason,
            "Ineligible"
        );
        assert!(relay.requests().await.is_empty(), "{operation}");
    }
}
