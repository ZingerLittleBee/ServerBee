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
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"name":"must-not-change","renewal":{"enabled":true}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 422, "enabled renewal must require date and interval");
    let body: Value = admin.get(format!("{base}/api/servers/{id}"))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(body["data"]["name"], "manual-host");
    assert_eq!(body["data"]["renewal"]["enabled"], false);
}

#[tokio::test]
async fn opt_in_projects_without_promoting_operator_history_to_payment_information() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client(); login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "expected-renewal").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"billing_cycle":"monthly","price":12.5,"currency":"USD","traffic_limit":1024,"billing_start_day":7,"renewal":{"enabled":true,"expiry_date":"2099-01-31","billing_timezone":"America/New_York"}}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["renewal"]["deadline_origin"], "projected");
    assert_eq!(body["data"]["renewal"]["confirmed_expired_at"], "2099-02-01T04:59:59.999999999Z");
    assert!(body["data"]["renewal"]["occurrence_id"].as_str().is_some_and(|id| !id.is_empty()));
}
