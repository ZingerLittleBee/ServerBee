//! Variable-length APNs tokens through authenticated setup and actual Relay transport.
use super::*;
use serde_json::json;
use serverbee_server::entity::mobile_push_registration as registration;

#[tokio::test]
async fn encrypted_setup_accepts_variable_length_tokens_and_rejects_malformed_changes() {
    let (base, state, _tmp) = setup_http().await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "admin", "variable-token-install").await;
    let access = login["access_token"].as_str().unwrap();
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(true, true))
            .await
            .status(),
        200
    );
    assert_eq!(
        setup_http_request(&client, &base, access, 1).await.status(),
        200
    );

    for token in [
        String::new(),
        "a".into(),
        "a".repeat(159),
        "a".repeat(1026),
        "A".repeat(160),
        "ag".into(),
        "ab/../cd".into(),
        "ab\r\n".into(),
        "ａｂ".into(),
    ] {
        let response = client
            .post(format!("{base}/api/mobile/push/encrypted-register"))
            .bearer_auth(access)
            .json(&content_registration(
                &json!({"device_token": token, "environment": "sandbox"}),
                2,
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
        let status = status_http(&client, &base, access).await;
        assert_eq!(status["revision"], 2);
        assert_eq!(status["registered"], true);
        let row = registration::Entity::find_by_id("variable-token-install")
            .one(&state.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.device_token, Some("a".repeat(64)));
    }

    let mut revision = 2;
    for length in [160, 64, 2, 1024] {
        let token = "ab".repeat(length / 2);
        let response = client
            .post(format!("{base}/api/mobile/push/encrypted-register"))
            .bearer_auth(access)
            .json(&content_registration(
                &json!({"device_token": token, "environment": "sandbox"}),
                revision,
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "valid token length {length}");
        revision += 1;
        let status = status_http(&client, &base, access).await;
        assert_eq!(status["revision"], revision);
        assert_eq!(status["registered"], true);
        assert_eq!(status["test_available"], true);
        let row = registration::Entity::find_by_id("variable-token-install")
            .one(&state.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.device_token.as_deref(), Some(token.as_str()));
    }
}

#[tokio::test]
async fn encrypted_simulator_token_reaches_real_relay_apns_transport_unchanged() {
    let relay = DeliveryRelayFixture::start().await;
    let (_, initial, _tmp) = setup_http().await;
    let mut config = initial.config.clone();
    config.push_relay.url = relay.ready["url"].as_str().unwrap().to_owned();
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    let base = serve_setup_state(state.clone()).await;
    let client = reqwest::Client::new();
    let login = login_http(&client, &base, "admin", "simulator-token-install").await;
    let access = login["access_token"].as_str().unwrap();
    let token = "ab".repeat(80);
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(true, true))
            .await
            .status(),
        200
    );
    let response = client
        .post(format!("{base}/api/mobile/push/encrypted-register"))
        .bearer_auth(access)
        .json(&content_registration(
            &json!({"device_token": token, "environment": "sandbox"}),
            1,
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        status_http(&client, &base, access).await["test_available"],
        true
    );

    let sent = post_test(&client, &base, access, 2).await;
    assert_eq!(sent.status(), 200);
    let receipt = sent.json::<serde_json::Value>().await.unwrap()["data"].clone();
    assert_eq!(receipt["outcome"], "accepted");
    let provider: serde_json::Value =
        serde_json::from_slice(&std::fs::read(relay.path("provider-request.json")).unwrap())
            .unwrap();
    assert_eq!(provider["token"], token);
    assert_eq!(provider["environment"], "sandbox");
    assert_eq!(provider["headers"]["apns-id"], receipt["event_id"]);
    let payload: serde_json::Value =
        serde_json::from_str(provider["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["aps"]["mutable-content"], 1);
    assert!(payload["serverbee_envelope"].is_object());
}
