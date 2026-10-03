//! Combined lifecycle checks through HTTP, scheduler completion, security rules,
//! alert evaluation, migrated SQLite and the real outbox worker.
use super::*;
use serverbee_server::service::apns::ApnsService;

#[path = "trace.rs"]
mod trace;

async fn all_categories(client: &reqwest::Client, base: &str, access: &str) {
    let confirmed = status_http(client, base, access).await;
    let mut prefs = confirmed["preferences"].clone();
    for key in ["alerts", "security", "task_failure", "task_success"] {
        prefs[key] = json!(true);
    }
    assert_eq!(
        preferences_http(
            client,
            base,
            access,
            confirmed["revision"].as_i64().unwrap(),
            prefs
        )
        .await
        .status(),
        200
    );
}

async fn register_categories(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    device: &str,
    byte: u8,
    admin: bool,
) {
    assert_eq!(
        preferences_http(client, base, access, 0, intent(false, true))
            .await
            .status(),
        200
    );
    let target = json!({"device_token":if device == "device-b" { "b".repeat(64) } else { "a".repeat(64) },
        "environment":"sandbox"});
    let mut request = content_registration(&target, 1);
    request["content_key"] = json!(STANDARD.encode([byte; 32]));
    request["content_key_id"] = json!(uuid::Uuid::new_v4().to_string());
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/encrypted-register"))
            .bearer_auth(access)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    if admin {
        all_categories(client, base, access).await;
    }
}

