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
    // The scheduler invokes this same production iteration; time moves directly.
    serverbee_server::task::alert_evaluator::evaluate_once(&state)
        .await
        .unwrap();
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

async fn configure(
    admin: &reqwest::Client,
    base: &str,
    id: &str,
    cycle: &str,
    date: &str,
    zone: &str,
) {
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":cycle,"renewal":{"enabled":true,"expiry_date":date,"billing_timezone":zone}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
}
async fn tick(state: &serverbee_server::state::AppState) {
    serverbee_server::task::alert_evaluator::evaluate_once(state)
        .await
        .unwrap();
}

#[tokio::test]
async fn current_local_date_remains_valid_until_its_last_nanosecond() {
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, _tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "full-local-day").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    let original = detail(&admin, &base, &id).await;
    for time in ["2026-02-01T04:59:59Z", "2026-02-01T04:59:59.999999999Z"] {
        clock.set(time);
        tick(&state).await;
        let current = detail(&admin, &base, &id).await;
        assert_eq!(current["renewal"], original["renewal"]);
        assert_eq!(current["expired_at"], original["expired_at"]);
    }
    clock.set("2026-02-01T05:00:00Z");
    tick(&state).await;
    assert_eq!(
        detail(&admin, &base, &id).await["renewal"]["expiry_date"],
        "2026-02-28"
    );
    clock.set("2026-03-01T05:00:00Z");
    tick(&state).await;
    let march = detail(&admin, &base, &id).await;
    assert_eq!(march["renewal"]["expiry_date"], "2026-03-31");
    assert_eq!(march["expired_at"], "2026-04-01T03:59:59.999999999Z");
    assert_eq!(
        march["renewal"]["confirmed_expired_at"],
        original["renewal"]["confirmed_expired_at"]
    );
}

#[tokio::test]
async fn quarterly_month_end_clamps_and_restores_the_original_day() {
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, _tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "quarterly-anchor").await;
    configure(
        &admin,
        &base,
        &id,
        "quarterly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    clock.set("2026-02-01T05:00:00Z");
    tick(&state).await;
    let april = detail(&admin, &base, &id).await;
    assert_eq!(april["renewal"]["expiry_date"], "2026-04-30");
    assert_eq!(april["expired_at"], "2026-05-01T03:59:59.999999999Z");
    clock.set("2026-05-01T04:00:00Z");
    tick(&state).await;
    let july = detail(&admin, &base, &id).await;
    assert_eq!(july["renewal"]["expiry_date"], "2026-07-31");
    assert_eq!(july["expired_at"], "2026-08-01T03:59:59.999999999Z");
    assert_eq!(
        july["renewal"]["confirmed_expired_at"],
        "2026-02-01T04:59:59.999999999Z"
    );
}

