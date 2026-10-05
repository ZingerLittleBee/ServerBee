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
