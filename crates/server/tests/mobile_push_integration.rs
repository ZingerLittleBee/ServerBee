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
    assert!(row.is_none(), "owner unregister should delete their own row");
}