#[tokio::test]
async fn yearly_leap_day_returns_after_non_leap_years_and_persistent_reopen() {
    let clock = ManualClock::at("2024-02-29T12:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "annual-leap-anchor").await;
    configure(
        &admin,
        &base,
        &id,
        "yearly",
        "2024-02-29",
        "America/New_York",
    )
    .await;
    clock.set("2024-03-01T05:00:00Z");
    tick(&state).await;
    let non_leap = detail(&admin, &base, &id).await;
    assert_eq!(non_leap["renewal"]["expiry_date"], "2025-02-28");
    assert_eq!(non_leap["expired_at"], "2025-03-01T04:59:59.999999999Z");
    state.db.clone().close().await.unwrap();
    clock.set("2028-02-28T12:00:00Z");
    let (base, _restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let leap = detail(&admin, &base, &id).await;
    assert_eq!(leap["renewal"]["expiry_date"], "2028-02-29");
    assert_eq!(leap["expired_at"], "2028-03-01T04:59:59.999999999Z");
    assert_eq!(
        leap["renewal"]["confirmed_expired_at"],
        "2024-03-01T04:59:59.999999999Z"
    );
}

#[tokio::test]
async fn onboarding_validates_before_mutation_and_retries_keep_original_identity_across_time() {
    let clock = ManualClock::at("2026-03-01T05:00:00Z");
    let (base, _tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    for invalid in [
        json!({"enabled":true,"expiry_date":"2026-01-31","billing_timezone":"UTC"}),
        json!({"enabled":true,"billing_timezone":"UTC"}),
        json!({"enabled":true,"expiry_date":"2026-01-31","billing_timezone":null}),
    ] {
        let response = admin.post(format!("{base}/api/servers"))
            .json(&json!({"onboarding_request_id":uuid::Uuid::new_v4().to_string(),"name":"invalid-create","renewal":invalid}))
            .send().await.unwrap();
        assert_eq!(response.status(), 422);
    }
    let body: Value = admin
        .get(format!("{base}/api/servers"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        body["data"].as_array().unwrap().is_empty(),
        "invalid enabled onboarding creates no server"
    );
    let request = json!({"onboarding_request_id":uuid::Uuid::new_v4().to_string(),"name":"valid-projected-create","billing_cycle":"monthly","renewal":{"enabled":true,"expiry_date":"2026-01-31","billing_timezone":"America/New_York"}});
    let response = admin
        .post(format!("{base}/api/servers"))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    let id = body["data"]["server_id"].as_str().unwrap();
    assert_eq!(
        detail(&admin, &base, id).await["renewal"]["expiry_date"],
        "2026-03-31"
    );
    clock.set("2026-06-01T04:00:00Z");
    tick(&state).await;
    let replay = admin
        .post(format!("{base}/api/servers"))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(
        replay.status(),
        200,
        "runtime projection cannot change canonical onboarding identity"
    );
    let replay: Value = replay.json().await.unwrap();
    assert_eq!(replay["data"]["server_id"], id);
    let current = detail(&admin, &base, id).await;
    assert_eq!(current["renewal"]["expiry_date"], "2026-06-30");
    assert_eq!(
        current["renewal"]["confirmed_expired_at"],
        "2026-02-01T04:59:59.999999999Z"
    );
}

#[tokio::test]
async fn concurrent_http_metadata_edits_and_scheduler_ticks_keep_one_stable_schedule() {
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, _tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "before-rollover").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    let original = detail(&admin, &base, &id).await;
    clock.set("2026-02-01T05:00:00Z");
    let mut work = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let state = state.clone();
        work.spawn(async move {
            tick(&state).await;
        });
    }
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"name":"after-rollover","price":99.5}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    while let Some(result) = work.join_next().await {
        result.unwrap();
    }
    let current = detail(&admin, &base, &id).await;
    assert_eq!(current["name"], "after-rollover");
    assert_eq!(current["price"], 99.5);
    assert_eq!(current["renewal"]["expiry_date"], "2026-02-28");
    assert_eq!(
        current["renewal"]["confirmed_expired_at"],
        original["renewal"]["confirmed_expired_at"]
    );
    assert_ne!(
        current["renewal"]["occurrence_id"],
        original["renewal"]["occurrence_id"]
    );
    for _ in 0..3 {
        tick(&state).await;
    }
    assert_eq!(
        detail(&admin, &base, &id).await["renewal"],
        current["renewal"]
    );
    clock.set("2026-03-01T05:00:00Z");
    tick(&state).await;
    assert_eq!(
        detail(&admin, &base, &id).await["renewal"]["expiry_date"],
        "2026-03-31"
    );
}

#[tokio::test]
async fn ordinary_expiration_window_tracks_current_projection_without_historical_backlog() {
    let clock = ManualClock::at("2026-02-01T05:00:00Z");
    let (base, _tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "current-reminder-target").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    let response = admin.post(format!("{base}/api/alert-rules"))
        .json(&json!({"name":"seven-day-renewal","trigger_mode":"once","cover_type":"include","server_ids":[id],"rules":[{"rule_type":"expiration","duration":7}]}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let rule: Value = response.json().await.unwrap();
    let rule_id = rule["data"]["id"].as_str().unwrap();
    tick(&state).await;
    let events: Value = admin
        .get(format!("{base}/api/alert-rules/{rule_id}/states"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        events["data"].as_array().unwrap().is_empty(),
        "historical January confirmation must not remain the reminder target"
    );
    clock.set("2026-02-23T12:00:00Z");
    tick(&state).await;
    let events: Value = admin
        .get(format!("{base}/api/alert-rules/{rule_id}/states"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(events["data"].as_array().unwrap().len(), 1);
    assert_eq!(events["data"][0]["server_id"], id);
    assert_eq!(events["data"][0]["resolved"], false);
}

#[tokio::test]
async fn cost_expiry_advisories_share_renewal_time_without_changing_independent_cost_periods() {
    let clock = ManualClock::at("2026-02-01T05:00:00Z");
    let (base, _tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "forecast-cost-advisory").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"price":31.0,"currency":"USD","billing_start_day":7}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let cost: Value = admin
        .get(format!("{base}/api/servers/{id}/cost-insights"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        !cost["data"]["advisories"]
            .as_array()
            .unwrap()
            .contains(&json!("expired_billing")),
        "current February projection has not expired on February 1"
    );
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":false}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    clock.set("2026-03-01T05:00:00Z");
    tick(&state).await;
    let frozen_cost: Value = admin
        .get(format!("{base}/api/servers/{id}/cost-insights"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        frozen_cost["data"]["advisories"]
            .as_array()
            .unwrap()
            .contains(&json!("expired_billing"))
    );
    assert_eq!(cost["data"]["configured"], true);
    for field in [
        "cycle_start",
        "cycle_end",
        "cycle_days",
        "days_elapsed",
        "days_remaining",
        "cost_per_second",
        "cost_per_hour",
        "cost_per_day",
        "cost_per_month_equivalent",
        "cycle_cost_elapsed",
        "cycle_cost_remaining",
        "cycle_burn_percent",
    ] {
        assert!(
            !cost["data"][field].is_null(),
            "configured cost field {field} remains available"
        );
        assert_eq!(
            cost["data"][field], frozen_cost["data"][field],
            "renewal time must not replace cost estimation calendar: {field}"
        );
    }
}

#[tokio::test]
async fn automatic_projection_skips_nonexistent_anchored_days_and_recovers_after_restart() {
    let clock = ManualClock::at("2011-12-31T00:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "apia-skipped-occurrence").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","renewal":{"enabled":true,"expiry_date":"2011-11-30","billing_timezone":"Pacific/Apia"}}))
        .send().await.unwrap();
    assert_eq!(
        response.status(),
        200,
        "a nonexistent projected December 30 cannot block catch-up"
    );
    let january = detail(&admin, &base, &id).await;
    assert_eq!(january["renewal"]["expiry_date"], "2012-01-30");
    assert_eq!(january["expired_at"], "2012-01-30T09:59:59.999999999Z");
    assert_eq!(
        january["renewal"]["confirmed_expired_at"],
        "2011-12-01T09:59:59.999999999Z"
    );
    // Persist an enabled pre-transition schedule, then genuinely close/reopen SQLite.
    clock.set("2011-11-30T12:00:00Z");
    let before_jump = create_server(&admin, &base, "apia-startup-occurrence").await;
    configure(
        &admin,
        &base,
        &before_jump,
        "monthly",
        "2011-11-30",
        "Pacific/Apia",
    )
    .await;
    state.db.clone().close().await.unwrap();
    clock.set("2011-12-31T00:00:00Z");
    let (base, restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let restored = detail(&admin, &base, &before_jump).await;
    assert_eq!(restored["renewal"]["expiry_date"], "2012-01-30");
    assert_eq!(restored["expired_at"], "2012-01-30T09:59:59.999999999Z");
    tick(&restarted).await;
    assert_eq!(
        detail(&admin, &base, &before_jump).await["renewal"],
        restored["renewal"]
    );
}

#[tokio::test]
async fn invalid_persisted_schedule_does_not_block_other_forecasts_or_hide_database_failure() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let invalid = create_server(&admin, &base, "damaged-schedule").await;
    let healthy = create_server(&admin, &base, "healthy-schedule").await;
    for id in [&invalid, &healthy] {
        configure(
            &admin,
            &base,
            id,
            "monthly",
            "2026-01-31",
            "America/New_York",
        )
        .await;
    }
    // Seed a durable invalid calendar record; all evaluation and persistence remain real.
    let mut damaged = detail(&admin, &base, &invalid).await["renewal"].clone();
    damaged["billing_timezone"] = json!("Not/A_Real_Zone");
    damaged["anchor_day"] = json!(31);
    state
        .db
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE servers SET renewal_state = ? WHERE id = ?",
            [damaged.to_string().into(), invalid.clone().into()],
        ))
        .await
        .unwrap();
    state.db.clone().close().await.unwrap();
    clock.set("2026-02-01T05:00:00Z");
    let (base, restarted) = reopen(&tmp, clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    assert_eq!(
        detail(&admin, &base, &healthy).await["renewal"]["expiry_date"],
        "2026-02-28"
    );
    assert_eq!(
        detail(&admin, &base, &invalid).await["name"],
        "damaged-schedule"
    );
    clock.set("2026-03-01T05:00:00Z");
    tick(&restarted).await;
    assert_eq!(
        detail(&admin, &base, &healthy).await["renewal"]["expiry_date"],
        "2026-03-31"
    );
    restarted.db.clone().close().await.unwrap();
    assert!(
        matches!(
            serverbee_server::task::alert_evaluator::evaluate_once(&restarted).await,
            Err(serverbee_server::error::AppError::Internal(_))
        ),
        "database failures must remain visible"
    );
}
