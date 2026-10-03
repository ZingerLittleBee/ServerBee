//! Real HTTP subscriptions/rules, migrated SQLite and security event entry points.
//! Only the external Relay/Apple boundary is substituted by the shared fixture.
use super::*;
use serde_json::{Value, json};
use serverbee_common::security::SecurityEventPayload;
use serverbee_server::entity::{mobile_push_outbox as outbox, security_event};

fn detection(
    kind: &str,
    ip: &str,
    count: u32,
    username: &str,
    first_seen: bool,
) -> SecurityEventPayload {
    let evidence = match kind {
        "ssh_login" => json!({"kind":kind,"auth_method":"publickey"}),
        "ssh_brute_force" => {
            json!({"kind":kind,"failed_count":count,"distinct_users":1,"sample_users":["root"],"invalid_user_count":0,"window_seconds":60,"threshold":10})
        }
        _ => {
            json!({"kind":kind,"distinct_ports":count,"sample_ports":[22,80,443],"total_attempts":count*2,"window_seconds":30,"threshold":5,"blocked_count":0})
        }
    };
    serde_json::from_value(json!({"event_type":kind,"severity":"high","source_ip":ip,
        "source_port":22,"username":username,"started_at":Utc::now().timestamp()-30,
        "ended_at":Utc::now().timestamp(),"first_seen":first_seen,"detector_source":"journal","evidence":evidence})).unwrap()
}

async fn security_register(client: &reqwest::Client, base: &str, access: &str, device: &str) {
    queued_register(client, base, access, device).await;
    assert_eq!(
        preferences_http(client, base, access, 2, intent(true, true))
            .await
            .status(),
        200
    );
    let confirmed = status_http(client, base, access).await;
    assert_eq!(confirmed["security_allowed"], true);
    assert_eq!(confirmed["preferences"]["security"], true);
    assert_eq!(confirmed["delivery_available"], true);
}

async fn rule(
    client: &reqwest::Client,
    base: &str,
    kind: &str,
    params: Value,
    cover: &str,
    servers: Vec<String>,
    enabled: bool,
) -> String {
    let res = client
        .post(format!("{base}/api/alert-rules"))
        .json(&json!({"name":format!("security-{kind}"),
        "rules":[{"rule_type":kind,"security":params}],"cover_type":cover,"server_ids":servers,
        "enabled":enabled,"notification_group_id":null}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    res.json::<Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .into()
}

async fn jobs(state: &AppState) -> Vec<outbox::Model> {
    outbox::Entity::find().all(&state.db).await.unwrap()
}

fn decrypted_delivery(request: &Value) -> Value {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ring::aead;
    let vector: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/push-envelope-v1.json"
    ))
    .unwrap();
    let envelope = &request["envelope"];
    let secret = STANDARD.decode(vector["key"].as_str().unwrap()).unwrap();
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
    let plaintext = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad.as_bytes()),
            &mut bytes,
        )
        .unwrap();
    serde_json::from_slice(plaintext).unwrap()
}

async fn wait_jobs(state: &AppState, expected: usize, outcome: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            let rows = jobs(state).await;
            if rows.len() == expected && rows.iter().all(|r| r.outcome == outcome) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("security delivery reached expected outcome");
}

/// Authenticate and create a Server through HTTP. A real registered Agent WS
/// is used by the category fan-out test below; other cases enter record_event,
/// the same production persistence/evaluation boundary, without private helpers.
async fn admin_client(base: &str) -> (reqwest::Client, Value) {
    let client = reqwest::Client::new();
    let login = login_http(&client, base, "admin", "security-a").await;
    let client = reqwest::Client::builder()
        .default_headers(bearer(login["access_token"].as_str().unwrap()))
        .build()
        .unwrap();
    (client, login)
}

