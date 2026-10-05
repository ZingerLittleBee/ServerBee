//! Opt-in renewal projection through authenticated HTTP and persistent SQLite.
mod common;
use common::{create_server, http_client, login_admin, start_test_server};
use serde_json::{Value, json};

#[tokio::test]
async fn automatic_renewal_requires_complete_resulting_configuration_atomically() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "manual-host").await;
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"name":"must-not-change","renewal":{"enabled":true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        422,
        "enabled renewal must require date and interval"
    );
    let body: Value = admin
        .get(format!("{base}/api/servers/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["name"], "manual-host");
    assert_eq!(body["data"]["renewal"]["enabled"], false);
}

#[tokio::test]
async fn opt_in_projects_without_promoting_operator_history_to_payment_information() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "expected-renewal").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","price":12.5,"currency":"USD","traffic_limit":1024,"billing_start_day":7,"renewal":{"enabled":true,"expiry_date":"2099-01-31","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["renewal"]["deadline_origin"], "projected");
    assert_eq!(
        body["data"]["renewal"]["confirmed_expired_at"],
        "2099-02-01T04:59:59.999999999Z"
    );
    assert!(
        body["data"]["renewal"]["occurrence_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
}

use chrono::{DateTime, Utc};
use std::sync::{Arc, RwLock};
struct ManualClock(RwLock<DateTime<Utc>>);
impl ManualClock {
    fn at(value: &str) -> Arc<Self> {
        Arc::new(Self(RwLock::new(instant(value))))
    }
    fn set(&self, value: &str) {
        *self.0.write().unwrap() = instant(value);
    }
}
impl serverbee_server::service::renewal_clock::RenewalClock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.read().unwrap()
    }
}
fn instant(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}
async fn controlled_server(
    clock: Arc<ManualClock>,
) -> (
    String,
    tempfile::TempDir,
    Arc<serverbee_server::state::AppState>,
) {
    common::start_test_server_with_renewal_clock(100, clock).await
}
async fn detail(admin: &reqwest::Client, base: &str, id: &str) -> Value {
    let body: Value = admin
        .get(format!("{base}/api/servers/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["data"].clone()
}

#[tokio::test]
async fn enabling_overdue_month_end_catches_up_from_original_anchor() {
    let clock = ManualClock::at("2026-03-01T05:00:00Z");
    let (base, _tmp, _state) = controlled_server(clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "offline-renewal").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","renewal":{"enabled":true,"expiry_date":"2026-01-31","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let model = detail(&admin, &base, &id).await;
    assert_eq!(model["renewal"]["expiry_date"], "2026-03-31");
    assert_eq!(model["expired_at"], "2026-04-01T03:59:59.999999999Z");
    assert_eq!(
        model["renewal"]["confirmed_expired_at"],
        "2026-02-01T04:59:59.999999999Z"
    );
}

#[tokio::test]
async fn scheduled_evaluation_advances_offline_month_end_and_restores_anchor_after_dst() {
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, _tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "offline-month-end").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","renewal":{"enabled":true,"expiry_date":"2026-01-31","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    clock.set("2026-02-01T05:00:00Z");
    // The production scheduler's first tick is immediate; no client opens a billing page.
    let task = tokio::spawn(serverbee_server::task::alert_evaluator::run(state.clone()));
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    task.abort();
    let model = detail(&admin, &base, &id).await;
    assert_eq!(model["renewal"]["expiry_date"], "2026-02-28");
    assert_eq!(model["expired_at"], "2026-03-01T04:59:59.999999999Z");
}

async fn reopen(
    tmp: &tempfile::TempDir,
    clock: Arc<ManualClock>,
) -> (String, Arc<serverbee_server::state::AppState>) {
    use serverbee_server::{config::AppConfig, router::create_router, state::AppState};
    let db = sea_orm::Database::connect(format!(
        "sqlite://{}?mode=rwc",
        tmp.path().join("test.db").display()
    ))
    .await
    .unwrap();
    let mut config = AppConfig::default();
    config.auth.secure_cookie = false;
    config.server.data_dir = tmp.path().to_str().unwrap().into();
    let state = AppState::new_with_renewal_clock(db, config, clock)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (base, state)
}

#[tokio::test]
async fn startup_reopens_persistent_schedule_and_catches_up_without_a_client_or_agent() {
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "restart-renewal").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","price":12.5,"currency":"USD","traffic_limit":1024,"billing_start_day":7,"renewal":{"enabled":true,"expiry_date":"2026-01-31","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    state.db.clone().close().await.unwrap();
    clock.set("2026-06-01T04:00:00Z");
    let (base, _restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let model = detail(&admin, &base, &id).await;
    assert_eq!(model["renewal"]["expiry_date"], "2026-06-30");
    assert_eq!(model["expired_at"], "2026-07-01T03:59:59.999999999Z");
    assert_eq!(
        model["renewal"]["confirmed_expired_at"],
        "2026-02-01T04:59:59.999999999Z"
    );
    assert_eq!(model["price"], 12.5);
    assert_eq!(model["currency"], "USD");
    assert_eq!(model["billing_cycle"], "monthly");
    assert_eq!(model["traffic_limit"], 1024);
    assert_eq!(model["billing_start_day"], 7);
}

#[tokio::test]
async fn disabling_freezes_the_same_projected_deadline_and_occurrence_after_restart() {
    let clock = ManualClock::at("2026-02-01T05:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "freeze-renewal").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","renewal":{"enabled":true,"expiry_date":"2026-01-31","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let before = detail(&admin, &base, &id).await;
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":false}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let frozen = detail(&admin, &base, &id).await;
    assert_eq!(frozen["renewal"]["deadline_origin"], "frozen");
    assert_eq!(frozen["expired_at"], before["expired_at"]);
    assert_eq!(
        frozen["renewal"]["occurrence_id"],
        before["renewal"]["occurrence_id"]
    );
    assert_eq!(
        frozen["renewal"]["confirmed_expired_at"],
        before["renewal"]["confirmed_expired_at"]
    );
    state.db.clone().close().await.unwrap();
    clock.set("2026-06-01T04:00:00Z");
    let (base, restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    serverbee_server::task::alert_evaluator::evaluate_once(&restarted)
        .await
        .unwrap();
    let persisted = detail(&admin, &base, &id).await;
    assert_eq!(persisted["renewal"], frozen["renewal"]);
    assert_eq!(persisted["expired_at"], frozen["expired_at"]);
}

#[tokio::test]
async fn enabled_prerequisites_cannot_be_cleared_but_disabling_and_clearing_is_atomic() {
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, _tmp, _state) = controlled_server(clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "required-settings").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","renewal":{"enabled":true,"expiry_date":"2026-01-31","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let before = detail(&admin, &base, &id).await;
    for invalid in [
        json!({"name":"must-not-change","renewal":{"expiry_date":null}}),
        json!({"name":"must-not-change","billing_cycle":null}),
        json!({"name":"must-not-change","renewal":{"billing_timezone":null}}),
        json!({"name":"must-not-change","renewal":{"billing_timezone":"Mars/Olympus"}}),
    ] {
        let response = admin
            .put(format!("{base}/api/servers/{id}"))
            .json(&invalid)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            422,
            "required input cannot be cleared while enabled: {invalid}"
        );
        let after = detail(&admin, &base, &id).await;
        assert_eq!(after["name"], before["name"]);
        assert_eq!(after["renewal"], before["renewal"]);
        assert_eq!(after["expired_at"], before["expired_at"]);
        assert_eq!(after["billing_cycle"], before["billing_cycle"]);
    }
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":null,"renewal":{"enabled":false,"expiry_date":null,"billing_timezone":null}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let after = detail(&admin, &base, &id).await;
    assert_eq!(after["renewal"]["enabled"], false);
    assert!(after["expired_at"].is_null());
    assert!(after["billing_cycle"].is_null());
}

#[tokio::test]
async fn enabling_legacy_instant_keeps_entire_selected_day_and_confirmed_history() {
    let clock = ManualClock::at("2026-01-31T18:00:00Z");
    let (base, _tmp, _state) = controlled_server(clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let body: Value = admin.post(format!("{base}/api/servers"))
        .json(&json!({"onboarding_request_id":uuid::Uuid::new_v4().to_string(),"name":"legacy-calendar","billing_cycle":"monthly","expired_at":"2026-01-31T12:34:56Z"}))
        .send().await.unwrap().json().await.unwrap();
    let id = body["data"]["server_id"].as_str().unwrap();
    let before = detail(&admin, &base, id).await;
    assert_eq!(before["expired_at"], "2026-01-31T12:34:56Z");
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let after = detail(&admin, &base, id).await;
    assert_eq!(
        after["renewal"]["expiry_date"], "2026-01-31",
        "today's full local date has not expired"
    );
    assert_eq!(after["expired_at"], "2026-01-31T23:59:59.999999999Z");
    assert_eq!(
        after["renewal"]["confirmed_expired_at"],
        "2026-01-31T12:34:56Z"
    );
}
