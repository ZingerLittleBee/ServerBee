//! Renewal reminders through authenticated HTTP, real persistent SQLite and the production tick.
mod common;
use chrono::{DateTime, Utc};
use common::{create_server, http_client, login_admin};
use serde_json::{Value, json};
use serverbee_server::{service::renewal_clock::RenewalClock, state::AppState};
use std::sync::{Arc, RwLock};

struct ManualClock(RwLock<DateTime<Utc>>);
impl ManualClock {
    fn at(value: &str) -> Arc<Self> {
        Arc::new(Self(RwLock::new(value.parse().unwrap())))
    }
    fn set(&self, value: &str) {
        *self.0.write().unwrap() = value.parse().unwrap();
    }
}
impl RenewalClock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.read().unwrap()
    }
}
async fn get(admin: &reqwest::Client, base: &str, path: &str) -> Value {
    let response = admin
        .get(format!("{base}/api/{path}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.json::<Value>().await.unwrap()["data"].clone()
}
async fn configure(admin: &reqwest::Client, base: &str, days: u32) -> (String, String) {
    let id = create_server(admin, base, "offline-renewal-reminder").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","renewal":{"enabled":true,"expiry_date":"2026-10-31","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let response = admin.post(format!("{base}/api/alert-rules"))
        .json(&json!({"name":"renewal reminder","rules":[{"rule_type":"expiration","duration":days}],"trigger_mode":"once","enabled":true,"cover_type":"include","server_ids":[id]}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    (id, body["data"]["id"].as_str().unwrap().to_string())
}
async fn tick(state: &AppState) {
    serverbee_server::task::alert_evaluator::evaluate_once(state)
        .await
        .unwrap();
}
#[tokio::test]
async fn once_reminder_rearms_for_next_month_even_when_sixty_day_condition_stays_true() {
    let clock = ManualClock::at("2026-10-31T12:00:00Z");
    let (base, _tmp, state) =
        common::start_test_server_with_renewal_clock(100, clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, rule) = configure(&admin, &base, 60).await;
    tick(&state).await;
    let first = get(&admin, &base, "alert-events").await;
    assert_eq!(first.as_array().unwrap().len(), 1);
    clock.set("2026-11-01T04:00:00Z");
    tick(&state).await;
    assert_eq!(
        get(&admin, &base, &format!("servers/{id}")).await["renewal"]["expiry_date"],
        "2026-11-30"
    );
    let events = get(&admin, &base, "alert-events").await;
    assert_eq!(
        events.as_array().unwrap().len(),
        2,
        "each current monthly occurrence must admit its own once reminder: {events}"
    );
    assert_ne!(events[0]["alert_key"], events[1]["alert_key"]);
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["rule_id"] == rule)
    );
}

#[tokio::test]
async fn explicit_confirmed_dates_each_admit_once_even_when_reminder_window_stays_true() {
    let clock = ManualClock::at("2026-10-31T12:00:00Z");
    let (base, _tmp, state) = common::start_test_server_with_renewal_clock(100, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, _) = configure(&admin, &base, 60).await;
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":false,"expiry_date":"2026-11-15"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    tick(&state).await;
    let first = get(&admin, &base, "alert-events").await;
    assert_eq!(first.as_array().unwrap().len(), 1);
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"expiry_date":"2026-11-30"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    tick(&state).await;
    let events = get(&admin, &base, "alert-events").await;
    assert_eq!(
        events.as_array().unwrap().len(),
        2,
        "operator-confirmed current dates also carry independent renewal occurrences"
    );
}

#[tokio::test]
async fn automatic_rollover_supersedes_previous_alert_without_a_recovery_claim() {
    let clock = ManualClock::at("2026-10-31T12:00:00Z");
    let (base, _tmp, state) =
        common::start_test_server_with_renewal_clock(100, clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (_, rule) = configure(&admin, &base, 60).await;
    tick(&state).await;
    let before = get(&admin, &base, "alert-events").await;
    let key = before[0]["alert_key"].as_str().unwrap().to_string();
    clock.set("2026-11-01T04:00:00Z");
    tick(&state).await;
    let detail = get(&admin, &base, &format!("alert-events/{key}")).await;
    assert_eq!(
        detail["status"], "superseded",
        "advancement is a new reminder target, never service recovery"
    );
    assert!(detail["resolved_at"].is_null());
    assert!(!detail["message"].as_str().unwrap().contains("resolved"));
    let states = get(&admin, &base, &format!("alert-rules/{rule}/states")).await;
    assert_eq!(
        states
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["status"] == "superseded")
            .count(),
        1
    );
}

#[tokio::test]
async fn zero_day_reminder_is_eligible_through_the_complete_local_expiry_date() {
    let clock = ManualClock::at("2026-10-31T03:59:59Z");
    let (base, _tmp, state) =
        common::start_test_server_with_renewal_clock(100, clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, _) = configure(&admin, &base, 0).await;
    tick(&state).await;
    assert!(
        get(&admin, &base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    clock.set("2026-10-31T04:00:00Z");
    tick(&state).await;
    assert_eq!(
        get(&admin, &base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .len(),
        1,
        "zero-day is eligible at the beginning of the billing timezone's expiry date"
    );
    clock.set("2026-11-01T03:59:59.999999999Z");
    tick(&state).await;
    assert_eq!(
        get(&admin, &base, &format!("servers/{id}")).await["renewal"]["expiry_date"],
        "2026-10-31"
    );
    assert_eq!(
        get(&admin, &base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    clock.set("2026-11-01T04:00:00Z");
    tick(&state).await;
    assert_eq!(
        get(&admin, &base, &format!("servers/{id}")).await["renewal"]["expiry_date"],
        "2026-11-30"
    );
    assert_eq!(
        get(&admin, &base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .len(),
        1,
        "rollover does not invent another reminder outside the new window"
    );
    clock.set("2026-11-30T05:00:00Z");
    tick(&state).await;
    assert_eq!(
        get(&admin, &base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .len(),
        2,
        "the next local expiry date remains eligible after the DST change"
    );
}

#[tokio::test]
async fn production_reminder_evaluation_catches_up_inside_its_admission_snapshot() {
    let clock = ManualClock::at("2026-10-31T12:00:00Z");
    let (base, _tmp, state) =
        common::start_test_server_with_renewal_clock(100, clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, _) = configure(&admin, &base, 60).await;
    clock.set("2027-07-13T12:00:00Z");
    // The same production alert entry point must select/advance/admit atomically,
    // rather than relying on a separately committed, earlier scheduler read.
    serverbee_server::service::alert::AlertService::evaluate_all_at(
        &state.db,
        &state.config,
        &state.agent_manager,
        &state.alert_state_manager,
        clock.now(),
    )
    .await
    .unwrap();
    let model = get(&admin, &base, &format!("servers/{id}")).await;
    assert_eq!(
        model["renewal"]["expiry_date"], "2027-07-31",
        "only the current anchored occurrence can be admitted after downtime"
    );
    let events = get(&admin, &base, "alert-events").await;
    assert_eq!(
        events.as_array().unwrap().len(),
        1,
        "no historical reminder backlog"
    );
    assert_eq!(
        model["renewal"]["confirmed_expired_at"],
        "2026-11-01T03:59:59.999999999Z"
    );
}

async fn reopen(
    tmp: &tempfile::TempDir,
    clock: Arc<ManualClock>,
    config: serverbee_server::config::AppConfig,
) -> (String, Arc<AppState>) {
    use sea_orm_migration::MigratorTrait;
    use sqlx::{
        ConnectOptions as _,
        sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    };
    let options = SqliteConnectOptions::new()
        .filename(tmp.path().join("test.db"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5))
        .disable_statement_logging();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .unwrap();
    let db = sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);
    serverbee_server::migration::Migrator::up(&db, None)
        .await
        .unwrap();
    let state = AppState::new_with_renewal_clock(db, config, clock)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = serverbee_server::router::create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (base, state)
}

#[tokio::test]
async fn retries_concurrent_owners_database_reopen_and_freeze_keep_one_admission_per_occurrence() {
    let clock = ManualClock::at("2026-10-31T12:00:00Z");
    let (base, tmp, state) = common::start_test_server_with_renewal_clock(100, clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, _) = configure(&admin, &base, 60).await;
    let (second_base, second) = reopen(&tmp, clock.clone(), state.config.clone()).await;
    tokio::join!(tick(&state), tick(&second));
    tick(&state).await;
    let first = get(&admin, &base, "alert-events").await;
    assert_eq!(first.as_array().unwrap().len(), 1);
    assert_eq!(first[0]["count"], 1);
    clock.set("2026-11-01T04:00:00Z");
    tokio::join!(tick(&second), tick(&state));
    let model = get(&admin, &second_base, &format!("servers/{id}")).await;
    let response = admin
        .put(format!("{second_base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":false}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let frozen = get(&admin, &base, &format!("servers/{id}")).await;
    assert_eq!(
        model["renewal"]["occurrence_id"],
        frozen["renewal"]["occurrence_id"]
    );
    let (third_base, third) = reopen(&tmp, clock, state.config.clone()).await;
    tokio::join!(tick(&third), tick(&state), tick(&second));
    let events = get(&admin, &third_base, "alert-events").await;
    assert_eq!(events.as_array().unwrap().len(), 2);
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["count"] == 1)
    );
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["alert_key"] == first[0]["alert_key"])
    );
}

async fn register_encrypted_alerts(admin: &reqwest::Client, base: &str) {
    let response = admin.post(format!("{base}/api/mobile/auth/login"))
        .json(&json!({"username":"admin","password":"testpass","installation_id":"renewal-test-install","device_name":"isolated fixture"}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    let token = body["data"]["access_token"].as_str().unwrap();
    let response = admin.put(format!("{base}/api/mobile/push/settings")).bearer_auth(token)
        .json(&json!({"expected_revision":0,"preferences":{"enabled":true,"alerts":true,"security":false,"task_success":false,"task_failure":false}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let response = admin.post(format!("{base}/api/mobile/push/encrypted-register")).bearer_auth(token)
        .json(&json!({"expected_revision":1,"device_token":"a".repeat(64),"environment":"sandbox",
            "content_key_id":"22222222-2222-4222-8222-222222222222","content_key":"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=","deployment_id":"https://serverbee.test"}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
}
async fn jobs(state: &AppState) -> Vec<serverbee_server::entity::mobile_push_outbox::Model> {
    use sea_orm::EntityTrait;
    serverbee_server::entity::mobile_push_outbox::Entity::find()
        .all(&state.db)
        .await
        .unwrap()
}
async fn admit_direct(state: &AppState, now: DateTime<Utc>) {
    serverbee_server::service::alert::AlertService::evaluate_all_at(
        &state.db,
        &state.config,
        &state.agent_manager,
        &state.alert_state_manager,
        now,
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn encrypted_queue_fault_rolls_back_calendar_and_admission_then_retry_and_reopen_are_deduplicated()
 {
    use sea_orm::ConnectionTrait;
    let clock = ManualClock::at("2026-10-31T12:00:00Z");
    let (_base, tmp, initial) =
        common::start_test_server_with_renewal_clock(100, clock.clone()).await;
    let mut config = initial.config.clone();
    config.push_relay.url = "https://unused-fixture-relay.invalid".into();
    let (base, state) = reopen(&tmp, clock.clone(), config).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, _) = configure(&admin, &base, 60).await;
    register_encrypted_alerts(&admin, &base).await;
    state.db.execute_unprepared("CREATE TRIGGER renewal_queue_failure BEFORE INSERT ON mobile_push_outbox BEGIN SELECT RAISE(FAIL,'isolated admission fault'); END").await.unwrap();
    clock.set("2026-11-01T04:00:00Z");
    admit_direct(&state, clock.now()).await;
    assert_eq!(
        get(&admin, &base, &format!("servers/{id}")).await["renewal"]["expiry_date"],
        "2026-10-31",
        "failed queue admission cannot commit the calendar snapshot"
    );
    assert!(
        get(&admin, &base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(jobs(&state).await.is_empty());
    state
        .db
        .execute_unprepared("DROP TRIGGER renewal_queue_failure")
        .await
        .unwrap();
    admit_direct(&state, clock.now()).await;
    assert_eq!(
        get(&admin, &base, &format!("servers/{id}")).await["renewal"]["expiry_date"],
        "2026-11-30"
    );
    let original = jobs(&state).await;
    assert_eq!(original.len(), 1);
    let envelope = original[0].envelope.as_ref().unwrap();
    for forbidden in [
        "offline-renewal-reminder",
        "renewal reminder",
        "server_id",
        "content_key",
    ] {
        assert!(!envelope.contains(forbidden));
    }
    let (next_base, next) = reopen(&tmp, clock.clone(), state.config.clone()).await;
    tokio::join!(
        admit_direct(&state, clock.now()),
        admit_direct(&next, clock.now())
    );
    assert_eq!(jobs(&next).await, original);
    assert_eq!(
        get(&admin, &next_base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn authenticated_catalog_signal_refetches_current_deadline_after_advancement_and_edits() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let clock = ManualClock::at("2026-10-31T12:00:00Z");
    let (base, _tmp, state) =
        common::start_test_server_with_renewal_clock(100, clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, _) = configure(&admin, &base, 60).await;
    let response = admin
        .post(format!("{base}/api/auth/api-keys"))
        .json(&json!({"name":"isolated catalog reader"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let key: Value = response.json().await.unwrap();
    let ws_url = format!("{}/api/ws/servers", base.replace("http://", "ws://"));
    assert!(
        matches!(tokio_tungstenite::connect_async(&ws_url).await, Err(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status() == 401)
    );
    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().insert(
        "x-api-key",
        key["data"]["key"].as_str().unwrap().parse().unwrap(),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let frame = socket.next().await.unwrap().unwrap();
    let full_sync: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(full_sync["type"], "full_sync");
    for server in full_sync["servers"].as_array().unwrap() {
        for private in ["expired_at", "renewal", "price", "billing_cycle"] {
            assert!(server.get(private).is_none());
        }
    }
    clock.set("2026-11-01T04:00:00Z");
    tick(&state).await;
    let frame = tokio::time::timeout(std::time::Duration::from_secs(1), socket.next())
        .await
        .expect("automatic advancement must invalidate authenticated REST catalog readers")
        .unwrap()
        .unwrap();
    let changed: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(
        changed,
        json!({"type":"server_catalog_changed","server_ids":[id]})
    );
    let detail = get(&admin, &base, &format!("servers/{id}")).await;
    let list = get(&admin, &base, "servers").await;
    assert_eq!(detail["renewal"]["expiry_date"], "2026-11-30");
    assert_eq!(
        list.as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap()["expired_at"],
        detail["expired_at"]
    );
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":false,"expiry_date":"2026-12-31"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let frame = tokio::time::timeout(std::time::Duration::from_secs(1), socket.next())
        .await
        .expect("renewal edits must invalidate authenticated catalog readers")
        .unwrap()
        .unwrap();
    let changed: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(
        changed,
        json!({"type":"server_catalog_changed","server_ids":[id]})
    );
    assert_eq!(
        get(&admin, &base, &format!("servers/{id}")).await["renewal"]["expiry_date"],
        "2026-12-31"
    );
    tick(&state).await;
    let frame = tokio::time::timeout(std::time::Duration::from_secs(1), socket.next())
        .await
        .expect("committed reminder retargeting must refresh earlier cached alert occurrences")
        .unwrap()
        .unwrap();
    let changed: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(
        changed,
        json!({"type":"server_catalog_changed","server_ids":[id]})
    );
    let events = get(&admin, &base, "alert-events").await;
    assert_eq!(
        events
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["status"] == "firing")
            .count(),
        1
    );
    assert_eq!(
        events
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["status"] == "superseded")
            .count(),
        1
    );
}

#[tokio::test]
async fn current_occurrence_resolves_when_expiration_condition_is_edited_false() {
    let clock = ManualClock::at("2026-10-30T12:00:00Z");
    let (base, _tmp, state) = common::start_test_server_with_renewal_clock(100, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, rule) = configure(&admin, &base, 60).await;
    tick(&state).await;
    let events = get(&admin, &base, "alert-events").await;
    let key = events[0]["alert_key"].as_str().unwrap();
    let occurrence =
        get(&admin, &base, &format!("servers/{id}")).await["renewal"]["occurrence_id"].clone();
    let response = admin
        .put(format!("{base}/api/alert-rules/{rule}"))
        .json(&json!({"rules":[{"rule_type":"expiration","duration":0}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    tick(&state).await;
    let detail = get(&admin, &base, &format!("alert-events/{key}")).await;
    assert_eq!(
        detail["status"], "resolved",
        "a false condition still recovers the current occurrence"
    );
    assert!(detail["resolved_at"].is_string());
    assert_eq!(
        get(&admin, &base, &format!("servers/{id}")).await["renewal"]["occurrence_id"],
        occurrence
    );
    let response = admin
        .put(format!("{base}/api/alert-rules/{rule}"))
        .json(&json!({"rules":[{"rule_type":"expiration","duration":60}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    tick(&state).await;
    assert_eq!(
        get(&admin, &base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .len(),
        1,
        "once remains consumed for the same occurrence"
    );
}

#[tokio::test]
async fn explicitly_clearing_expiry_resolves_its_retained_occurrence() {
    let clock = ManualClock::at("2026-10-31T12:00:00Z");
    let (base, _tmp, state) = common::start_test_server_with_renewal_clock(100, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let (id, _) = configure(&admin, &base, 60).await;
    tick(&state).await;
    let events = get(&admin, &base, "alert-events").await;
    let key = events[0]["alert_key"].as_str().unwrap();
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":false,"expiry_date":null}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    tick(&state).await;
    let detail = get(&admin, &base, &format!("alert-events/{key}")).await;
    assert_eq!(
        detail["status"], "resolved",
        "an explicit cleared date cannot leave its old occurrence firing"
    );
    assert!(detail["resolved_at"].is_string());
    assert!(get(&admin, &base, &format!("servers/{id}")).await["expired_at"].is_null());
    assert_eq!(
        get(&admin, &base, "alert-events")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
