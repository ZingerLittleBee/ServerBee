//! Once-only production Agent WS admission under real file-SQLite faults.
use super::*;
use futures_util::SinkExt;
use serverbee_common::{constants::CAP_SECURITY_EVENTS, protocol::AgentMessage};
use serverbee_server::entity::{alert_state, block_list, mobile_push_registration, user};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, task::JoinHandle};

struct OnceOnlyFixture {
    base: String,
    state: Arc<AppState>,
    tmp: tempfile::TempDir,
    relay: OutboxRelay,
    client: reqwest::Client,
    login: Value,
    operator_access: String,
    server: String,
    rule: String,
    sink: common::AgentSink,
    _reader: common::AgentReader,
    connected: bool,
    external: Arc<Mutex<Vec<String>>>,
    recovery: Option<JoinHandle<()>>,
}

impl Drop for OnceOnlyFixture {
    fn drop(&mut self) {
        if let Some(worker) = &self.recovery {
            worker.abort();
        }
    }
}

impl OnceOnlyFixture {
    async fn new(kind: &str) -> Self {
        use axum::{Router, routing::post};
        let (base, state, tmp, relay) = queued_setup().await;
        let (client, login) = admin_client(&base).await;
        security_register(
            &client,
            &base,
            login["access_token"].as_str().unwrap(),
            "device-a",
        )
        .await;
        AuthService::create_user(&state.db, "operator", "testpass", "admin")
            .await
            .unwrap();
        let operator = login_http(&client, &base, "operator", "security-b").await;
        let operator_access = operator["access_token"].as_str().unwrap().to_owned();
        security_register(&client, &base, &operator_access, "device-b").await;
        let (server, token) = common::register_agent(&client, &base).await;
        let external = Arc::new(Mutex::new(Vec::new()));
        let captured = external.clone();
        let app = Router::new().route(
            "/hook",
            post(move |body: String| {
                let captured = captured.clone();
                async move {
                    captured.lock().await.push(body);
                    "ok"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let webhook = format!("http://{}/hook", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let channel = client
            .post(format!("{base}/api/notifications"))
            .json(
                &json!({"name":"once-only-external","notify_type":"webhook","enabled":true,
                "config_json":{"url":webhook,"body_template":"{{server_id}}|{{message}}"}}),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(channel.status(), 200);
        let channel = channel.json::<Value>().await.unwrap()["data"]["id"].clone();
        let group = client
            .post(format!("{base}/api/notification-groups"))
            .json(&json!({"name":"once-only-group","notification_ids":[channel]}))
            .send()
            .await
            .unwrap();
        assert_eq!(group.status(), 200);
        let group = group.json::<Value>().await.unwrap()["data"]["id"].clone();
        let rule_type = if kind == "ssh_login" {
            "ssh_new_ip_login"
        } else {
            "port_scan_detected"
        };
        let actions = if kind == "ssh_login" {
            json!([])
        } else {
            json!([{"type":"block_source_ip","cover_type":"all"}])
        };
        let res = client
            .post(format!("{base}/api/alert-rules"))
            .json(
                &json!({"name":"once-only-rule","cover_type":"all","enabled":true,
                "rules":[{"rule_type":rule_type,"security":{"dedupe_window_seconds":300}}],
                "notification_group_id":group,"actions":actions}),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let rule = res.json::<Value>().await.unwrap()["data"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        // A second matching rule must still produce one mobile category/event.
        super::rule(
            &client,
            &base,
            rule_type,
            json!({"dedupe_window_seconds":300}),
            "all",
            vec![],
            true,
        )
        .await;
        let (mut sink, mut reader) = common::connect_agent(&base, &token).await;
        common::send_system_info(
            &mut sink,
            &mut reader,
            "once-only-agent",
            Some(CAP_SECURITY_EVENTS),
        )
        .await;
        let recovery = Some(state.security_service.start_recovery());
        Self {
            base,
            state,
            tmp,
            relay,
            client,
            login,
            operator_access,
            server,
            rule,
            sink,
            _reader: reader,
            connected: true,
            external,
            recovery,
        }
    }

    async fn send_once(&mut self, payload: &SecurityEventPayload) {
        self.sink
            .send(tokio_tungstenite::tungstenite::Message::Text(
                serde_json::to_string(&AgentMessage::SecurityEvent(payload.clone()))
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }

    async fn fault(&self, kind: &str) {
        let sql = match kind {
            "intent" => {
                "CREATE TRIGGER fail_once_only BEFORE UPDATE ON security_event
                WHEN OLD.admission_payload IS NOT NULL AND NEW.admission_payload IS NULL
                BEGIN SELECT RAISE(ABORT, 'fixture intent commit failure'); END"
            }
            "raw" => {
                "CREATE TRIGGER fail_once_only BEFORE INSERT ON security_event
                BEGIN SELECT RAISE(ABORT, 'fixture raw write failure'); END"
            }
            _ => {
                "CREATE TRIGGER fail_once_only BEFORE INSERT ON mobile_push_outbox
                WHEN NEW.installation_id='security-b'
                BEGIN SELECT RAISE(ABORT, 'fixture fanout write failure'); END"
            }
        };
        self.state.db.execute_unprepared(sql).await.unwrap();
    }

    async fn clear_fault(&self) {
        self.state
            .db
            .execute_unprepared("DROP TRIGGER fail_once_only")
            .await
            .unwrap();
    }

    async fn event(&self, admission_pending: bool, push_pending: bool) -> security_event::Model {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let rows = security_event::Entity::find()
                    .all(&self.state.db)
                    .await
                    .unwrap();
                if rows.len() == 1
                    && rows[0].admission_payload.is_some() == admission_pending
                    && rows[0].push_intent.is_some() == push_pending
                {
                    break rows.into_iter().next().unwrap();
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("original event reached the expected durable phase")
    }

    async fn external_count(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if self.external.lock().await.len() == expected {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("existing external channel effect was preserved");
    }

    async fn restart(&mut self) {
        if let Some(worker) = self.recovery.take() {
            worker.abort();
            let _ = worker.await;
        }
        if self.connected {
            self.sink.close().await.unwrap();
            self.connected = false;
        }
        let db = Database::connect(format!(
            "sqlite://{}/test.db?mode=rwc",
            self.tmp.path().display()
        ))
        .await
        .unwrap();
        self.state = AppState::new(db, self.state.config.clone()).await.unwrap();
        self.recovery = Some(self.state.security_service.start_recovery());
    }

    async fn assert_original(&self, original: &security_event::Model, expected_jobs: usize) {
        let recovered = self.event(false, false).await;
        let mut expected = original.clone();
        expected.admission_payload = None;
        expected.push_intent = None;
        assert_eq!(
            recovered, expected,
            "every original security fact survives recovery"
        );
        let queued = jobs(&self.state).await;
        assert_eq!(queued.len(), expected_jobs);
        for job in queued {
            assert_eq!(job.event_id, original.id);
            assert_eq!(job.category, "security");
            assert_eq!(job.created_at, original.created_at.timestamp());
            assert_eq!(job.expires_at, original.created_at.timestamp() + 1800);
        }
        let states = alert_state::Entity::find()
            .all(&self.state.db)
            .await
            .unwrap();
        assert_eq!(states.len(), 2);
        assert!(
            states
                .iter()
                .all(|s| s.count == 1 && s.last_notified_at == original.created_at)
        );
        self.external_count(1).await;
    }
}

async fn once_only_push_failure_recovers(restart: bool, kind: &str) {
    let mut f = OnceOnlyFixture::new(kind).await;
    f.fault("outbox").await;
    let payload = detection(kind, "203.0.113.42", 20, "alice", true);
    let mut browser = f.state.browser_tx.subscribe();
    f.send_once(&payload).await;
    let original = f.event(false, true).await;
    let detail = f
        .client
        .get(format!("{}/api/security/events/{}", f.base, original.id))
        .send()
        .await
        .unwrap();
    assert_eq!(
        detail.status(),
        200,
        "raw history is available during the push fault"
    );
    let detail = detail.json::<Value>().await.unwrap();
    assert_eq!(detail["data"]["server_id"], f.server);
    assert_eq!(
        detail["data"]["evidence"],
        serde_json::to_value(&payload.evidence).unwrap()
    );
    assert!(detail["data"].get("admission_payload").is_none());
    assert!(detail["data"].get("push_intent").is_none());
    let broadcast = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let serverbee_common::protocol::BrowserMessage::SecurityEvent(event) =
                browser.recv().await.unwrap()
            {
                break event;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(broadcast.event_id, original.id);
    assert_eq!(
        serde_json::to_value(broadcast.event).unwrap(),
        serde_json::to_value(&payload).unwrap()
    );
    assert!(
        jobs(&f.state).await.is_empty(),
        "failed second recipient leaves no partial fan-out"
    );
    f.external_count(1).await;
    let external = f.external.lock().await;
    assert!(external[0].contains(&f.server));
    assert!(external[0].contains(&payload.source_ip));
    drop(external);
    let snapshot = alert_state::Entity::find().all(&f.state.db).await.unwrap();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(
        f.state
            .alert_state_manager
            .get_info(&f.rule, &f.server, &payload.source_ip)
            .unwrap()
            .count,
        1
    );
    let plan: Value = serde_json::from_str(original.push_intent.as_ref().unwrap()).unwrap();
    assert_eq!(plan.as_array().unwrap().len(), 2);
    assert!(
        !original
            .push_intent
            .as_ref()
            .unwrap()
            .contains("content_key")
    );
    assert!(
        !original
            .push_intent
            .as_ref()
            .unwrap()
            .contains("grant_token")
    );
    if kind == "port_scan" {
        let blocks = block_list::Entity::find().all(&f.state.db).await.unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].origin_event_id.as_ref(), Some(&original.id));
    }
    if restart {
        f.restart().await;
    }
    f.clear_fault().await;
    // No redetection, no record_event call: only the production recovery owner.
    f.assert_original(&original, 2).await;
    assert_eq!(
        snapshot,
        alert_state::Entity::find().all(&f.state.db).await.unwrap()
    );
    f.relay
        .status
        .store(200, std::sync::atomic::Ordering::SeqCst);
    let delivery = serverbee_server::service::mobile_push_outbox::start(f.state.clone());
    wait_jobs(&f.state, 2, "accepted").await;
    delivery.abort();
    let _ = delivery.await;
    assert_eq!(f.relay.requests().await.len(), 2);
    for request in f.relay.requests().await {
        let content = decrypted_delivery(&request);
        assert_eq!(content["event_id"], original.id);
        assert_eq!(content["security_event_id"], original.id);
        assert_eq!(content["server_id"], f.server);
        assert_eq!(content["created_at"], original.created_at.timestamp());
        assert_eq!(
            content["expires_at"],
            original.created_at.timestamp() + 1800
        );
    }
    // Replay workers on the same file cannot advance suppression or effects again.
    f.restart().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(jobs(&f.state).await.len(), 2);
    assert_eq!(f.external.lock().await.len(), 1);
    assert_eq!(
        snapshot,
        alert_state::Entity::find().all(&f.state.db).await.unwrap()
    );
}

#[tokio::test]
async fn security_once_only_ssh_ws_recovers_after_outbox_fault_without_redetection() {
    once_only_push_failure_recovers(false, "ssh_login").await;
}

#[tokio::test]
async fn security_once_only_ssh_ws_recovers_original_identity_and_deadline_after_restart() {
    once_only_push_failure_recovers(true, "ssh_login").await;
}

#[tokio::test]
async fn security_once_only_scan_ws_preserves_browser_firewall_and_external_effects() {
    once_only_push_failure_recovers(true, "port_scan").await;
}

#[tokio::test]
async fn security_once_only_ws_intent_commit_failure_preserves_raw_and_recovers_after_restart() {
    let mut f = OnceOnlyFixture::new("ssh_login").await;
    f.fault("intent").await;
    let payload = detection("ssh_login", "203.0.113.42", 0, "alice", true);
    f.send_once(&payload).await;
    let original = f.event(true, false).await;
    assert_eq!(
        serde_json::from_str::<Value>(original.admission_payload.as_ref().unwrap()).unwrap(),
        serde_json::to_value(&payload).unwrap()
    );
    assert!(
        alert_state::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        f.state
            .alert_state_manager
            .get_info(&f.rule, &f.server, &payload.source_ip)
            .is_none()
    );
    assert!(jobs(&f.state).await.is_empty());
    assert!(f.external.lock().await.is_empty());
    f.restart().await;
    f.clear_fault().await;
    f.assert_original(&original, 2).await;
}

#[tokio::test]
async fn security_once_only_ws_waits_for_raw_storage_instead_of_consuming_failed_intent() {
    let mut f = OnceOnlyFixture::new("ssh_login").await;
    f.fault("raw").await;
    let payload = detection("ssh_login", "203.0.113.42", 0, "alice", true);
    f.send_once(&payload).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        security_event::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    f.clear_fault().await;
    let original = f.event(false, false).await;
    assert!(original.first_seen);
    assert_eq!(original.source_ip, payload.source_ip);
    assert_eq!(original.ended_at.timestamp(), payload.ended_at);
    f.assert_original(&original, 2).await;
}

#[tokio::test]
async fn security_once_only_ws_recovery_rechecks_current_recipient_bindings() {
    for mutation in [
        "subscription",
        "role",
        "logout",
        "session_expiry",
        "grant",
        "revision",
        "owner",
    ] {
        let mut f = OnceOnlyFixture::new("ssh_login").await;
        f.fault("outbox").await;
        f.send_once(&detection("ssh_login", "203.0.113.42", 0, "alice", true))
            .await;
        let original = f.event(false, true).await;
        f.external_count(1).await;
        let access = f.login["access_token"].as_str().unwrap();
        match mutation {
            "subscription" => assert_eq!(
                preferences_http(&f.client, &f.base, access, 3, intent(false, true))
                    .await
                    .status(),
                200
            ),
            "role" => assert_eq!(
                f.client
                    .put(format!(
                        "{}/api/users/{}",
                        f.base,
                        f.login["user"]["id"].as_str().unwrap()
                    ))
                    .bearer_auth(&f.operator_access)
                    .json(&json!({"role":"member"}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            ),
            "logout" => assert_eq!(
                f.client
                    .post(format!("{}/api/mobile/auth/logout", f.base))
                    .bearer_auth(access)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            ),
            "session_expiry" => {
                mobile_session::Entity::update_many()
                    .col_expr(
                        mobile_session::Column::ExpiresAt,
                        sea_orm::sea_query::Expr::value(Utc::now() - ChronoDuration::seconds(1)),
                    )
                    .filter(mobile_session::Column::InstallationId.eq("security-a"))
                    .exec(&f.state.db)
                    .await
                    .unwrap();
            }
            "grant" => {
                mobile_push_registration::Entity::update_many()
                    .col_expr(
                        mobile_push_registration::Column::GrantExpiresAt,
                        sea_orm::sea_query::Expr::value(Utc::now() - ChronoDuration::seconds(1)),
                    )
                    .filter(mobile_push_registration::Column::InstallationId.eq("security-a"))
                    .exec(&f.state.db)
                    .await
                    .unwrap();
            }
            "revision" => {
                f.state.db.execute_unprepared("UPDATE mobile_push_registrations SET revision=revision+1 WHERE installation_id='security-a'").await.unwrap();
            }
            _ => {
                let replacement = user::Entity::find()
                    .filter(user::Column::Username.eq("operator"))
                    .one(&f.state.db)
                    .await
                    .unwrap()
                    .unwrap();
                mobile_push_registration::Entity::update_many()
                    .col_expr(
                        mobile_push_registration::Column::UserId,
                        sea_orm::sea_query::Expr::value(replacement.id),
                    )
                    .filter(mobile_push_registration::Column::InstallationId.eq("security-a"))
                    .exec(&f.state.db)
                    .await
                    .unwrap();
            }
        }
        f.restart().await;
        f.clear_fault().await;
        f.assert_original(&original, 1).await;
        assert_eq!(
            jobs(&f.state).await[0].installation_id,
            "security-b",
            "{mutation}"
        );
        assert!(f.relay.requests().await.is_empty());
    }
}

#[tokio::test]
async fn security_once_only_ws_recovery_never_renews_expired_original_window() {
    let mut f = OnceOnlyFixture::new("ssh_login").await;
    f.fault("outbox").await;
    f.send_once(&detection("ssh_login", "203.0.113.42", 0, "alice", true))
        .await;
    let original = f.event(false, true).await;
    f.external_count(1).await;
    // Advance the persisted original creation boundary, without another WS send.
    let expired = Utc::now() - ChronoDuration::seconds(1801);
    security_event::Entity::update_many()
        .col_expr(
            security_event::Column::CreatedAt,
            sea_orm::sea_query::Expr::value(expired),
        )
        .filter(security_event::Column::Id.eq(&original.id))
        .exec(&f.state.db)
        .await
        .unwrap();
    f.restart().await;
    f.clear_fault().await;
    let recovered = f.event(false, false).await;
    assert_eq!(recovered.id, original.id);
    assert_eq!(recovered.created_at, expired);
    assert!(jobs(&f.state).await.is_empty());
    assert!(f.relay.requests().await.is_empty());
    assert_eq!(f.external.lock().await.len(), 1);
}
