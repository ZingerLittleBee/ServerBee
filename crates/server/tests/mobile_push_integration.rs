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
        "expected_revision":revision, "device_token":"a".repeat(64), "environment":"sandbox", "key_id":"fixture-key", "grant_id":"fixture-grant", "grant_token":grant
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
    assert_eq!(confirmed["delivery_available"], false);
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
            "grant_token":format!("{environment}-fixture")
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
