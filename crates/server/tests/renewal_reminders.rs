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
