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

/// Match production's options on every pooled connection, including reopen.
async fn production_wal_db(
    path: &std::path::Path,
    max_connections: u32,
) -> sea_orm::DatabaseConnection {
    use sqlx::ConnectOptions as _;
    use sqlx::sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
    };
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5))
        .disable_statement_logging();
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .expect("production-configured WAL pool");
    sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool)
}

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

    let db = production_wal_db(&tmp.path().join("test.db"), config.database.max_connections).await;
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
    serde_json::json!({"enabled":enabled, "alerts":true, "security":security, "task_failure":false, "task_success":false})
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
        permitted["task_failure"] = serde_json::json!(false);
        permitted["task_success"] = serde_json::json!(false);
        let saved = preferences_http(&client, &base, access, 2, permitted.clone()).await;
        assert_eq!(saved.status(), 200);
        let saved = saved.json::<serde_json::Value>().await.unwrap()["data"].clone();
        assert_eq!(saved["preferences"], permitted);
        assert_eq!(saved["revision"], 3);
        assert_eq!(saved["security_allowed"], false);
        assert_eq!(saved["registered"], enabled);
        assert_eq!(saved["delivery_available"], true);
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
        assert!(!row.task_failure);
        assert!(!row.task_success);
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

#[path = "mobile_push_tasks/mod.rs"]
mod task_outcomes;