fn transport_content(delivery: &RecordedDelivery, snapshots: &[registration::Model]) -> Value {
    use ring::aead;
    let row = snapshots
        .iter()
        .find(|row| {
            delivery.body["device_token"] == row.device_token.as_deref().unwrap()
                && delivery.body["environment"] == row.environment.as_deref().unwrap()
                && delivery.body["envelope"]["key_id"] == row.content_key_id.as_deref().unwrap()
        })
        .unwrap();
    let envelope = &delivery.body["envelope"];
    assert_eq!(envelope["key_id"], row.content_key_id.as_deref().unwrap());
    let secret = STANDARD
        .decode(row.content_key.as_deref().unwrap())
        .unwrap();
    let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &secret).unwrap());
    let nonce: [u8; 12] = STANDARD
        .decode(envelope["nonce"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let aad = format!(
        "ServerBee.Push.v1|{}|{}",
        envelope["key_id"].as_str().unwrap(),
        envelope["identity"].as_str().unwrap()
    );
    let mut bytes = STANDARD
        .decode(envelope["ciphertext"].as_str().unwrap())
        .unwrap();
    let content: Value = serde_json::from_slice(
        key.open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad.as_bytes()),
            &mut bytes,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(content["installation_id"], row.installation_id);
    assert_eq!(content["user_id"], row.user_id);
    assert_eq!(delivery.body["event_id"], content["event_id"]);
    assert_eq!(delivery.body["expires_at"], content["expires_at"]);
    assert_eq!(
        content["expires_at"].as_i64().unwrap() - content["created_at"].as_i64().unwrap(),
        1800
    );
    for value in [
        "private-command-output",
        "content_key",
        "https://serverbee.test",
        "203.0.113",
    ] {
        assert!(!delivery.body.to_string().contains(value));
    }
    content
}

/// Produce both final task outcomes, an alert and a rule-admitted security
/// event. No policy/persistence helper is replaced; execution is an Agent peer.
async fn events(
    client: &reqwest::Client,
    base: &str,
    state: &AppState,
    access: &str,
    source: &str,
) -> (String, String) {
    let (target, mut sink, mut reader) = agent(client, base, access).await;
    for code in [1, 0] {
        let id = task(
            client,
            base,
            access,
            std::slice::from_ref(&target),
            0,
            "0 0 0 * * *",
        )
        .await;
        run(client, base, access, &id).await;
        let execution = exec(&mut reader).await;
        reply(&mut sink, &execution, code, "private-command-output").await;
        completed(state, &id).await;
        disable(client, base, access, &id).await;
    }
    let (alert_server, alert_rule) = alert_http_fixture(client, base, access, "once").await;
    evaluate_alerts(state).await;
    let response = client
        .post(format!("{base}/api/alert-rules"))
        .bearer_auth(access)
        .json(
            &json!({"name":"lifecycle-security", "enabled":true, "cover_type":"all",
            "rules":[{"rule_type":"port_scan_detected", "security":{"min_distinct_ports":5}}]}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let payload = serde_json::from_value(json!({"event_type":"port_scan", "severity":"high", "source_ip":source,
        "source_port":22, "username":null, "started_at":Utc::now().timestamp()-30, "ended_at":Utc::now().timestamp(),
        "first_seen":false, "detector_source":"journal", "evidence":{"kind":"port_scan", "distinct_ports":20,
            "sample_ports":[22,80,443], "total_attempts":40, "window_seconds":30, "threshold":5, "blocked_count":0}})).unwrap();
    state
        .security_service
        .record_event(&target, payload)
        .await
        .unwrap();
    sink.close().await.unwrap();
    (alert_server, alert_rule)
}

async fn targeted_test(client: &reqwest::Client, base: &str, access: &str) {
    let confirmed = status_http(client, base, access).await;
    let response = enqueue_test(
        client,
        base,
        access,
        confirmed["revision"].as_i64().unwrap(),
        &uuid::Uuid::new_v4().to_string(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let data: Value = response.json().await.unwrap();
    assert_eq!(data["data"]["outcome"], "pending");
    assert_eq!(data["data"]["presentation"], "unobserved");
}

async fn drain(state: &Arc<AppState>, expected: usize) -> Vec<outbox::Model> {
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let rows = jobs(state).await;
            if rows.len() == expected
                && rows
                    .iter()
                    .all(|r| !matches!(r.outcome.as_str(), "pending" | "retryable" | "inflight"))
            {
                return rows;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    worker.abort();
    let _ = worker.await;
    result.expect("combined categories reached terminal receipts")
}

fn assert_original_receipts(before: &[outbox::Model], after: &[outbox::Model]) {
    for row in after {
        let original = before
            .iter()
            .find(|j| j.event_id == row.event_id && j.installation_id == row.installation_id)
            .unwrap();
        assert_eq!(row.created_at, original.created_at);
        assert_eq!(row.expires_at, original.expires_at);
        assert_eq!(row.expires_at - row.created_at, 1800);
        assert!(row.envelope.is_none(), "terminal ciphertext is erased");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_and_legacy_migration_preserve_all_categories_per_installation() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let a = login_http(&client, &base, "admin", "lifecycle-a").await;
    let b = login_http(&client, &base, "admin", "lifecycle-b").await;
    let legacy = login_http(&client, &base, "member", "legacy-member").await;
    let member = login_http(&client, &base, "member", "modern-member").await;
    let a_access = a["access_token"].as_str().unwrap();
    let b_access = b["access_token"].as_str().unwrap();
    let member_access = member["access_token"].as_str().unwrap();
    assert_eq!(
        client
            .post(format!("{base}/api/users"))
            .bearer_auth(a_access)
            .json(&json!({"username":"other-admin", "password":"testpass", "role":"admin"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let other = login_http(&client, &base, "other-admin", "other-admin").await;
    let other_access = other["access_token"].as_str().unwrap();
    for login in [&a, &legacy] {
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
    for (access, device, byte, admin) in [
        (a_access, "device-a", 1, true),
        (b_access, "device-b", 2, true),
        (member_access, "member-device", 3, false),
        (other_access, "other-device", 4, true),
    ] {
        register_categories(&client, &base, access, device, byte, admin).await;
    }
    assert_eq!(preferences_http(&client, &base, member_access, 2,
        json!({"enabled":true,"alerts":true,"security":true,"task_failure":true,"task_success":true})).await.status(), 403);
    assert_eq!(
        ApnsService::legacy_recipients(&state.db)
            .await
            .unwrap()
            .len(),
        1
    );
    events(&client, &base, &state, a_access, "203.0.113.7").await;
    for access in [a_access, b_access, member_access, other_access] {
        targeted_test(&client, &base, access).await;
    }
    let before_refresh_jobs = jobs(&state).await;
    assert_eq!(before_refresh_jobs.len(), 15);
    let snapshots = registration::Entity::find().all(&state.db).await.unwrap();
    let refresh = client
        .post(format!("{base}/api/mobile/auth/refresh"))
        .json(&json!({"installation_id":"lifecycle-a", "refresh_token":a["refresh_token"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), 200);
    let rotated: Value = refresh.json().await.unwrap();
    let rotated_access = rotated["data"]["access_token"].as_str().unwrap();
    assert_eq!(
        client
            .get(format!("{base}/api/mobile/push/settings"))
            .bearer_auth(a_access)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let after_refresh = registration::Entity::find_by_id("lifecycle-a")
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        *snapshots
            .iter()
            .find(|r| r.installation_id == "lifecycle-a")
            .unwrap(),
        after_refresh
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/push/register"))
            .bearer_auth(rotated_access)
            .json(&json!({"device_token":"d".repeat(64)}))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    // Every real event category also enters after refresh under the same login.
    events(&client, &base, &state, rotated_access, "203.0.113.8").await;
    let queued = jobs(&state).await;
    assert_eq!(queued.len(), 26);
    for installation in ["lifecycle-a", "lifecycle-b"] {
        for category in ["alert", "security", "task_failure", "task_success", "test"] {
            assert_eq!(
                queued
                    .iter()
                    .filter(|j| j.installation_id == installation && j.category == category)
                    .count(),
                if category == "test" { 1 } else { 2 }
            );
        }
    }
    assert!(
        queued
            .iter()
            .filter(|j| j.installation_id == "modern-member")
            .all(|j| matches!(j.category.as_str(), "alert" | "test"))
    );
    assert!(
        queued
            .iter()
            .filter(|j| j.installation_id == "other-admin")
            .all(|j| matches!(j.category.as_str(), "alert" | "security" | "test"))
    );
    assert_eq!(
        preferences_http(&client, &base, rotated_access, 0, intent(false, false))
            .await
            .status(),
        409
    );
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let final_rows = drain(&state, 26).await;
    assert!(final_rows.iter().all(|r| r.outcome == "accepted"));
    assert_original_receipts(&queued, &final_rows);
    let deliveries = relay.deliveries().await;
    assert_eq!(deliveries.len(), 26);
    for delivery in deliveries {
        let content = transport_content(&delivery, &snapshots);
        let row = queued
            .iter()
            .find(|j| {
                content["event_id"] == j.event_id && content["installation_id"] == j.installation_id
            })
            .unwrap();
        assert_eq!(content["kind"], row.category);
        assert_eq!(content["created_at"], row.created_at);
        assert_eq!(content["expires_at"], row.expires_at);
    }
    assert_eq!(
        ApnsService::legacy_recipients(&state.db)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logout_account_replacement_cannot_inherit_any_queued_category() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let a = login_http(&client, &base, "admin", "lifecycle-a").await;
    let b = login_http(&client, &base, "admin", "lifecycle-b").await;
    let a_access = a["access_token"].as_str().unwrap();
    let b_access = b["access_token"].as_str().unwrap();
    for (access, device, byte) in [(a_access, "device-a", 1), (b_access, "device-b", 2)] {
        register_categories(&client, &base, access, device, byte, true).await;
    }
    events(&client, &base, &state, a_access, "203.0.113.7").await;
    targeted_test(&client, &base, a_access).await;
    targeted_test(&client, &base, b_access).await;
    let queued = jobs(&state).await;
    let snapshots = registration::Entity::find().all(&state.db).await.unwrap();
    assert_eq!(queued.len(), 10);
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/auth/logout"))
            .bearer_auth(a_access)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let replacement = login_http(&client, &base, "member", "lifecycle-a").await;
    let replacement_access = replacement["access_token"].as_str().unwrap();
    queued_register(&client, &base, replacement_access, "replacement-device").await;
    assert_eq!(preferences_http(&client, &base, replacement_access, 2,
        json!({"enabled":true,"alerts":true,"security":true,"task_failure":true,"task_success":true})).await.status(), 403);
    // The replacement cannot inspect the former login's targeted test receipt.
    let old_test = queued
        .iter()
        .find(|j| j.installation_id == "lifecycle-a" && j.category == "test")
        .unwrap();
    assert_eq!(
        client
            .get(format!("{base}/api/mobile/push/test/{}", old_test.event_id))
            .bearer_auth(replacement_access)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let final_rows = drain(&state, 10).await;
    for row in &final_rows {
        assert_eq!(
            row.outcome,
            if row.installation_id == "lifecycle-a" {
                "permanent"
            } else {
                "accepted"
            }
        );
    }
    assert_original_receipts(&queued, &final_rows);
    let deliveries = relay.deliveries().await;
    assert_eq!(deliveries.len(), 5);
    for delivery in deliveries {
        let content = transport_content(&delivery, &snapshots);
        assert_eq!(content["installation_id"], "lifecycle-b");
        assert_eq!(content["user_id"], a["user"]["id"]);
    }
    assert_eq!(
        status_http(&client, &base, replacement_access).await["registered"],
        true
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_downgrade_cancels_old_categories_and_allows_new_member_alert_and_test() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let owner = login_http(&client, &base, "admin", "lifecycle-a").await;
    let access = owner["access_token"].as_str().unwrap();
    queued_register(&client, &base, access, "device-a").await;
    all_categories(&client, &base, access).await;
    events(&client, &base, &state, access, "203.0.113.7").await;
    targeted_test(&client, &base, access).await;
    let queued = jobs(&state).await;
    assert_eq!(queued.len(), 5);
    AuthService::create_user(&state.db, "operator", "testpass", "admin")
        .await
        .unwrap();
    let operator = login_http(&client, &base, "operator", "operator").await;
    let operator_access = operator["access_token"].as_str().unwrap();
    assert_eq!(
        client
            .put(format!(
                "{base}/api/users/{}",
                owner["user"]["id"].as_str().unwrap()
            ))
            .bearer_auth(operator_access)
            .json(&json!({"role":"member"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let current = status_http(&client, &base, access).await;
    assert_eq!(current["security_allowed"], false);
    assert_eq!(current["tasks_allowed"], false);
    assert_eq!(
        preferences_http(
            &client,
            &base,
            access,
            current["revision"].as_i64().unwrap(),
            current["preferences"].clone()
        )
        .await
        .status(),
        403
    );
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let final_rows = drain(&state, 5).await;
    assert!(final_rows.iter().all(|r| r.outcome == "permanent"));
    assert_original_receipts(&queued, &final_rows);
    assert!(
        relay.requests().await.is_empty(),
        "queued admin snapshots cannot retain old authority"
    );
    assert_eq!(
        preferences_http(
            &client,
            &base,
            access,
            current["revision"].as_i64().unwrap(),
            intent(false, true)
        )
        .await
        .status(),
        200
    );
    targeted_test(&client, &base, access).await;
    alert_http_fixture(&client, &base, operator_access, "once").await;
    evaluate_alerts(&state).await;
    let final_rows = drain(&state, 7).await;
    assert_eq!(
        final_rows
            .iter()
            .filter(|r| r.outcome == "accepted")
            .count(),
        2
    );
    for request in relay.requests().await {
        let content = super::super::security_push::decrypted_delivery(&request);
        assert!(matches!(content["kind"].as_str(), Some("test" | "alert")));
        assert_eq!(content["user_id"], owner["user"]["id"]);
    }
}
