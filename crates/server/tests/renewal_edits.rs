//! Billing edit intent through production HTTP, calendar evaluation and reopened SQLite.
mod common;
use common::{create_server, http_client, login_admin};
use serde_json::{Value, json};

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
async fn interval_edit_after_clamped_month_retains_history_and_original_anchor_on_reopen() {
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "edited-quarterly-anchor").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    clock.set("2026-02-01T05:00:00Z");
    tick(&state).await;
    let february = detail(&admin, &base, &id).await;
    assert_eq!(february["renewal"]["expiry_date"], "2026-02-28");
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"quarterly"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let edited = detail(&admin, &base, &id).await;
    assert_eq!(edited["renewal"], february["renewal"]);
    assert_eq!(edited["expired_at"], february["expired_at"]);
    state.db.clone().close().await.unwrap();
    clock.set("2026-03-01T05:00:00Z");
    let (base, _restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let may = detail(&admin, &base, &id).await;
    assert_eq!(may["billing_cycle"], "quarterly");
    assert_eq!(may["renewal"]["expiry_date"], "2026-05-31");
    assert_eq!(may["expired_at"], "2026-06-01T03:59:59.999999999Z");
    assert_eq!(
        may["renewal"]["confirmed_expired_at"],
        february["renewal"]["confirmed_expired_at"]
    );
    assert_ne!(
        may["renewal"]["occurrence_id"],
        february["renewal"]["occurrence_id"]
    );
}