// Alert subscriptions exercise real HTTP setup, migrated SQLite, production
// evaluation and durable dispatch. Only the Relay network boundary is replaced.
async fn alert_http_fixture(
    client: &reqwest::Client,
    base: &str,
    admin: &str,
    mode: &str,
) -> (String, String) {
    let response = client.post(format!("{base}/api/servers")).bearer_auth(admin)
        .json(&serde_json::json!({"onboarding_request_id":uuid::Uuid::new_v4().to_string(),"name":"Private alert Server"}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let server = response.json::<serde_json::Value>().await.unwrap()["data"]["server_id"]
        .as_str()
        .unwrap()
        .to_owned();
    set_alert_expiration(client, base, admin, &server, true).await;
    let response = client
        .post(format!("{base}/api/alert-rules"))
        .bearer_auth(admin)
        .json(
            &serde_json::json!({"name":"Private expiration rule","enabled":true,"trigger_mode":mode,
            "cover_type":"include","server_ids":[&server],
            "rules":[{"rule_type":"expiration","duration":7}]}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let rule = response.json::<serde_json::Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (server, rule)
}
async fn set_alert_expiration(
    client: &reqwest::Client,
    base: &str,
    admin: &str,
    server: &str,
    firing: bool,
) {
    let response = client.put(format!("{base}/api/servers/{server}")).bearer_auth(admin)
        .json(&serde_json::json!({"expired_at":(Utc::now()+ChronoDuration::days(if firing {1} else {90})).to_rfc3339()}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
}
async fn evaluate_alerts(state: &AppState) {
    serverbee_server::service::alert::AlertService::evaluate_all(
        &state.db,
        &state.config,
        &state.agent_manager,
        &state.alert_state_manager,
    )
    .await
    .unwrap();
}
async fn alert_jobs(state: &AppState) -> Vec<serverbee_server::entity::mobile_push_outbox::Model> {
    use serverbee_server::entity::mobile_push_outbox as outbox;
    outbox::Entity::find()
        .filter(outbox::Column::Category.eq("alert"))
        .all(&state.db)
        .await
        .unwrap()
}
fn decrypt_alert_envelope(envelope: &serde_json::Value) -> serde_json::Value {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ring::aead;
    let key = STANDARD
        .decode("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=")
        .unwrap();
    let nonce: [u8; 12] = STANDARD
        .decode(envelope["nonce"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let mut ciphertext = STANDARD
        .decode(envelope["ciphertext"].as_str().unwrap())
        .unwrap();
    let aad = format!(
        "ServerBee.Push.v1|{}|{}",
        envelope["key_id"].as_str().unwrap(),
        envelope["identity"].as_str().unwrap()
    );
    let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &key).unwrap());
    let bytes = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad.as_bytes()),
            &mut ciphertext,
        )
        .unwrap();
    serde_json::from_slice(bytes).unwrap()
}
async fn wait_alert_dispatch(state: &AppState) {
    for _ in 0..100 {
        if alert_jobs(state)
            .await
            .iter()
            .all(|j| !matches!(j.outcome.as_str(), "pending" | "retryable"))
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("Alert jobs did not reach a terminal result");
}

#[tokio::test]
async fn alert_subscriptions_fan_out_trigger_recovery_without_group_and_open_exact_cycle() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let operator = login_http(&client, &base, "admin", "alert-operator").await;
    let admin = operator["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    let member = login_http(&client, &base, "member", "alert-device-a").await;
    let second = login_http(&client, &base, "member", "alert-device-b").await;
    queued_register(
        &client,
        &base,
        member["access_token"].as_str().unwrap(),
        "device-a",
    )
    .await;
    queued_register(
        &client,
        &base,
        second["access_token"].as_str().unwrap(),
        "device-b",
    )
    .await;
    let unsubscribed = login_http(&client, &base, "member", "alert-unsubscribed").await;
    let unsubscribed_access = unsubscribed["access_token"].as_str().unwrap();
    queued_register(&client, &base, unsubscribed_access, "device-b").await;
    let mut prefs = intent(false, true);
    prefs["alerts"] = serde_json::json!(false);
    assert_eq!(
        preferences_http(&client, &base, unsubscribed_access, 2, prefs)
            .await
            .status(),
        200
    );
    let (server, rule) = alert_http_fixture(&client, &base, admin, "once").await;
    evaluate_alerts(&state).await;
    evaluate_alerts(&state).await;
    let jobs = alert_jobs(&state).await;
    assert_eq!(jobs.len(), 3, "Once mode suppresses repeated evaluations");
    assert_eq!(
        jobs[0].event_id, jobs[1].event_id,
        "One logical transition per installation"
    );
    let envelope: serde_json::Value =
        serde_json::from_str(jobs[0].envelope.as_deref().unwrap()).unwrap();
    let content = decrypt_alert_envelope(&envelope);
    assert_eq!(content["kind"], "alert");
    assert_eq!(content["alert"]["status"], "firing");
    assert_eq!(
        content["expires_at"].as_i64().unwrap() - content["created_at"].as_i64().unwrap(),
        1800
    );
    let key = content["alert"]["alert_key"].as_str().unwrap().to_owned();
    let detail = client
        .get(format!("{base}/api/alert-events/{key}"))
        .bearer_auth(member["access_token"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(detail.status(), 200);
    let detail = detail.json::<serde_json::Value>().await.unwrap();
    assert_eq!(detail["data"]["rule_id"], rule);
    assert_eq!(detail["data"]["server_id"], server);
    let list = client
        .get(format!("{base}/api/alert-events"))
        .bearer_auth(admin)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(list["data"][0]["alert_key"], key);
    for job in &jobs {
        let ciphertext = job.envelope.as_ref().unwrap();
        for forbidden in [
            "Private alert Server",
            "Private expiration rule",
            "rule_id",
            "server_id",
            "deployment_id",
            "content_key",
        ] {
            assert!(
                !ciphertext.contains(forbidden),
                "No plaintext notification data in queue envelope"
            );
        }
    }
    set_alert_expiration(&client, &base, admin, &server, false).await;
    evaluate_alerts(&state).await;
    evaluate_alerts(&state).await;
    let recovery_jobs = alert_jobs(&state).await;
    assert_eq!(recovery_jobs.len(), 6, "Recovery is edge-triggered");
    let recovered = recovery_jobs
        .iter()
        .find(|j| j.event_id != jobs[0].event_id)
        .unwrap();
    let content = decrypt_alert_envelope(
        &serde_json::from_str(recovered.envelope.as_deref().unwrap()).unwrap(),
    );
    assert_eq!(content["alert"]["status"], "resolved");
    assert_eq!(content["alert"]["alert_key"], key);
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    wait_alert_dispatch(&state).await;
    worker.abort();
    let _ = worker.await;
    assert_eq!(relay.requests().await.len(), 6);
    // Restart must not replay accepted logical transitions.
    let restarted = AppState::new(state.db.clone(), state.config.clone())
        .await
        .unwrap();
    evaluate_alerts(&restarted).await;
    assert_eq!(alert_jobs(&restarted).await.len(), 6);
    set_alert_expiration(&client, &base, admin, &server, true).await;
    evaluate_alerts(&restarted).await;
    assert_eq!(
        client
            .get(format!("{base}/api/alert-events/{key}"))
            .bearer_auth(admin)
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "Old cycle cannot open the new cycle"
    );
    assert_eq!(
        client
            .delete(format!("{base}/api/alert-rules/{rule}"))
            .bearer_auth(admin)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{base}/api/alert-events/{key}"))
            .bearer_auth(admin)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

#[tokio::test]
async fn alert_subscription_disabled_maintenance_and_always_suppression_preserve_gates() {
    let (base, state, _tmp, _relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let operator = login_http(&client, &base, "admin", "gates-admin").await;
    let admin = operator["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    let (server, rule) = alert_http_fixture(&client, &base, admin, "always").await;
    let response = client
        .put(format!("{base}/api/alert-rules/{rule}"))
        .bearer_auth(admin)
        .json(&serde_json::json!({"enabled":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    evaluate_alerts(&state).await;
    assert!(alert_jobs(&state).await.is_empty());
    assert_eq!(
        client
            .put(format!("{base}/api/alert-rules/{rule}"))
            .bearer_auth(admin)
            .json(&serde_json::json!({"enabled":true}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let response=client.post(format!("{base}/api/maintenances")).bearer_auth(admin).json(&serde_json::json!({"title":"Planned work",
        "start_at":(Utc::now()-ChronoDuration::minutes(5)).to_rfc3339(),"end_at":(Utc::now()+ChronoDuration::minutes(5)).to_rfc3339(),
        "server_ids_json": [&server],"is_public":false})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let maintenance = response.json::<serde_json::Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    evaluate_alerts(&state).await;
    assert!(alert_jobs(&state).await.is_empty());
    assert_eq!(
        client
            .delete(format!("{base}/api/maintenances/{maintenance}"))
            .bearer_auth(admin)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    evaluate_alerts(&state).await;
    evaluate_alerts(&state).await;
    assert_eq!(
        alert_jobs(&state).await.len(),
        1,
        "Five-minute debounce remains active"
    );
    // Recovery deliberately retains the existing evaluator's maintenance behavior.
    set_alert_expiration(&client, &base, admin, &server, false).await;
    evaluate_alerts(&state).await;
    assert_eq!(alert_jobs(&state).await.len(), 2);
}

#[tokio::test]
async fn alert_unsubscribe_before_dispatch_stops_only_that_installation() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let operator = login_http(&client, &base, "admin", "unsubscribe-admin").await;
    let admin = operator["access_token"].as_str().unwrap();
    let a = login_http(&client, &base, "member", "unsubscribe-a").await;
    let b = login_http(&client, &base, "member", "unsubscribe-b").await;
    let access = a["access_token"].as_str().unwrap();
    queued_register(&client, &base, access, "device-a").await;
    queued_register(
        &client,
        &base,
        b["access_token"].as_str().unwrap(),
        "device-b",
    )
    .await;
    alert_http_fixture(&client, &base, admin, "once").await;
    evaluate_alerts(&state).await;
    let mut prefs = intent(false, true);
    prefs["alerts"] = serde_json::json!(false);
    assert_eq!(
        preferences_http(&client, &base, access, 1, prefs.clone())
            .await
            .status(),
        409,
        "Failed save cannot replace confirmed subscription"
    );
    assert_eq!(
        status_http(&client, &base, access).await["preferences"]["alerts"],
        true
    );
    assert_eq!(
        preferences_http(&client, &base, access, 2, prefs)
            .await
            .status(),
        200
    );
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    wait_alert_dispatch(&state).await;
    worker.abort();
    let _ = worker.await;
    assert_eq!(relay.requests().await.len(), 1);
    let jobs = alert_jobs(&state).await;
    assert_eq!(
        jobs.iter()
            .find(|j| j.installation_id == "unsubscribe-a")
            .unwrap()
            .reason,
        "Ineligible"
    );
    assert_eq!(
        jobs.iter()
            .find(|j| j.installation_id == "unsubscribe-b")
            .unwrap()
            .outcome,
        "accepted"
    );
}

#[tokio::test]
async fn event_alerts_enqueue_general_category_but_security_matches_do_not() {
    let (base, state, _tmp, _relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let operator = login_http(&client, &base, "admin", "event-admin").await;
    let admin = operator["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    let (server, _) = alert_http_fixture(&client, &base, admin, "once").await;
    for kind in ["ip_changed", "ssh_brute_force_detected"] {
        let created = client.post(format!("{base}/api/alert-rules")).bearer_auth(admin)
            .json(&serde_json::json!({"name":kind,"enabled":true,"trigger_mode":"once", "cover_type":"all",
                "rules":[{"rule_type":kind}]})).send().await.unwrap();
        assert_eq!(created.status(), 200);
        serverbee_server::service::alert::AlertService::check_event_rules(
            &state.db,
            &state.config,
            &state.alert_state_manager,
            &server,
            kind,
        )
        .await
        .unwrap();
    }
    let jobs = alert_jobs(&state).await;
    assert_eq!(
        jobs.len(),
        1,
        "Security is never fanned out through general alert subscriptions"
    );
    let content = decrypt_alert_envelope(
        &serde_json::from_str(jobs[0].envelope.as_deref().unwrap()).unwrap(),
    );
    assert_eq!(content["alert"]["rule_name"], "ip_changed");
}

#[tokio::test]
async fn alert_detail_complete_identity_distinguishes_event_dimensions() {
    use serverbee_common::security::{
        DetectorSource, SecurityEventPayload, SecurityEventType, SecurityEvidence, Severity,
    };
    let (base, state, _tmp, _relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let operator = login_http(&client, &base, "admin", "dimension-admin").await;
    let admin = operator["access_token"].as_str().unwrap();
    let (server, _) = alert_http_fixture(&client, &base, admin, "once").await;
    let created = client
        .post(format!("{base}/api/alert-rules"))
        .bearer_auth(admin)
        .json(
            &serde_json::json!({"name":"Dimension rule","enabled":true,"cover_type":"all",
            "rules":[{"rule_type":"ssh_brute_force_detected"}]}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);
    let rule = created.json::<serde_json::Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for ip in ["203.0.113.5", "203.0.113.6"] {
        state
            .security_service
            .record_event(
                &server,
                SecurityEventPayload {
                    event_type: SecurityEventType::SshBruteForce,
                    severity: Severity::High,
                    source_ip: ip.into(),
                    source_port: None,
                    username: None,
                    started_at: Utc::now().timestamp() - 60,
                    ended_at: Utc::now().timestamp(),
                    first_seen: false,
                    detector_source: DetectorSource::Journal,
                    evidence: SecurityEvidence::SshBruteForce {
                        failed_count: 47,
                        distinct_users: 1,
                        sample_users: vec!["root".into()],
                        invalid_user_count: 0,
                        window_seconds: 60,
                        threshold: 10,
                    },
                },
            )
            .await
            .unwrap();
    }
    let events = client
        .get(format!("{base}/api/alert-events"))
        .bearer_auth(admin)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let events: Vec<_> = events["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["rule_id"] == rule)
        .collect();
    assert_eq!(events.len(), 2);
    assert_ne!(events[0]["alert_key"], events[1]["alert_key"]);
    for event in events {
        let key = event["alert_key"].as_str().unwrap();
        let detail = client
            .get(format!("{base}/api/alert-events/{key}"))
            .bearer_auth(admin)
            .send()
            .await
            .unwrap();
        assert_eq!(detail.status(), 200);
        assert_eq!(
            detail.json::<serde_json::Value>().await.unwrap()["data"]["alert_key"],
            key
        );
    }
    assert_eq!(
        client
            .get(format!("{base}/api/alert-events/{rule}:{server}"))
            .bearer_auth(admin)
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "Legacy keys must not select an arbitrary security dimension"
    );
}

#[derive(Clone, Copy, Debug)]
enum AlertAdmissionFault {
    SecondInsert,
    DeferredCommit,
}

#[derive(Clone, Copy, Debug)]
enum AlertRollbackPhase {
    Trigger,
    Recovery,
    Rearm,
}

async fn fault_alert_admission(state: &AppState, fault: AlertAdmissionFault) {
    match fault {
        AlertAdmissionFault::SecondInsert => {
            // Fail after the first installation's job was actually inserted.
            state.db.execute_unprepared("CREATE TRIGGER fail_alert_admission
                BEFORE INSERT ON mobile_push_outbox
                WHEN NEW.category='alert' AND EXISTS
                    (SELECT 1 FROM mobile_push_outbox WHERE event_id=NEW.event_id AND category='alert')
                BEGIN SELECT RAISE(ABORT, 'injected second alert insert failure'); END")
                .await.expect("install SQLite insertion fault");
        }
        AlertAdmissionFault::DeferredCommit => {
            // Every write succeeds; the deferred FK fails the real COMMIT.
            state
                .db
                .execute_unprepared(
                    "PRAGMA foreign_keys=ON;
                CREATE TABLE alert_commit_parent (id INTEGER PRIMARY KEY);
                CREATE TABLE alert_commit_child (parent_id INTEGER NOT NULL
                    REFERENCES alert_commit_parent(id) DEFERRABLE INITIALLY DEFERRED);
                CREATE TRIGGER fail_alert_admission AFTER INSERT ON mobile_push_outbox
                WHEN NEW.category='alert'
                BEGIN INSERT INTO alert_commit_child(parent_id) VALUES (1); END",
                )
                .await
                .expect("install SQLite commit fault");
        }
    }
}

async fn reopen_alert_state(state: &AppState, directory: &tempfile::TempDir) -> Arc<AppState> {
    let mut options = ConnectOptions::new(format!(
        "sqlite://{}/test.db?mode=rwc",
        directory.path().display()
    ));
    options.max_connections(5).sqlx_logging(false);
    let db = Database::connect(options)
        .await
        .expect("reopen real SQLite");
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .expect("enable constraints");
    AppState::new(db, state.config.clone())
        .await
        .expect("restore production alert cache")
}

async fn persisted_alert_cycle(
    state: &AppState,
    rule: &str,
    server: &str,
) -> Option<serverbee_server::entity::alert_state::Model> {
    use serverbee_server::entity::alert_state;
    alert_state::Entity::find()
        .filter(alert_state::Column::RuleId.eq(rule))
        .filter(alert_state::Column::ServerId.eq(server))
        .filter(alert_state::Column::EventKey.eq(""))
        .one(&state.db)
        .await
        .expect("read durable alert state")
}

async fn attach_alert_webhook(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    rule: &str,
) -> Arc<tokio::sync::Mutex<Vec<String>>> {
    use axum::{Router, routing::post};
    let received = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let sink = received.clone();
    let app = Router::new().route(
        "/",
        post(move |body: String| {
            let sink = sink.clone();
            async move {
                sink.lock().await.push(body);
                "ok"
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let channel = client.post(format!("{base}/api/notifications")).bearer_auth(access)
        .json(&serde_json::json!({"name":"Atomic alert webhook","notify_type":"webhook","enabled":true,
            "config_json":{"url":url,"method":"POST","body_template":"{{event}}"}}))
        .send().await.unwrap();
    assert_eq!(channel.status(), 200);
    let channel = channel.json::<serde_json::Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let group = client
        .post(format!("{base}/api/notification-groups"))
        .bearer_auth(access)
        .json(&serde_json::json!({"name":"Atomic alert group","notification_ids":[channel]}))
        .send()
        .await
        .unwrap();
    assert_eq!(group.status(), 200);
    let group = group.json::<serde_json::Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        client
            .put(format!("{base}/api/alert-rules/{rule}"))
            .bearer_auth(access)
            .json(&serde_json::json!({"notification_group_id":group}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    received
}

async fn assert_alert_rollback(
    state: &AppState,
    rule: &str,
    server: &str,
    before: &Option<serverbee_server::entity::alert_state::Model>,
    jobs_before: &[serverbee_server::entity::mobile_push_outbox::Model],
) {
    assert_eq!(
        &persisted_alert_cycle(state, rule, server).await,
        before,
        "Failed transaction must not consume a transition or rearm"
    );
    assert_eq!(
        alert_jobs(state).await,
        jobs_before,
        "First recipient and durable alert state must roll back with the failed recipient/commit"
    );
    assert_eq!(
        state.alert_state_manager.is_triggered(rule, server, ""),
        before.as_ref().is_some_and(|cycle| !cycle.resolved)
    );
    if let Some(cycle) = before.as_ref().filter(|cycle| !cycle.resolved) {
        let cached = state
            .alert_state_manager
            .get_info(rule, server, "")
            .expect("Firing cache survives failed recovery");
        assert_eq!(cached.first_triggered_at, cycle.first_triggered_at);
        assert_eq!(cached.last_notified_at, cycle.last_notified_at);
        assert_eq!(cached.count, cycle.count as u32);
    }
}

async fn exercise_alert_rollback_restart(phase: AlertRollbackPhase, fault: AlertAdmissionFault) {
    let (base, state, directory, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let operator = login_http(&client, &base, "admin", "rollback-a").await;
    let member = login_http(&client, &base, "member", "rollback-b").await;
    let admin = operator["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    queued_register(
        &client,
        &base,
        member["access_token"].as_str().unwrap(),
        "device-b",
    )
    .await;
    let (server, rule) = alert_http_fixture(&client, &base, admin, "once").await;
    let webhook = attach_alert_webhook(&client, &base, admin, &rule).await;
    match phase {
        AlertRollbackPhase::Trigger => {}
        AlertRollbackPhase::Recovery => {
            evaluate_alerts(&state).await;
            set_alert_expiration(&client, &base, admin, &server, false).await;
        }
        AlertRollbackPhase::Rearm => {
            evaluate_alerts(&state).await;
            set_alert_expiration(&client, &base, admin, &server, false).await;
            evaluate_alerts(&state).await;
            set_alert_expiration(&client, &base, admin, &server, true).await;
        }
    }
    let before = persisted_alert_cycle(&state, &rule, &server).await;
    let jobs_before = alert_jobs(&state).await;
    let external_before = webhook.lock().await.len();
    fault_alert_admission(&state, fault).await;
    // Repeat in the same process: a failed attempt cannot consume the hot cache.
    for _ in 0..2 {
        evaluate_alerts(&state).await;
        assert_alert_rollback(&state, &rule, &server, &before, &jobs_before).await;
        assert_eq!(
            webhook.lock().await.len(),
            external_before,
            "External dispatch follows committed admission"
        );
    }
    assert!(
        relay.requests().await.is_empty(),
        "No network work on evaluation"
    );
    // The SQLite fault itself survives reopening. Verify rollback after a fresh
    // production cache has loaded, before removing only the external fault.
    let restarted = reopen_alert_state(&state, &directory).await;
    evaluate_alerts(&restarted).await;
    assert_alert_rollback(&restarted, &rule, &server, &before, &jobs_before).await;
    restarted
        .db
        .execute_unprepared("DROP TRIGGER fail_alert_admission")
        .await
        .unwrap();
    evaluate_alerts(&restarted).await;
    let admitted_state = persisted_alert_cycle(&restarted, &rule, &server)
        .await
        .unwrap();
    let admitted_jobs = alert_jobs(&restarted).await;
    let new_jobs: Vec<_> = admitted_jobs
        .iter()
        .filter(|job| {
            !jobs_before.iter().any(|old| {
                old.event_id == job.event_id && old.installation_id == job.installation_id
            })
        })
        .collect();
    assert_eq!(
        new_jobs.len(),
        2,
        "Exactly one successful logical admission for each installation"
    );
    assert_eq!(new_jobs[0].event_id, new_jobs[1].event_id);
    let content = decrypt_alert_envelope(
        &serde_json::from_str(new_jobs[0].envelope.as_deref().unwrap()).unwrap(),
    );
    let resolved = matches!(phase, AlertRollbackPhase::Recovery);
    assert_eq!(
        content["alert"]["status"],
        if resolved { "resolved" } else { "firing" }
    );
    assert_eq!(
        content["alert"]["alert_key"],
        serverbee_server::service::alert::alert_detail_key(&admitted_state)
    );
    if let Some(before) = &before {
        if resolved {
            assert_eq!(admitted_state.first_triggered_at, before.first_triggered_at);
        } else {
            assert_ne!(admitted_state.first_triggered_at, before.first_triggered_at);
        }
    }
    for job in &new_jobs {
        assert_eq!(job.expires_at - job.created_at, 1800);
        assert_eq!(content["event_id"], job.event_id);
        assert_eq!(content["created_at"], job.created_at);
        assert_eq!(content["expires_at"], job.expires_at);
    }
    assert_eq!(webhook.lock().await.len(), external_before + 1);
    assert_eq!(
        webhook.lock().await.last().unwrap(),
        if resolved { "resolved" } else { "triggered" }
    );
    // A committed logical identity and its original deadline survive another
    // database reopen, repeated evaluation and the real durable worker.
    evaluate_alerts(&restarted).await;
    assert_eq!(alert_jobs(&restarted).await, admitted_jobs);
    let final_restart = reopen_alert_state(&restarted, &directory).await;
    evaluate_alerts(&final_restart).await;
    assert_eq!(alert_jobs(&final_restart).await, admitted_jobs);
    assert_eq!(
        webhook.lock().await.len(),
        external_before + 1,
        "No duplicate external transition after restart"
    );
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(final_restart.clone());
    wait_alert_dispatch(&final_restart).await;
    worker.abort();
    let _ = worker.await;
    let requests = relay.requests().await;
    assert_eq!(requests.len(), admitted_jobs.len());
    for original in &admitted_jobs {
        let receipt = outbox_job(
            &final_restart,
            &original.event_id,
            &original.installation_id,
        )
        .await;
        assert_eq!(receipt.outcome, "accepted");
        assert_eq!(receipt.created_at, original.created_at);
        assert_eq!(receipt.expires_at, original.expires_at);
        assert!(
            requests
                .iter()
                .any(|request| request["event_id"] == original.event_id
                    && request["expires_at"] == original.expires_at)
        );
    }
}

#[tokio::test]
async fn alert_trigger_admission_rolls_back_and_recovers_after_sqlite_restart() {
    for fault in [
        AlertAdmissionFault::SecondInsert,
        AlertAdmissionFault::DeferredCommit,
    ] {
        exercise_alert_rollback_restart(AlertRollbackPhase::Trigger, fault).await;
    }
}

#[tokio::test]
async fn alert_recovery_admission_rolls_back_and_recovers_after_sqlite_restart() {
    for fault in [
        AlertAdmissionFault::SecondInsert,
        AlertAdmissionFault::DeferredCommit,
    ] {
        exercise_alert_rollback_restart(AlertRollbackPhase::Recovery, fault).await;
    }
}

#[tokio::test]
async fn alert_rearm_admission_rolls_back_and_recovers_after_sqlite_restart() {
    for fault in [
        AlertAdmissionFault::SecondInsert,
        AlertAdmissionFault::DeferredCommit,
    ] {
        exercise_alert_rollback_restart(AlertRollbackPhase::Rearm, fault).await;
    }
}

#[tokio::test]
async fn alert_delivery_expiry_preserves_current_authenticated_detail_lookup() {
    use serverbee_server::entity::mobile_push_outbox as outbox;
    let (base, state, _directory, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let operator = login_http(&client, &base, "admin", "late-tap-operator").await;
    let member = login_http(&client, &base, "member", "late-tap-viewer").await;
    let admin = operator["access_token"].as_str().unwrap();
    let access = member["access_token"].as_str().unwrap();
    queued_register(&client, &base, access, "device-a").await;
    let (_server, rule) = alert_http_fixture(&client, &base, admin, "once").await;
    evaluate_alerts(&state).await;
    let jobs = alert_jobs(&state).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].expires_at - jobs[0].created_at, 1800);
    let content = decrypt_alert_envelope(
        &serde_json::from_str(jobs[0].envelope.as_deref().unwrap()).unwrap(),
    );
    let target = content["alert"]["alert_key"].as_str().unwrap();
    // Move only the persisted delivery clock to a past 30-minute window.
    // Detail lookup has its own current authenticated HTTP policy, not this clock.
    let created = Utc::now().timestamp() - 3600;
    outbox::Entity::update_many()
        .col_expr(
            outbox::Column::CreatedAt,
            sea_orm::sea_query::Expr::value(created),
        )
        .col_expr(
            outbox::Column::ExpiresAt,
            sea_orm::sea_query::Expr::value(created + 1800),
        )
        .exec(&state.db)
        .await
        .unwrap();
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    wait_alert_dispatch(&state).await;
    worker.abort();
    let _ = worker.await;
    let expired = alert_jobs(&state).await;
    assert_eq!(expired[0].outcome, "expired");
    assert_eq!(expired[0].expires_at, created + 1800);
    assert!(relay.requests().await.is_empty());
    let url = format!("{base}/api/alert-events/{target}");
    let detail = client.get(&url).bearer_auth(access).send().await.unwrap();
    assert_eq!(
        detail.status(),
        200,
        "Delivery expiry cannot expire a current alert target"
    );
    assert_eq!(
        detail.json::<serde_json::Value>().await.unwrap()["data"]["alert_key"],
        target
    );
    assert_eq!(client.get(&url).send().await.unwrap().status(), 401);
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
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(access)
            .send()
            .await
            .unwrap()
            .status(),
        401,
        "A prior notification never authorizes a revoked current session"
    );
    assert_eq!(
        client
            .delete(format!("{base}/api/alert-rules/{rule}"))
            .bearer_auth(admin)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(admin)
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "An unavailable exact target never opens another alert"
    );
}

async fn event_ws_fixture(
    client: &reqwest::Client,
    base: &str,
    admin: &str,
) -> (
    String,
    String,
    String,
    common::AgentSink,
    common::AgentReader,
) {
    let created = client.post(format!("{base}/api/servers")).bearer_auth(admin)
        .json(&serde_json::json!({"onboarding_request_id":uuid::Uuid::new_v4().to_string(),"name":"Event intent Server"}))
        .send().await.unwrap().json::<serde_json::Value>().await.unwrap();
    let server = created["data"]["server_id"].as_str().unwrap().to_owned();
    let code = created["data"]["enrollment"]["code"].as_str().unwrap();
    let token = format!("test-token-{}", uuid::Uuid::new_v4());
    let registered = client
        .post(format!("{base}/api/agent/register"))
        .bearer_auth(code)
        .json(&serde_json::json!({"proposed_run_token":token}))
        .send()
        .await
        .unwrap();
    assert_eq!(registered.status(), 200);
    let (mut sink, mut reader) = common::connect_agent(base, &token).await;
    assert_eq!(
        common::recv_agent_text(&mut reader).await["type"],
        "welcome"
    );
    // Complete initial address population before enabling the event rule.
    common::send_system_info(&mut sink, &mut reader, "event-baseline", None).await;
    let response = client
        .post(format!("{base}/api/alert-rules"))
        .bearer_auth(admin)
        .json(
            &serde_json::json!({"name":"Once-only IP change","enabled":true,"trigger_mode":"once",
            "cover_type":"include","server_ids":[server],"rules":[{"rule_type":"ip_changed"}]}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let rule = response.json::<serde_json::Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (server, rule, token, sink, reader)
}

fn ip_event_frame(ip: &str, system_info: bool) -> serde_json::Value {
    if system_info {
        serde_json::json!({"type":"system_info","msg_id":"event-report","cpu_name":"Fixture CPU","cpu_cores":1,
            "cpu_arch":"x86_64","os":"Linux","kernel_version":"fixture","mem_total":1024,"swap_total":0,"disk_total":1024,
            "ipv4":ip,"ipv6":null,"virtualization":null,"agent_version":"0.1.0","protocol_version":1,"features":[]})
    } else {
        serde_json::json!({"type":"ip_changed","ipv4":ip,"ipv6":null,"interfaces":[]})
    }
}

async fn send_ip_event(sink: &mut common::AgentSink, ip: &str, system_info: bool) {
    use futures_util::SinkExt;
    sink.send(tokio_tungstenite::tungstenite::Message::Text(
        ip_event_frame(ip, system_info).to_string().into(),
    ))
    .await
    .unwrap();
}

async fn event_ws_barrier(sink: &mut common::AgentSink, reader: &mut common::AgentReader) {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    // FIFO control frame proves the preceding production WS handler completed.
    sink.send(Message::Ping(b"event-intent-barrier".to_vec().into()))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Message::Pong(payload) = reader.next().await.unwrap().unwrap()
                && payload.as_ref() == b"event-intent-barrier"
            {
                return;
            }
        }
    })
    .await
    .expect("production WS event completed");
}

async fn event_intents(
    state: &AppState,
) -> Vec<serverbee_server::entity::alert_event_intent::Model> {
    use sea_orm::QueryOrder;
    use serverbee_server::entity::alert_event_intent as intent;
    intent::Entity::find()
        .order_by_asc(intent::Column::Id)
        .all(&state.db)
        .await
        .unwrap()
}

async fn wait_event_replay(state: &AppState) {
    for _ in 0..100 {
        if event_intents(state).await.is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("Production evaluator did not replay the durable event");
}

async fn exercise_ws_event_restart(
    system_info: bool,
    fault: AlertAdmissionFault,
    disable_second: bool,
) {
    let (base, state, directory, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let operator = login_http(&client, &base, "admin", "event-owner-a").await;
    let member = login_http(&client, &base, "member", "event-owner-b").await;
    let admin = operator["access_token"].as_str().unwrap();
    let other = member["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    queued_register(&client, &base, other, "device-b").await;
    let (server, rule, _token, mut sink, mut reader) =
        event_ws_fixture(&client, &base, admin).await;
    let webhook = attach_alert_webhook(&client, &base, admin, &rule).await;
    fault_alert_admission(&state, fault).await;
    send_ip_event(&mut sink, "203.0.113.8", system_info).await;
    event_ws_barrier(&mut sink, &mut reader).await;
    let captured = event_intents(&state).await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].rule_id, rule);
    assert_eq!(captured[0].server_id, server);
    assert_eq!(captured[0].event_type, "ip_changed");
    assert!(captured[0].should_notify);
    assert_eq!(captured[0].first_triggered_at, captured[0].occurred_at);
    assert!(
        alert_jobs(&state).await.is_empty(),
        "First recipient rolled back with failed admission"
    );
    assert!(
        persisted_alert_cycle(&state, &rule, &server)
            .await
            .is_none()
    );
    assert!(!state.alert_state_manager.is_triggered(&rule, &server, ""));
    assert!(webhook.lock().await.is_empty());
    assert_eq!(
        serverbee_server::entity::server::Entity::find_by_id(&server)
            .one(&state.db)
            .await
            .unwrap()
            .unwrap()
            .ipv4
            .as_deref(),
        Some("203.0.113.8")
    );
    // An unchanged report cannot manufacture another event. A different event
    // while admission is pending uses the reserved once-cycle and is suppressed.
    send_ip_event(&mut sink, "203.0.113.8", system_info).await;
    event_ws_barrier(&mut sink, &mut reader).await;
    assert_eq!(event_intents(&state).await, captured);
    send_ip_event(&mut sink, "203.0.113.9", system_info).await;
    event_ws_barrier(&mut sink, &mut reader).await;
    let pending = event_intents(&state).await;
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0], captured[0]);
    assert_eq!(
        pending[1].first_triggered_at,
        captured[0].first_triggered_at
    );
    assert!(!pending[1].should_notify);
    let restarted = reopen_alert_state(&state, &directory).await;
    // The real startup evaluator tick, not a manual event retry, sees the intent.
    let evaluator = tokio::spawn(serverbee_server::task::alert_evaluator::run(
        restarted.clone(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    evaluator.abort();
    let _ = evaluator.await;
    assert_eq!(event_intents(&restarted).await, pending);
    assert!(alert_jobs(&restarted).await.is_empty());
    assert!(webhook.lock().await.is_empty());
    if disable_second {
        assert_eq!(
            preferences_http(&client, &base, other, 2, intent(false, false))
                .await
                .status(),
            200
        );
    }
    restarted
        .db
        .execute_unprepared("DROP TRIGGER fail_alert_admission")
        .await
        .unwrap();
    let evaluator = tokio::spawn(serverbee_server::task::alert_evaluator::run(
        restarted.clone(),
    ));
    wait_event_replay(&restarted).await;
    // Replay deletes intents before best-effort external dispatch. Wait for the
    // loopback response rather than assuming deletion proves the external call.
    for _ in 0..100 {
        if webhook.lock().await.len() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    evaluator.abort();
    let _ = evaluator.await;
    assert_eq!(*webhook.lock().await, ["triggered"]);
    let jobs = alert_jobs(&restarted).await;
    assert_eq!(jobs.len(), if disable_second { 1 } else { 2 });
    assert!(jobs.iter().all(|job| job.event_id == jobs[0].event_id));
    for job in &jobs {
        assert_eq!(job.created_at, captured[0].occurred_at.timestamp());
        assert_eq!(job.expires_at, captured[0].occurred_at.timestamp() + 1800);
        let content = decrypt_alert_envelope(
            &serde_json::from_str(job.envelope.as_deref().unwrap()).unwrap(),
        );
        assert_eq!(content["event_id"], job.event_id);
        assert_eq!(content["created_at"], job.created_at);
        assert_eq!(content["expires_at"], job.expires_at);
        let expected = persisted_alert_cycle(&restarted, &rule, &server)
            .await
            .unwrap();
        assert_eq!(expected.first_triggered_at, captured[0].first_triggered_at);
        assert_eq!(
            content["alert"]["alert_key"],
            serverbee_server::service::alert::alert_detail_key(&expected)
        );
        if disable_second {
            assert_eq!(job.installation_id, "event-owner-a");
        }
    }
    // Another owner and ordinary startup tick cannot replay external dispatch.
    let final_restart = reopen_alert_state(&restarted, &directory).await;
    let evaluator = tokio::spawn(serverbee_server::task::alert_evaluator::run(
        final_restart.clone(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    evaluator.abort();
    let _ = evaluator.await;
    assert_eq!(alert_jobs(&final_restart).await, jobs);
    assert_eq!(*webhook.lock().await, ["triggered"]);
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(final_restart.clone());
    wait_alert_dispatch(&final_restart).await;
    worker.abort();
    let _ = worker.await;
    assert_eq!(relay.requests().await.len(), jobs.len());
    for job in jobs {
        let receipt = outbox_job(&final_restart, &job.event_id, &job.installation_id).await;
        assert_eq!(receipt.outcome, "accepted");
        assert_eq!(receipt.created_at, job.created_at);
        assert_eq!(receipt.expires_at, job.expires_at);
    }
}

#[tokio::test]
async fn ws_ip_event_intents_recover_insert_and_commit_failures_on_startup() {
    for system_info in [false, true] {
        for fault in [
            AlertAdmissionFault::SecondInsert,
            AlertAdmissionFault::DeferredCommit,
        ] {
            exercise_ws_event_restart(system_info, fault, false).await;
        }
    }
}

#[tokio::test]
async fn ws_ip_event_replay_rechecks_installation_eligibility() {
    exercise_ws_event_restart(false, AlertAdmissionFault::SecondInsert, true).await;
}

#[tokio::test]
async fn ws_ip_intent_capture_failure_does_not_consume_source_update() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    for (system_info, fail_commit) in [(false, false), (true, false), (false, true), (true, true)] {
        let (base, state, directory, _relay) = queued_setup().await;
        let client = reqwest::Client::new();
        let login = login_http(&client, &base, "admin", "capture-owner").await;
        let admin = login["access_token"].as_str().unwrap();
        queued_register(&client, &base, admin, "device-a").await;
        let (server, rule, token, mut sink, mut reader) =
            event_ws_fixture(&client, &base, admin).await;
        let webhook = attach_alert_webhook(&client, &base, admin, &rule).await;
        let fault_sql = if fail_commit {
            "CREATE TABLE source_commit_parent (id INTEGER PRIMARY KEY);
            CREATE TABLE source_commit_child (parent_id INTEGER REFERENCES source_commit_parent(id)
                DEFERRABLE INITIALLY DEFERRED);
            CREATE TRIGGER fail_event_capture AFTER INSERT ON alert_event_intents
            BEGIN INSERT INTO source_commit_child(parent_id) VALUES (1); END"
        } else {
            "CREATE TRIGGER fail_event_capture BEFORE INSERT ON alert_event_intents
            BEGIN SELECT RAISE(ABORT, 'injected capture failure'); END"
        };
        state.db.execute_unprepared(fault_sql).await.unwrap();
        send_ip_event(&mut sink, "203.0.113.8", system_info).await;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match reader.next().await {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                    Some(Ok(Message::Text(text))) => {
                        let message: serde_json::Value = serde_json::from_str(&text).unwrap();
                        assert_ne!(
                            message["msg_id"], "event-report",
                            "Failed capture cannot Ack SystemInfo"
                        );
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("Failed capture closes the source socket for automatic reconnect");
        assert_eq!(
            serverbee_server::entity::server::Entity::find_by_id(&server)
                .one(&state.db)
                .await
                .unwrap()
                .unwrap()
                .ipv4
                .as_deref(),
            Some("1.2.3.4")
        );
        assert!(event_intents(&state).await.is_empty());
        assert!(alert_jobs(&state).await.is_empty());
        assert!(webhook.lock().await.is_empty());
        let restarted = reopen_alert_state(&state, &directory).await;
        restarted
            .db
            .execute_unprepared("DROP TRIGGER fail_event_capture")
            .await
            .unwrap();
        let restarted_base = serve_outbox_http(restarted.clone()).await;
        let (mut sink, mut reader) = common::connect_agent(&restarted_base, &token).await;
        assert_eq!(
            common::recv_agent_text(&mut reader).await["type"],
            "welcome"
        );
        // Agent reconnect sends the SAME current IP in its SystemInfo snapshot;
        // it need not observe a second IP transition to recover the first one.
        send_ip_event(&mut sink, "203.0.113.8", true).await;
        event_ws_barrier(&mut sink, &mut reader).await;
        assert_eq!(
            serverbee_server::entity::server::Entity::find_by_id(&server)
                .one(&restarted.db)
                .await
                .unwrap()
                .unwrap()
                .ipv4
                .as_deref(),
            Some("203.0.113.8")
        );
        assert!(event_intents(&restarted).await.is_empty());
        assert_eq!(alert_jobs(&restarted).await.len(), 1);
        assert_eq!(*webhook.lock().await, ["triggered"]);
    }
}

#[tokio::test]
async fn ws_event_replay_does_not_restart_an_expired_mobile_deadline() {
    use serverbee_server::entity::alert_event_intent as pending;
    let (base, state, directory, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "admin", "expired-event-owner").await;
    let admin = login["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    // The fault needs two recipients to fail after the first actual insertion.
    let other = login_http(&client, &base, "member", "expired-event-other").await;
    queued_register(
        &client,
        &base,
        other["access_token"].as_str().unwrap(),
        "device-b",
    )
    .await;
    let (server, rule, _token, mut sink, mut reader) =
        event_ws_fixture(&client, &base, admin).await;
    let webhook = attach_alert_webhook(&client, &base, admin, &rule).await;
    fault_alert_admission(&state, AlertAdmissionFault::SecondInsert).await;
    send_ip_event(&mut sink, "203.0.113.8", false).await;
    event_ws_barrier(&mut sink, &mut reader).await;
    let original = event_intents(&state).await;
    assert_eq!(original.len(), 1);
    let old = Utc::now() - ChronoDuration::seconds(3600);
    pending::Entity::update_many()
        .col_expr(
            pending::Column::OccurredAt,
            sea_orm::sea_query::Expr::value(old),
        )
        .col_expr(
            pending::Column::FirstTriggeredAt,
            sea_orm::sea_query::Expr::value(old),
        )
        .exec(&state.db)
        .await
        .unwrap();
    state
        .db
        .execute_unprepared("DROP TRIGGER fail_alert_admission")
        .await
        .unwrap();
    let restarted = reopen_alert_state(&state, &directory).await;
    let evaluator = tokio::spawn(serverbee_server::task::alert_evaluator::run(
        restarted.clone(),
    ));
    wait_event_replay(&restarted).await;
    for _ in 0..100 {
        if webhook.lock().await.len() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    evaluator.abort();
    let _ = evaluator.await;
    assert!(
        alert_jobs(&restarted).await.is_empty(),
        "Expired event cannot gain a new mobile window"
    );
    let cycle = persisted_alert_cycle(&restarted, &rule, &server)
        .await
        .unwrap();
    assert_eq!(cycle.first_triggered_at, old);
    assert_eq!(cycle.last_notified_at, old);
    assert!(relay.requests().await.is_empty());
    assert_eq!(
        *webhook.lock().await,
        ["triggered"],
        "Existing external channels retain their own semantics"
    );
}

async fn capability_ws_fixture(
    client: &reqwest::Client,
    base: &str,
    admin: &str,
) -> (
    String,
    String,
    String,
    common::AgentSink,
    common::AgentReader,
) {
    let (server, rule, token, sink, reader) = event_ws_fixture(client, base, admin).await;
    assert_eq!(
        client
            .put(format!("{base}/api/alert-rules/{rule}"))
            .bearer_auth(admin)
            .json(&serde_json::json!({"rules":[{"rule_type":"capability_grant_detected"}]}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    (server, rule, token, sink, reader)
}

fn capability_source_frame(
    at: chrono::DateTime<Utc>,
    cap: &str,
    action: &str,
) -> serde_json::Value {
    serde_json::json!({"type":"capabilities_changed","msg_id":uuid::Uuid::new_v4().to_string(),
        "occurred_at":at,"capabilities":serverbee_common::constants::CAP_DEFAULT,"temporary":[],
        "changes":[{"cap":cap,"action":action,"expires_at":at.timestamp()+3600,
            "granted_by":"root","reason":"original source event"}]})
}

async fn receive_capability_ack(
    sink: &mut common::AgentSink,
    reader: &mut common::AgentReader,
    expected_id: &str,
) -> Result<(), String> {
    use futures_util::{SinkExt, StreamExt};
    use serverbee_common::protocol::ServerMessage;
    use tokio_tungstenite::tungstenite::Message;
    loop {
        let message = reader
            .next()
            .await
            .ok_or("Socket ended before capability Ack")?
            .map_err(|error| error.to_string())?;
        match message {
            Message::Text(text) => {
                // Decode the actual protocol so malformed control payloads are
                // not silently treated as unrelated noise.
                let parsed: ServerMessage =
                    serde_json::from_str(&text).expect("valid Server control frame");
                match parsed {
                    ServerMessage::Ack { msg_id } => {
                        assert_eq!(
                            msg_id, expected_id,
                            "Ack must belong to the current source event"
                        );
                        return Ok(());
                    }
                    ServerMessage::Ping => {
                        sink.send(Message::Text(
                            serde_json::json!({"type":"pong"}).to_string().into(),
                        ))
                        .await
                        .map_err(|error| error.to_string())?;
                    }
                    _ => {
                        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                        assert!(
                            common::is_first_connect_noise(value["type"].as_str()),
                            "Unexpected Server frame before capability Ack: {value}"
                        );
                    }
                }
            }
            Message::Ping(payload) => {
                sink.send(Message::Pong(payload))
                    .await
                    .map_err(|error| error.to_string())?;
            }
            Message::Pong(_) => {}
            Message::Close(_) => return Err("Socket closed before capability Ack".into()),
            other => panic!("Unexpected non-control frame before capability Ack: {other:?}"),
        }
    }
}

async fn capability_send_and_ack(
    sink: &mut common::AgentSink,
    reader: &mut common::AgentReader,
    frame: &serde_json::Value,
) {
    use futures_util::SinkExt;
    // One total deadline includes sending, any number of real desired-state
    // frames, the exact matching Ack and completion of the handler's replay.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        sink.send(tokio_tungstenite::tungstenite::Message::Text(
            frame.to_string().into(),
        ))
        .await
        .unwrap();
        receive_capability_ack(sink, reader, frame["msg_id"].as_str().unwrap())
            .await
            .expect("Current source must receive its owned Ack before socket close");
        event_ws_barrier(sink, reader).await;
    })
    .await
    .expect("Total capability Ack/replay deadline");
}

/// The external Agent boundary retains the original frame on disk and retries
/// automatically after reconnect. Production reporter/source restart is covered
/// separately by the Agent's real loopback reporter test, without helper retries.
async fn retained_capability_source(
    endpoint: tokio::sync::watch::Receiver<String>,
    token: String,
    path: std::path::PathBuf,
    attempted: Arc<std::sync::atomic::AtomicUsize>,
    attempted_endpoint: tokio::sync::watch::Sender<String>,
) {
    use futures_util::{SinkExt, StreamExt};
    loop {
        let base = endpoint.borrow().clone();
        let request_url = format!("{}/api/agent/ws", base.replace("http://", "ws://"));
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = request_url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse().unwrap());
        if let Ok((mut ws, _)) = tokio_tungstenite::connect_async(request).await {
            let welcome = ws.next().await.unwrap().unwrap();
            if let tokio_tungstenite::tungstenite::Message::Text(text) = welcome {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&text).unwrap()["capability_event_ack"],
                    true
                );
            }
            let frame: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let (mut sink, mut reader) = ws.split();
            let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                sink.send(tokio_tungstenite::tungstenite::Message::Text(
                    frame.to_string().into(),
                ))
                .await
                .unwrap();
                attempted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                attempted_endpoint.send(base).unwrap();
                receive_capability_ack(&mut sink, &mut reader, frame["msg_id"].as_str().unwrap())
                    .await
            })
            .await;
            if matches!(outcome, Ok(Ok(()))) {
                std::fs::remove_file(&path).unwrap();
                return;
            }
            // Failed capture closes the real source socket without an Ack;
            // retain the SAME persisted frame for the next automatic attempt.
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

async fn exercise_capability_capture_restart(
    fail_commit: bool,
    expired: bool,
    revoke_recipient: bool,
) {
    use serverbee_server::entity::{audit_log, capability_event_receipt as receipt};
    let (base, state, directory, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "admin", "capability-owner-a").await;
    let other = login_http(&client, &base, "member", "capability-owner-b").await;
    let admin = login["access_token"].as_str().unwrap();
    let other_access = other["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    queued_register(&client, &base, other_access, "device-b").await;
    let (server, rule, token, sink, reader) = capability_ws_fixture(&client, &base, admin).await;
    drop(sink);
    drop(reader);
    let webhook = attach_alert_webhook(&client, &base, admin, &rule).await;
    let original = Utc::now() - ChronoDuration::seconds(if expired { 3600 } else { 120 });
    let frame = capability_source_frame(original, "terminal", "granted");
    let path = directory.path().join("retained-capability-source.json");
    std::fs::write(&path, serde_json::to_vec(&frame).unwrap()).unwrap();
    let sql = if fail_commit {
        "CREATE TABLE capability_capture_parent(id INTEGER PRIMARY KEY);
         CREATE TABLE capability_capture_child(parent_id INTEGER REFERENCES capability_capture_parent(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER fail_capability_capture AFTER INSERT ON alert_event_intents
         BEGIN INSERT INTO capability_capture_child(parent_id) VALUES(1); END"
    } else {
        "CREATE TRIGGER fail_capability_capture BEFORE INSERT ON alert_event_intents
         BEGIN SELECT RAISE(ABORT,'injected capability intent capture failure'); END"
    };
    state.db.execute_unprepared(sql).await.unwrap();
    let (endpoint_tx, endpoint_rx) = tokio::sync::watch::channel(base.clone());
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (attempted_tx, mut attempted_rx) = tokio::sync::watch::channel(String::new());
    let source = tokio::spawn(retained_capability_source(
        endpoint_rx,
        token.clone(),
        path.clone(),
        attempts.clone(),
        attempted_tx,
    ));
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while attempts.load(std::sync::atomic::Ordering::SeqCst) < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("original source automatically reconnects after failed admission");
    assert!(
        path.exists(),
        "No admission Ack consumed the original source"
    );
    assert!(event_intents(&state).await.is_empty());
    assert!(alert_jobs(&state).await.is_empty());
    assert!(
        receipt::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq("capability_temporarily_granted"))
            .all(&state.db)
            .await
            .unwrap()
            .is_empty(),
        "Audit rolled back with failed capture/commit"
    );
    assert!(webhook.lock().await.is_empty());
    if revoke_recipient {
        assert_eq!(
            client
                .post(format!("{base}/api/mobile/auth/logout"))
                .bearer_auth(other_access)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    let restarted = reopen_alert_state(&state, &directory).await;
    let restarted_base = serve_outbox_http(restarted.clone()).await;
    endpoint_tx.send(restarted_base.clone()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let current = attempted_rx.borrow().clone();
            if current == restarted_base {
                break;
            }
            attempted_rx.changed().await.unwrap();
        }
    })
    .await
    .expect("source reconnects to restarted owner before fault release");
    restarted
        .db
        .execute_unprepared("DROP TRIGGER fail_capability_capture")
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), source)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !path.exists(),
        "Only owned durable admission Ack consumes the source"
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while persisted_alert_cycle(&restarted, &rule, &server)
            .await
            .is_none()
            || !event_intents(&restarted).await.is_empty()
        {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let cycle = persisted_alert_cycle(&restarted, &rule, &server)
        .await
        .unwrap();
    assert_eq!(cycle.first_triggered_at, original);
    assert_eq!(cycle.last_notified_at, original);
    let admitted = receipt::Entity::find().all(&restarted.db).await.unwrap();
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0].msg_id, frame["msg_id"].as_str().unwrap());
    assert_eq!(admitted[0].server_id, server);
    assert_eq!(admitted[0].occurred_at, original);
    assert_eq!(
        serverbee_server::entity::server::Entity::find_by_id(&server)
            .one(&restarted.db)
            .await
            .unwrap()
            .unwrap()
            .capabilities as u32,
        serverbee_common::constants::CAP_DEFAULT,
        "Historical Granted metadata cannot restore a revoked capability snapshot"
    );
    let jobs = alert_jobs(&restarted).await;
    assert_eq!(
        jobs.len(),
        if expired {
            0
        } else if revoke_recipient {
            1
        } else {
            2
        }
    );
    for job in &jobs {
        assert_eq!(job.created_at, original.timestamp());
        assert_eq!(job.expires_at, original.timestamp() + 1800);
        assert_eq!(job.event_id, jobs[0].event_id);
        let content = decrypt_alert_envelope(
            &serde_json::from_str(job.envelope.as_deref().unwrap()).unwrap(),
        );
        assert_eq!(
            content["alert"]["alert_key"],
            serverbee_server::service::alert::alert_detail_key(&cycle)
        );
    }
    // Simulate lost Ack/restart: same original identity, current authority snapshot
    // may have changed. No second alert, audit, webhook, or renewed deadline.
    let (mut sink, mut reader) = common::connect_agent(&restarted_base, &token).await;
    common::recv_agent_text(&mut reader).await;
    let mut replay = frame.clone();
    replay["capabilities"] = serde_json::json!(serverbee_common::constants::CAP_DEFAULT);
    capability_send_and_ack(&mut sink, &mut reader, &replay).await;
    assert_eq!(alert_jobs(&restarted).await, jobs);
    assert_eq!(
        receipt::Entity::find()
            .all(&restarted.db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq("capability_temporarily_granted"))
            .all(&restarted.db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        *webhook.lock().await,
        ["triggered"],
        "External channels retain independent expiry and send once"
    );
    let final_restart = reopen_alert_state(&restarted, &directory).await;
    let evaluator = tokio::spawn(serverbee_server::task::alert_evaluator::run(
        final_restart.clone(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    evaluator.abort();
    let _ = evaluator.await;
    assert_eq!(alert_jobs(&final_restart).await, jobs);
    let final_base = serve_outbox_http(final_restart.clone()).await;
    let (mut sink, mut reader) = common::connect_agent(&final_base, &token).await;
    common::recv_agent_text(&mut reader).await;
    capability_send_and_ack(&mut sink, &mut reader, &frame).await;
    assert_eq!(alert_jobs(&final_restart).await, jobs);
    assert_eq!(
        receipt::Entity::find()
            .all(&final_restart.db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(*webhook.lock().await, ["triggered"]);
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(final_restart.clone());
    wait_alert_dispatch(&final_restart).await;
    worker.abort();
    let _ = worker.await;
    assert_eq!(relay.requests().await.len(), jobs.len());
}

#[tokio::test]
async fn ws_capability_capture_insert_and_commit_failure_automatically_replay_original_source() {
    exercise_capability_capture_restart(false, false, false).await;
    exercise_capability_capture_restart(true, false, false).await;
}

#[tokio::test]
async fn ws_capability_capture_replay_preserves_expiry_and_current_recipient_ownership() {
    exercise_capability_capture_restart(false, true, false).await;
    exercise_capability_capture_restart(true, false, true).await;
}

#[tokio::test]
async fn ws_capability_low_risk_revoked_expired_and_once_suppression_remain_distinct() {
    let (base, state, _directory, _relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "admin", "capability-gates-owner").await;
    let admin = login["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    let (server, rule, _token, mut sink, mut reader) =
        capability_ws_fixture(&client, &base, admin).await;
    let webhook = attach_alert_webhook(&client, &base, admin, &rule).await;
    for (cap, action) in [
        ("ping", "granted"),
        ("terminal", "expired"),
        ("terminal", "revoked"),
    ] {
        capability_send_and_ack(
            &mut sink,
            &mut reader,
            &capability_source_frame(Utc::now(), cap, action),
        )
        .await;
        assert!(alert_jobs(&state).await.is_empty());
        assert!(
            persisted_alert_cycle(&state, &rule, &server)
                .await
                .is_none()
        );
    }
    let first = capability_source_frame(Utc::now(), "terminal", "granted");
    capability_send_and_ack(&mut sink, &mut reader, &first).await;
    let jobs = alert_jobs(&state).await;
    assert_eq!(jobs.len(), 1);
    capability_send_and_ack(
        &mut sink,
        &mut reader,
        &capability_source_frame(Utc::now(), "exec", "granted"),
    )
    .await;
    assert_eq!(
        alert_jobs(&state).await,
        jobs,
        "A distinct new grant still honors once suppression"
    );
    assert_eq!(
        persisted_alert_cycle(&state, &rule, &server)
            .await
            .unwrap()
            .count,
        2
    );
    assert_eq!(*webhook.lock().await, ["triggered"]);
}

#[tokio::test]
async fn ws_capability_replay_rejects_changed_identity_and_superseded_socket() {
    use futures_util::{SinkExt, StreamExt};
    use serverbee_server::entity::capability_event_receipt as receipt;
    use tokio_tungstenite::tungstenite::Message;
    let (base, state, _directory, _relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "admin", "capability-connection-owner").await;
    let admin = login["access_token"].as_str().unwrap();
    queued_register(&client, &base, admin, "device-a").await;
    let (server, rule, token, mut old_sink, mut old_reader) =
        capability_ws_fixture(&client, &base, admin).await;
    let (mut current_sink, mut current_reader) = common::connect_agent(&base, &token).await;
    common::recv_agent_text(&mut current_reader).await;
    let source = capability_source_frame(Utc::now(), "terminal", "granted");
    let _ = old_sink
        .send(Message::Text(source.to_string().into()))
        .await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(Ok(message)) = old_reader.next().await {
            if matches!(message, Message::Close(_)) {
                break;
            }
            if let Message::Text(text) = message {
                assert_ne!(
                    serde_json::from_str::<serde_json::Value>(&text).unwrap()["msg_id"],
                    source["msg_id"]
                );
            }
        }
    })
    .await
    .expect("Superseded socket cannot acknowledge retained source");
    assert!(
        receipt::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        persisted_alert_cycle(&state, &rule, &server)
            .await
            .is_none()
    );
    capability_send_and_ack(&mut current_sink, &mut current_reader, &source).await;
    let jobs = alert_jobs(&state).await;
    let cycle = persisted_alert_cycle(&state, &rule, &server).await.unwrap();
    let mut changed = source.clone();
    changed["changes"][0]["cap"] = serde_json::json!("exec");
    current_sink
        .send(Message::Text(changed.to_string().into()))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(Ok(message)) = current_reader.next().await {
            if matches!(message, Message::Close(_)) {
                break;
            }
            if let Message::Text(text) = message {
                assert_ne!(
                    serde_json::from_str::<serde_json::Value>(&text).unwrap()["msg_id"],
                    source["msg_id"]
                );
            }
        }
    })
    .await
    .expect("Changed content cannot reuse an admitted source identity");
    assert_eq!(alert_jobs(&state).await, jobs);
    assert_eq!(
        persisted_alert_cycle(&state, &rule, &server).await.unwrap(),
        cycle
    );
    assert_eq!(
        receipt::Entity::find().all(&state.db).await.unwrap().len(),
        1
    );
}
