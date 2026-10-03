//! Extend the existing real Relay handler/admission/APNs fixture into all categories.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stitched_categories_use_actual_relay_and_preserve_external_legacy_delivery() {
    let relay = DeliveryRelayFixture::start().await;
    let (_, initial, _tmp) = setup_http().await;
    let mut config = initial.config.clone();
    config.push_relay.url = relay.ready["url"].as_str().unwrap().to_owned();
    let state = AppState::new(initial.db.clone(), config).await.unwrap();
    let base = serve_outbox_http(state.clone()).await;
    let client = reqwest::Client::new();
    let owner = login_http(&client, &base, "admin", "stitched-modern").await;
    let legacy = login_http(&client, &base, "member", "stitched-legacy").await;
    let access = owner["access_token"].as_str().unwrap();
    for login in [&owner, &legacy] {
        assert_eq!(
            client
                .post(format!("{base}/api/mobile/push/register"))
                .bearer_auth(login["access_token"].as_str().unwrap())
                .json(&json!({"device_token":"c".repeat(64)}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    assert_eq!(
        preferences_http(&client, &base, access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    let registration = content_registration(&relay.ready, 1);
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/encrypted-register"))
            .bearer_auth(access)
            .json(&registration)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    all_categories(&client, &base, access).await;
    let legacy_rows = ApnsService::legacy_recipients(&state.db).await.unwrap();
    assert_eq!(legacy_rows.len(), 1);
    assert_eq!(legacy_rows[0].installation_id, "stitched-legacy");
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/register"))
            .bearer_auth(access)
            .json(&json!({"device_token":"d".repeat(64)}))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    let (alert_server, alert_rule) = events(&client, &base, &state, access, "203.0.113.17").await;
    let webhook = attach_alert_webhook(&client, &base, access, &alert_rule).await;
    set_alert_expiration(&client, &base, access, &alert_server, false).await;
    evaluate_alerts(&state).await;
    set_alert_expiration(&client, &base, access, &alert_server, true).await;
    evaluate_alerts(&state).await;
    evaluate_alerts(&state).await;
    targeted_test(&client, &base, access).await;
    let queued = jobs(&state).await;
    assert_eq!(queued.len(), 7);
    let mut contents = Vec::new();
    for job in &queued {
        assert_eq!(job.installation_id, "stitched-modern");
        contents.push((job.event_id.clone(), plaintext(&state, job).await));
    }
    let final_rows = drain(&state, 7).await;
    assert!(final_rows.iter().all(|j| j.outcome == "accepted"));
    assert_original_receipts(&queued, &final_rows);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while webhook.lock().await.len() != 2 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("one external recovery and rearm trigger");
    let external = webhook.lock().await.clone();
    assert!(external.iter().any(|body| body.contains("resolved")));
    assert!(external.iter().any(|body| body.contains("triggered")));
    assert_eq!(
        ApnsService::legacy_recipients(&state.db).await.unwrap(),
        legacy_rows
    );
    let captured = std::fs::read_to_string(relay.path("provider-requests.jsonl")).unwrap();
    let mut entries = Vec::new();
    for line in captured.lines() {
        let request: Value = serde_json::from_str(line).unwrap();
        assert_eq!(request["token"], relay.ready["device_token"]);
        assert_eq!(request["environment"], "sandbox");
        assert_eq!(request["headers"]["apns-topic"], "com.serverbee.mobile");
        assert_eq!(request["headers"]["apns-push-type"], "alert");
        assert_eq!(request["headers"]["apns-priority"], "10");
        let event_id = request["headers"]["apns-id"].as_str().unwrap();
        let (identity, content) = contents.iter().find(|(id, _)| id == event_id).unwrap();
        let payload: Value = serde_json::from_str(request["payload"].as_str().unwrap()).unwrap();
        let job = queued.iter().find(|j| &j.event_id == identity).unwrap();
        assert_eq!(
            payload["serverbee_envelope"],
            serde_json::from_str::<Value>(job.envelope.as_deref().unwrap()).unwrap()
        );
        assert_eq!(
            request["headers"]["apns-expiration"],
            job.expires_at.to_string()
        );
        for forbidden in [
            "content_key",
            "private-command-output",
            "https://serverbee.test",
            "203.0.113.17",
            "stitched-modern",
        ] {
            assert!(!request["payload"].as_str().unwrap().contains(forbidden));
        }
        entries.push(json!({"payload":payload, "content":content, "registration":registration,
            "user_id":owner["user"]["id"], "installation_id":"stitched-modern", "event_id":identity}));
    }
    assert_eq!(
        entries.len(),
        7,
        "exactly one real APNs transport request per event"
    );
    for kind in ["test", "alert", "security", "task_failure", "task_success"] {
        assert!(entries.iter().any(|e| e["content"]["kind"] == kind));
    }
    if let Some(directory) = std::env::var_os("SERVERBEE_PUSH_TRACE_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join("trace-categories.json"),
            serde_json::to_vec_pretty(&json!({"entries":entries})).unwrap(),
        )
        .unwrap();
    }
}