#[tokio::test]
async fn actual_date_correction_confirms_provider_date_and_replaces_clamped_anchor() {
    let clock = ManualClock::at("2026-01-31T12:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "corrected-provider-date").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    clock.set("2026-02-01T05:00:00Z");
    tick(&state).await;
    let february = detail(&admin, &base, &id).await;
    assert_eq!(february["renewal"]["expiry_date"], "2026-02-28");
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"expiry_date":"2026-02-15"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let corrected = detail(&admin, &base, &id).await;
    assert_eq!(corrected["renewal"]["expiry_date"], "2026-02-15");
    assert_eq!(corrected["expired_at"], "2026-02-16T04:59:59.999999999Z");
    assert_eq!(
        corrected["renewal"]["confirmed_expired_at"],
        "2026-02-16T04:59:59.999999999Z"
    );
    assert_eq!(corrected["renewal"]["deadline_origin"], "projected");
    assert_eq!(corrected["renewal"]["enabled"], true);
    assert_ne!(
        corrected["renewal"]["occurrence_id"],
        february["renewal"]["occurrence_id"]
    );
    state.db.clone().close().await.unwrap();
    clock.set("2026-02-16T05:00:00Z");
    let (base, _restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let march = detail(&admin, &base, &id).await;
    assert_eq!(march["renewal"]["expiry_date"], "2026-03-15");
    assert_eq!(march["expired_at"], "2026-03-16T03:59:59.999999999Z");
    assert_eq!(
        march["renewal"]["confirmed_expired_at"],
        corrected["renewal"]["confirmed_expired_at"]
    );
}

#[tokio::test]
async fn past_date_correction_confirms_recorded_date_and_catches_up_with_new_anchor() {
    let clock = ManualClock::at("2026-02-01T05:00:00Z");
    let (base, _tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "past-provider-correction").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    let before = detail(&admin, &base, &id).await;
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"expiry_date":"2026-01-20"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let corrected = detail(&admin, &base, &id).await;
    assert_eq!(
        corrected["renewal"]["confirmed_expired_at"],
        "2026-01-21T04:59:59.999999999Z"
    );
    assert_eq!(corrected["renewal"]["expiry_date"], "2026-02-20");
    assert_eq!(corrected["expired_at"], "2026-02-21T04:59:59.999999999Z");
    assert_ne!(
        corrected["renewal"]["occurrence_id"],
        before["renewal"]["occurrence_id"]
    );
    clock.set("2026-02-21T05:00:00Z");
    tick(&state).await;
    let march = detail(&admin, &base, &id).await;
    assert_eq!(march["renewal"]["expiry_date"], "2026-03-20");
    assert_eq!(march["expired_at"], "2026-03-21T03:59:59.999999999Z");
    assert_eq!(
        march["renewal"]["confirmed_expired_at"],
        corrected["renewal"]["confirmed_expired_at"]
    );
}

#[tokio::test]
async fn timezone_only_edit_preserves_local_projection_history_occurrence_and_anchor_through_dst() {
    let clock = ManualClock::at("2026-02-01T05:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let clamped = create_server(&admin, &base, "timezone-clamped-date").await;
    configure(
        &admin,
        &base,
        &clamped,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    let dst = create_server(&admin, &base, "timezone-dst-date").await;
    configure(
        &admin,
        &base,
        &dst,
        "monthly",
        "2026-02-08",
        "America/New_York",
    )
    .await;
    clock.set("2026-02-09T05:00:00Z");
    tick(&state).await;
    let mut histories = Vec::new();
    for (id, zone, date, boundary) in [
        (
            &clamped,
            "Asia/Tokyo",
            "2026-02-28",
            "2026-02-28T14:59:59.999999999Z",
        ),
        (
            &dst,
            "America/Los_Angeles",
            "2026-03-08",
            "2026-03-09T06:59:59.999999999Z",
        ),
    ] {
        let before = detail(&admin, &base, id).await;
        let response = admin
            .put(format!("{base}/api/servers/{id}"))
            .json(&json!({"renewal":{"billing_timezone":zone}}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let after = detail(&admin, &base, id).await;
        assert_eq!(after["renewal"]["expiry_date"], date);
        assert_eq!(after["expired_at"], boundary);
        assert_eq!(
            after["renewal"]["confirmed_expired_at"],
            before["renewal"]["confirmed_expired_at"]
        );
        assert_eq!(
            after["renewal"]["occurrence_id"],
            before["renewal"]["occurrence_id"]
        );
        assert_eq!(after["renewal"]["deadline_origin"], "projected");
        histories.push(before["renewal"]["confirmed_expired_at"].clone());
    }
    state.db.clone().close().await.unwrap();
    clock.set("2026-03-09T07:00:00Z");
    let (base, _restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let march = detail(&admin, &base, &clamped).await;
    assert_eq!(march["renewal"]["expiry_date"], "2026-03-31");
    assert_eq!(march["expired_at"], "2026-03-31T14:59:59.999999999Z");
    assert_eq!(march["renewal"]["confirmed_expired_at"], histories[0]);
    let april = detail(&admin, &base, &dst).await;
    assert_eq!(april["renewal"]["expiry_date"], "2026-04-08");
    assert_eq!(april["expired_at"], "2026-04-09T06:59:59.999999999Z");
    assert_eq!(april["renewal"]["confirmed_expired_at"], histories[1]);
}

#[tokio::test]
async fn actual_full_form_shapes_and_legacy_resubmissions_preserve_projected_anchor_on_reopen() {
    let clock = ManualClock::at("2026-02-01T05:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "full-form-clamped-projection").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    let before = detail(&admin, &base, &id).await;
    // Actual Web and iOS editors submit these ordinary fields and omit unchanged
    // renewal settings. Retain old clients' instant and UTC-prefix saves too.
    let web = json!({"name":"web-save","weight":100,"hidden":false,"group_id":null,
        "remark":"","public_remark":"","price":12.5,"billing_cycle":"monthly",
        "currency":"USD","traffic_limit":1073741824_i64,"traffic_limit_type":"sum","billing_start_day":7});
    let mut ios = web.clone();
    ios["name"] = json!("ios-save");
    ios["price"] = json!(15.0);
    let mut calendar_full_form = web.clone();
    calendar_full_form["renewal"] =
        json!({"enabled":true,"expiry_date":"2026-02-28","billing_timezone":"America/New_York"});
    let mut legacy_ios = ios.clone();
    legacy_ios["expired_at"] = before["expired_at"].clone();
    let mut legacy_web = web.clone();
    legacy_web["expired_at"] = json!("2026-03-01T00:00:00.000Z");
    for payload in [web, ios, calendar_full_form, legacy_ios, legacy_web] {
        let response = admin
            .put(format!("{base}/api/servers/{id}"))
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let saved = detail(&admin, &base, &id).await;
        assert_eq!(saved["renewal"], before["renewal"]);
        assert_eq!(saved["expired_at"], before["expired_at"]);
        for field in [
            "name",
            "price",
            "currency",
            "traffic_limit",
            "traffic_limit_type",
            "billing_start_day",
        ] {
            assert_eq!(
                saved[field], payload[field],
                "ordinary field {field} still saves"
            );
        }
    }
    state.db.clone().close().await.unwrap();
    clock.set("2026-03-01T05:00:00Z");
    let (base, _restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let march = detail(&admin, &base, &id).await;
    assert_eq!(march["renewal"]["expiry_date"], "2026-03-31");
    assert_eq!(march["expired_at"], "2026-04-01T03:59:59.999999999Z");
    assert_eq!(
        march["renewal"]["confirmed_expired_at"],
        before["renewal"]["confirmed_expired_at"]
    );
    assert_eq!(march["price"], 12.5);
    assert_eq!(march["currency"], "USD");
    assert_eq!(march["traffic_limit"], 1073741824_i64);
    assert_eq!(march["billing_start_day"], 7);
}

#[tokio::test]
async fn interval_edit_of_non_leap_projection_keeps_leap_day_anchor_and_confirmation_on_reopen() {
    let clock = ManualClock::at("2024-02-29T12:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "edited-leap-anchor").await;
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
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"quarterly","renewal":{"enabled":true,"expiry_date":"2025-02-28","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        detail(&admin, &base, &id).await["renewal"],
        non_leap["renewal"]
    );
    state.db.clone().close().await.unwrap();
    clock.set("2025-03-01T05:00:00Z");
    let (base, _restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let may = detail(&admin, &base, &id).await;
    assert_eq!(may["renewal"]["expiry_date"], "2025-05-29");
    assert_eq!(may["expired_at"], "2025-05-30T03:59:59.999999999Z");
    assert_eq!(
        may["renewal"]["confirmed_expired_at"],
        "2024-03-01T04:59:59.999999999Z"
    );
}

#[tokio::test]
async fn frozen_timezone_edit_keeps_provenance_until_actual_date_correction_confirms_it() {
    let clock = ManualClock::at("2026-02-01T05:00:00Z");
    let (base, tmp, state) = controlled_server(clock.clone()).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "manual-frozen-correction").await;
    configure(
        &admin,
        &base,
        &id,
        "monthly",
        "2026-01-31",
        "America/New_York",
    )
    .await;
    let projected = detail(&admin, &base, &id).await;
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":false,"billing_timezone":"Asia/Tokyo"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let frozen = detail(&admin, &base, &id).await;
    assert_eq!(frozen["renewal"]["expiry_date"], "2026-02-28");
    assert_eq!(frozen["expired_at"], "2026-02-28T14:59:59.999999999Z");
    assert_eq!(frozen["renewal"]["deadline_origin"], "frozen");
    assert_eq!(
        frozen["renewal"]["occurrence_id"],
        projected["renewal"]["occurrence_id"]
    );
    assert_eq!(
        frozen["renewal"]["confirmed_expired_at"],
        projected["renewal"]["confirmed_expired_at"]
    );
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"expiry_date":"2026-02-15"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let corrected = detail(&admin, &base, &id).await;
    assert_eq!(corrected["renewal"]["deadline_origin"], "confirmed");
    assert_eq!(corrected["renewal"]["enabled"], false);
    assert_eq!(corrected["expired_at"], "2026-02-15T14:59:59.999999999Z");
    assert_eq!(
        corrected["renewal"]["confirmed_expired_at"],
        corrected["expired_at"]
    );
    assert_ne!(
        corrected["renewal"]["occurrence_id"],
        frozen["renewal"]["occurrence_id"]
    );
    state.db.clone().close().await.unwrap();
    clock.set("2026-03-01T00:00:00Z");
    let (base, restarted) = reopen(&tmp, clock).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    tick(&restarted).await;
    assert_eq!(
        detail(&admin, &base, &id).await["renewal"],
        corrected["renewal"]
    );
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"enabled":true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let resumed = detail(&admin, &base, &id).await;
    assert_eq!(resumed["renewal"]["expiry_date"], "2026-03-15");
    assert_eq!(
        resumed["renewal"]["confirmed_expired_at"],
        corrected["expired_at"]
    );
}