#[tokio::test]
async fn security_rules_deliver_one_category_per_event_to_each_admin_installation() {
    use futures_util::SinkExt;
    use serverbee_common::{constants::CAP_SECURITY_EVENTS, protocol::AgentMessage};
    let (base, state, _tmp, relay) = queued_setup().await;
    let (client, login) = admin_client(&base).await;
    security_register(
        &client,
        &base,
        login["access_token"].as_str().unwrap(),
        "device-a",
    )
    .await;
    let second = login_http(&client, &base, "admin", "security-b").await;
    security_register(
        &client,
        &base,
        second["access_token"].as_str().unwrap(),
        "device-b",
    )
    .await;
    let member = login_http(&client, &base, "member", "security-member").await;
    let member_access = member["access_token"].as_str().unwrap();
    queued_register(&client, &base, member_access, "device-member").await;
    assert_eq!(
        preferences_http(&client, &base, member_access, 2, intent(true, true))
            .await
            .status(),
        403
    );
    assert_eq!(
        status_http(&client, &base, member_access).await["preferences"]["security"],
        false
    );

    let (server_id, token) = common::register_agent(&client, &base).await;
    for _ in 0..2 {
        rule(
            &client,
            &base,
            "ssh_brute_force_detected",
            json!({"min_failed_count":10,"dedupe_window_seconds":300}),
            "all",
            vec![],
            true,
        )
        .await;
    }
    let (mut sink, mut reader) = common::connect_agent(&base, &token).await;
    common::send_system_info(
        &mut sink,
        &mut reader,
        "security-handshake",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    let payload = detection("ssh_brute_force", "203.0.113.9", 20, "root", false);
    sink.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&AgentMessage::SecurityEvent(payload.clone()))
            .unwrap()
            .into(),
    ))
    .await
    .unwrap();
    wait_jobs(&state, 2, "pending").await;
    let queued = jobs(&state).await;
    assert_eq!(
        queued[0].event_id, queued[1].event_id,
        "two admitted rules share the raw event identity"
    );
    for job in &queued {
        assert_eq!(job.category, "security");
        assert_eq!(job.recipient_role, "admin");
        assert_eq!(job.expires_at - job.created_at, 1800);
        assert_ne!(job.installation_id, "security-member");
        let encoded = job.envelope.as_ref().unwrap();
        assert!(!encoded.contains(&server_id));
        assert!(!encoded.contains("203.0.113.9"));
        assert!(!encoded.contains("content_key"));
    }
    let event_id = queued[0].event_id.clone();
    let detail = client
        .get(format!("{base}/api/security/events/{event_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(detail.status(), 200);
    assert_eq!(
        detail.json::<Value>().await.unwrap()["data"]["server_id"],
        server_id
    );
    serverbee_server::service::alert::AlertService::evaluate_all(
        &state.db,
        &state.config,
        &state.agent_manager,
        &state.alert_state_manager,
    )
    .await
    .unwrap();
    assert_eq!(
        jobs(&state).await.len(),
        2,
        "polling does not emit a second general-alert category"
    );
    state
        .security_service
        .record_event(&server_id, payload)
        .await
        .unwrap();
    assert_eq!(
        jobs(&state).await.len(),
        2,
        "existing rule cooldown suppresses mobile repeats"
    );
    assert_eq!(
        security_event::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .len(),
        2,
        "raw event persistence is retained"
    );
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    wait_jobs(&state, 2, "accepted").await;
    worker.abort();
    let _ = worker.await;
    assert_eq!(relay.requests().await.len(), 2);
    for request in relay.requests().await {
        let bytes = request.to_string();
        assert!(!bytes.contains(&server_id));
        assert!(!bytes.contains("203.0.113.9"));
        assert!(!bytes.contains("content_key"));
        let content = decrypted_delivery(&request);
        assert_eq!(content["kind"], "security");
        assert_eq!(content["server_id"], server_id);
        assert_eq!(content["security_event_id"], event_id);
        assert_eq!(content["event_id"], event_id);
        assert_eq!(content["security_event_type"], "ssh_brute_force");
        assert!(matches!(
            content["installation_id"].as_str(),
            Some("security-a" | "security-b")
        ));
    }
    sink.close().await.unwrap();
}

#[tokio::test]
async fn security_admission_preserves_thresholds_exclusions_coverage_and_maintenance() {
    let (base, state, _tmp, _relay) = queued_setup().await;
    let (client, login) = admin_client(&base).await;
    security_register(
        &client,
        &base,
        login["access_token"].as_str().unwrap(),
        "device-a",
    )
    .await;
    let server = common::create_server(&client, &base, "security-target").await;
    rule(
        &client,
        &base,
        "ssh_brute_force_detected",
        json!({"min_failed_count":10}),
        "all",
        vec![],
        true,
    )
    .await;
    rule(
        &client,
        &base,
        "port_scan_detected",
        json!({"min_distinct_ports":5}),
        "include",
        vec![server.clone()],
        true,
    )
    .await;
    rule(
        &client,
        &base,
        "ssh_new_ip_login",
        json!({"exclude_users":["ROOT"],"exclude_cidrs":["10.0.0.0/8","198.51.100.7"]}),
        "all",
        vec![],
        true,
    )
    .await;
    rule(
        &client,
        &base,
        "ssh_brute_force_detected",
        json!({"min_failed_count":1}),
        "exclude",
        vec![server.clone()],
        true,
    )
    .await;
    rule(
        &client,
        &base,
        "port_scan_detected",
        json!({"min_distinct_ports":1}),
        "all",
        vec![],
        false,
    )
    .await;
    for payload in [
        detection("ssh_brute_force", "203.0.113.1", 9, "root", false),
        detection("port_scan", "203.0.113.2", 4, "root", false),
        detection("ssh_login", "203.0.113.3", 0, "root", true),
        detection("ssh_login", "10.1.2.3", 0, "alice", true),
        detection("ssh_login", "198.51.100.7", 0, "alice", true),
        detection("ssh_login", "203.0.113.4", 0, "alice", false),
    ] {
        state
            .security_service
            .record_event(&server, payload)
            .await
            .unwrap();
        assert!(
            jobs(&state).await.is_empty(),
            "routine detections must not notify"
        );
    }
    for (kind, count) in [("ssh_brute_force", 10), ("port_scan", 5), ("ssh_login", 0)] {
        state
            .security_service
            .record_event(
                &server,
                detection(kind, "203.0.113.5", count, "alice", true),
            )
            .await
            .unwrap();
    }
    assert_eq!(
        jobs(&state).await.len(),
        3,
        "all three admitted types notify without a group"
    );
    let res = client
        .post(format!("{base}/api/maintenances"))
        .json(
            &json!({"title":"maintenance","start_at":Utc::now()-ChronoDuration::minutes(1),
        "end_at":Utc::now()+ChronoDuration::minutes(1),"server_ids_json":[server.clone()]}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    state
        .security_service
        .record_event(
            &server,
            detection("port_scan", "203.0.113.6", 20, "alice", true),
        )
        .await
        .unwrap();
    assert_eq!(
        jobs(&state).await.len(),
        3,
        "maintenance suppresses admitted security delivery"
    );
}

#[tokio::test]
async fn security_dispatch_rechecks_current_role_subscription_and_session() {
    for mutation in ["role", "subscription", "logout", "expiry"] {
        let (base, state, _tmp, relay) = queued_setup().await;
        let (client, login) = admin_client(&base).await;
        let access = login["access_token"].as_str().unwrap();
        security_register(&client, &base, access, "device-a").await;
        let server = common::create_server(&client, &base, "security-target").await;
        rule(
            &client,
            &base,
            "port_scan_detected",
            json!({"min_distinct_ports":5}),
            "all",
            vec![],
            true,
        )
        .await;
        state
            .security_service
            .record_event(
                &server,
                detection("port_scan", "203.0.113.7", 20, "alice", true),
            )
            .await
            .unwrap();
        assert_eq!(jobs(&state).await.len(), 1);
        match mutation {
            "role" => {
                // A separate administrator changes the recipient's current role.
                AuthService::create_user(&state.db, "operator", "testpass", "admin")
                    .await
                    .unwrap();
                let operator =
                    login_http(&reqwest::Client::new(), &base, "operator", "operator").await;
                let res = reqwest::Client::new()
                    .put(format!(
                        "{base}/api/users/{}",
                        login["user"]["id"].as_str().unwrap()
                    ))
                    .bearer_auth(operator["access_token"].as_str().unwrap())
                    .json(&json!({"role":"member"}))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(res.status(), 200);
            }
            "subscription" => {
                assert_eq!(
                    preferences_http(&client, &base, access, 3, intent(false, true))
                        .await
                        .status(),
                    200
                );
            }
            "logout" => {
                assert_eq!(
                    client
                        .post(format!("{base}/api/mobile/auth/logout"))
                        .bearer_auth(access)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    200
                );
            }
            _ => {
                // Advance only the persisted session time boundary.
                mobile_session::Entity::update_many()
                    .col_expr(
                        mobile_session::Column::ExpiresAt,
                        sea_orm::sea_query::Expr::value(Utc::now() - ChronoDuration::seconds(1)),
                    )
                    .filter(mobile_session::Column::InstallationId.eq("security-a"))
                    .exec(&state.db)
                    .await
                    .unwrap();
            }
        }
        state
            .security_service
            .record_event(
                &server,
                detection("port_scan", "203.0.113.11", 20, "alice", true),
            )
            .await
            .unwrap();
        assert_eq!(
            jobs(&state).await.len(),
            1,
            "{mutation} also stops new admission"
        );
        relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
        let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
        wait_jobs(&state, 1, "permanent").await;
        worker.abort();
        let _ = worker.await;
        assert!(
            relay.requests().await.is_empty(),
            "{mutation} stops queued delivery before the Relay boundary"
        );
    }
}

#[tokio::test]
async fn security_queue_survives_restart_and_keeps_original_expiry() {
    let (base, state, tmp, relay) = queued_setup().await;
    let (client, login) = admin_client(&base).await;
    security_register(
        &client,
        &base,
        login["access_token"].as_str().unwrap(),
        "device-a",
    )
    .await;
    let server = common::create_server(&client, &base, "security-target").await;
    rule(
        &client,
        &base,
        "port_scan_detected",
        json!({"min_distinct_ports":5}),
        "all",
        vec![],
        true,
    )
    .await;
    state
        .security_service
        .record_event(
            &server,
            detection("port_scan", "203.0.113.8", 20, "alice", true),
        )
        .await
        .unwrap();
    let original = jobs(&state).await.remove(0);
    let db = Database::connect(format!(
        "sqlite://{}/test.db?mode=rwc",
        tmp.path().display()
    ))
    .await
    .unwrap();
    let restarted = AppState::new(db, state.config.clone()).await.unwrap();
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(restarted.clone());
    wait_jobs(&restarted, 1, "accepted").await;
    worker.abort();
    let _ = worker.await;
    let final_job = jobs(&restarted).await.remove(0);
    assert_eq!(final_job.event_id, original.event_id);
    assert_eq!(final_job.expires_at, original.created_at + 1800);
    assert!(final_job.envelope.is_none());
    assert_eq!(relay.requests().await.len(), 1);
}

#[tokio::test]
async fn security_queue_expires_without_delivering_after_thirty_minutes() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let (client, login) = admin_client(&base).await;
    security_register(
        &client,
        &base,
        login["access_token"].as_str().unwrap(),
        "device-a",
    )
    .await;
    let server = common::create_server(&client, &base, "security-target").await;
    rule(
        &client,
        &base,
        "port_scan_detected",
        json!({"min_distinct_ports":5}),
        "all",
        vec![],
        true,
    )
    .await;
    state
        .security_service
        .record_event(
            &server,
            detection("port_scan", "203.0.113.10", 20, "alice", true),
        )
        .await
        .unwrap();
    let original = jobs(&state).await.remove(0);
    assert_eq!(original.expires_at - original.created_at, 1800);
    // Move the persisted event deadline into the past; worker expiry policy,
    // subscriptions and transport remain real.
    outbox::Entity::update_many()
        .col_expr(
            outbox::Column::CreatedAt,
            sea_orm::sea_query::Expr::value(Utc::now().timestamp() - 1801),
        )
        .col_expr(
            outbox::Column::ExpiresAt,
            sea_orm::sea_query::Expr::value(Utc::now().timestamp() - 1),
        )
        .exec(&state.db)
        .await
        .unwrap();
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    wait_jobs(&state, 1, "expired").await;
    worker.abort();
    let _ = worker.await;
    assert!(jobs(&state).await.remove(0).envelope.is_none());
    assert!(relay.requests().await.is_empty());
}

#[tokio::test]
async fn security_admission_rolls_back_cooldown_and_fanout_on_real_sqlite_failure() {
    use serverbee_server::entity::alert_state;
    let (base, state, tmp, relay) = queued_setup().await;
    let (client, login) = admin_client(&base).await;
    security_register(
        &client,
        &base,
        login["access_token"].as_str().unwrap(),
        "device-a",
    )
    .await;
    let second = login_http(&client, &base, "admin", "security-b").await;
    security_register(
        &client,
        &base,
        second["access_token"].as_str().unwrap(),
        "device-b",
    )
    .await;
    let server = common::create_server(&client, &base, "atomic-security").await;
    let rule_id = rule(
        &client,
        &base,
        "port_scan_detected",
        json!({"min_distinct_ports":5,"dedupe_window_seconds":0}),
        "all",
        vec![],
        true,
    )
    .await;
    let original_payload = detection("port_scan", "203.0.113.12", 20, "alice", true);
    let original = state
        .security_service
        .record_event(&server, original_payload.clone())
        .await
        .unwrap();
    let before = alert_state::Entity::find().all(&state.db).await.unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].count, 1);
    assert_eq!(jobs(&state).await.len(), 2);
    // Abort one installation insert using an actual SQLite storage failure.
    // All other installations, raw evidence, sliding state and cache must roll back.
    state
        .db
        .execute_unprepared(
            "CREATE TRIGGER fail_security_fanout BEFORE INSERT ON mobile_push_outbox
         WHEN NEW.installation_id='security-b'
         BEGIN SELECT RAISE(ABORT, 'fixture outbox insert failure'); END",
        )
        .await
        .unwrap();
    let mut browser = state.browser_tx.subscribe();
    assert!(
        state
            .security_service
            .record_event(&server, original_payload.clone())
            .await
            .is_err()
    );
    let fresh_payload = detection("port_scan", "203.0.113.13", 20, "alice", true);
    assert!(
        state
            .security_service
            .record_event(&server, fresh_payload.clone())
            .await
            .is_err()
    );
    assert_eq!(
        before,
        alert_state::Entity::find().all(&state.db).await.unwrap(),
        "failure cannot advance existing suppression or create a new admitted state"
    );
    assert_eq!(
        state
            .alert_state_manager
            .get_info(&rule_id, &server, "203.0.113.12")
            .unwrap()
            .count,
        1
    );
    assert!(
        state
            .alert_state_manager
            .get_info(&rule_id, &server, "203.0.113.13")
            .is_none()
    );
    assert_eq!(
        security_event::Entity::find()
            .all(&state.db)
            .await
            .unwrap()
            .len(),
        1,
        "failed raw event is not reported as accepted"
    );
    assert_eq!(jobs(&state).await.len(), 2);
    assert!(
        jobs(&state)
            .await
            .iter()
            .all(|job| job.event_id == original)
    );
    assert!(matches!(
        browser.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    assert!(relay.requests().await.is_empty());
    state
        .db
        .execute_unprepared("DROP TRIGGER fail_security_fanout")
        .await
        .unwrap();
    let db = Database::connect(format!(
        "sqlite://{}/test.db?mode=rwc",
        tmp.path().display()
    ))
    .await
    .unwrap();
    let restarted = AppState::new(db, state.config.clone()).await.unwrap();
    let retry = restarted
        .security_service
        .record_event(&server, fresh_payload)
        .await
        .unwrap();
    assert_eq!(
        jobs(&restarted).await.len(),
        4,
        "retry after restart admits every device once"
    );
    assert_eq!(
        jobs(&restarted)
            .await
            .iter()
            .filter(|job| job.event_id == retry)
            .count(),
        2
    );
    assert_eq!(
        security_event::Entity::find()
            .all(&restarted.db)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        restarted
            .alert_state_manager
            .get_info(&rule_id, &server, "203.0.113.13")
            .unwrap()
            .count,
        1
    );
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(restarted.clone());
    wait_jobs(&restarted, 4, "accepted").await;
    worker.abort();
    let _ = worker.await;
    assert_eq!(relay.requests().await.len(), 4);
}
