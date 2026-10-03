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
    token: String,
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
        Self::with_window(kind, 300).await
    }

    async fn with_window(kind: &str, window: u32) -> Self {
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
                "rules":[{"rule_type":rule_type,"security":{"dedupe_window_seconds":window}}],
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
            json!({"dedupe_window_seconds":window}),
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
            token,
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

    async fn assert_production_wal(&self) {
        use sea_orm::TransactionTrait;
        // Hold every pooled connection so options cannot accidentally be
        // verified repeatedly on only one connection.
        let mut connections = Vec::new();
        for _ in 0..self.state.config.database.max_connections {
            connections.push(self.state.db.begin().await.unwrap());
        }
        for connection in &connections {
            let mode = connection
                .query_one(sea_orm::Statement::from_string(
                    sea_orm::DatabaseBackend::Sqlite,
                    "PRAGMA journal_mode".to_owned(),
                ))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(mode.try_get::<String>("", "journal_mode").unwrap(), "wal");
            for (pragma, value) in [
                ("synchronous", 1_i64),
                ("foreign_keys", 1),
                ("busy_timeout", 5000),
            ] {
                let row = connection
                    .query_one(sea_orm::Statement::from_string(
                        sea_orm::DatabaseBackend::Sqlite,
                        format!("PRAGMA {pragma}"),
                    ))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(row.try_get::<i64>("", pragma).unwrap(), value);
            }
        }
        for connection in connections {
            connection.rollback().await.unwrap();
        }
    }

    async fn revoke_response(&self) -> (reqwest::StatusCode, String) {
        let response = tokio::time::timeout(
            Duration::from_secs(8),
            self.client
                .delete(format!(
                    "{}/api/servers/{}/agent-authority",
                    self.base, self.server
                ))
                .send(),
        )
        .await
        .expect("revocation is bounded by the production write wait")
        .unwrap();
        let status = response.status();
        let body = response.text().await.unwrap();
        (status, body)
    }

    async fn original_for(&self, username: &str) -> security_event::Model {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if let Some(event) = security_event::Entity::find()
                    .filter(security_event::Column::Username.eq(username))
                    .one(&self.state.db)
                    .await
                    .unwrap()
                {
                    break event;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("once-only WS raw event persisted")
    }

    async fn stop_recovery(&mut self) {
        if let Some(worker) = self.recovery.take() {
            worker.abort();
            let _ = worker.await;
        }
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
        let db = production_wal_db(
            &self.tmp.path().join("test.db"),
            self.state.config.database.max_connections,
        )
        .await;
        self.state = AppState::new(db, self.state.config.clone()).await.unwrap();
        self.base = serve_outbox_http(self.state.clone()).await;
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
    assert!(detail["data"].get("authority_fingerprint").is_none());
    assert!(detail["data"].get("maintenance_at_admission").is_none());
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
    // The fault stays active throughout the message and Pong regressions.
    common::send_system_info(
        &mut f.sink,
        &mut f._reader,
        "raw-fault-barrier",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    let accepted_before = Utc::now();
    f.sink
        .send(tokio_tungstenite::tungstenite::Message::Text(
            json!({
                "type":"report", "cpu":12.5,"mem_used":4000,"swap_used":0,"disk_used":2000,
                "net_in_speed":1,"net_out_speed":2,"net_in_transfer":3,"net_out_transfer":4,
                "load1":0.5,"load5":0.4,"load15":0.3,"tcp_conn":42,"udp_conn":7,
                "process_count":120,"uptime":86400,"temperature":55.0,"gpu":null
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while f.state.agent_manager.get_latest_report(&f.server).is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("report progresses while raw INSERT keeps failing");
    assert_eq!(
        f.state
            .agent_manager
            .get_latest_report(&f.server)
            .unwrap()
            .cpu,
        12.5
    );
    // Wait until only Pong can move the connection out of the stale set.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(
        f.state
            .agent_manager
            .stale_connection_candidates(1)
            .iter()
            .any(|(id, _)| id == &f.server)
    );
    f.sink
        .send(tokio_tungstenite::tungstenite::Message::Pong(
            Vec::new().into(),
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while f
            .state
            .agent_manager
            .stale_connection_candidates(1)
            .iter()
            .any(|(id, _)| id == &f.server)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Pong progresses while the raw fault stays active");
    assert!(
        security_event::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    // Stopping and restarting this owner preserves service-owned memory. This
    // is deliberately not a claim of process-death recovery before durability.
    let worker = f.recovery.take().unwrap();
    worker.abort();
    let _ = worker.await;
    f.recovery = Some(f.state.security_service.start_recovery());
    f.clear_fault().await;
    let original = f.event(false, false).await;
    assert!(
        original.created_at <= accepted_before,
        "raw retries never renew event time"
    );
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

#[tokio::test]
async fn security_raw_fault_allows_connection_replacement_and_close_without_resend() {
    let mut f = OnceOnlyFixture::new("ssh_login").await;
    f.fault("raw").await;
    let payload = detection("ssh_login", "203.0.113.42", 0, "alice", true);
    f.send_once(&payload).await;
    common::send_system_info(
        &mut f.sink,
        &mut f._reader,
        "retained-before-reconnect",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    let accepted_before = Utc::now();
    let (mut replacement, mut reader) = tokio::time::timeout(
        Duration::from_secs(3),
        common::connect_agent(&f.base, &f.token),
    )
    .await
    .expect("replacement admission must not wait for raw recovery");
    common::send_system_info(
        &mut replacement,
        &mut reader,
        "replacement-with-fault",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    let current = f
        .state
        .agent_manager
        .stale_connection_candidates(0)
        .into_iter()
        .find(|(id, _)| id == &f.server)
        .unwrap()
        .1;
    // Superseded cleanup must not remove the new socket.
    let _ = f.sink.close().await;
    common::send_system_info(
        &mut replacement,
        &mut reader,
        "after-old-close",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    assert!(
        f.state
            .agent_manager
            .is_current_connection(&f.server, current)
    );
    assert!(
        security_event::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    replacement.close().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while f.state.agent_manager.is_online(&f.server) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("current socket cleanup progresses with raw fault active");
    f.connected = false;
    f.clear_fault().await;
    let original = f.event(false, false).await;
    assert!(original.created_at <= accepted_before);
    assert_eq!(original.source_ip, payload.source_ip);
    assert_eq!(original.ended_at.timestamp(), payload.ended_at);
    f.assert_original(&original, 2).await;
    assert!(
        !f.state.agent_manager.is_online(&f.server),
        "recovery never restores a closed connection"
    );
}

#[tokio::test]
async fn security_raw_fault_allows_authority_revocation_and_cancels_stale_recovery() {
    let mut f = OnceOnlyFixture::new("port_scan").await;
    f.fault("raw").await;
    f.send_once(&detection("port_scan", "203.0.113.42", 10, "alice", true))
        .await;
    common::send_system_info(
        &mut f.sink,
        &mut f._reader,
        "before-revoke",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        f.client
            .delete(format!(
                "{}/api/servers/{}/agent-authority",
                f.base, f.server
            ))
            .send(),
    )
    .await
    .expect("revocation must not wait for raw INSERT recovery")
    .unwrap();
    assert_eq!(response.status(), 200);
    assert!(!f.state.agent_manager.is_online(&f.server));
    assert!(
        security_event::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    f.clear_fault().await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        security_event::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(jobs(&f.state).await.is_empty());
    assert!(
        block_list::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.external.lock().await.is_empty());
    assert!(!f.state.agent_manager.is_online(&f.server));
    let url = format!(
        "{}/api/agent/ws?token={}",
        f.base.replace("http://", "ws://"),
        f.token
    );
    assert!(
        tokio_tungstenite::connect_async(url).await.is_err(),
        "old authority remains invalid"
    );
    f.connected = false;
}

#[tokio::test]
async fn security_raw_fault_rechecks_agent_capability_before_storage() {
    let mut f = OnceOnlyFixture::new("port_scan").await;
    f.fault("raw").await;
    f.send_once(&detection("port_scan", "203.0.113.42", 10, "alice", true))
        .await;
    common::send_system_info(&mut f.sink, &mut f._reader, "capability-revoked", Some(0)).await;
    f.clear_fault().await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        security_event::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(jobs(&f.state).await.is_empty());
    assert!(
        block_list::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.external.lock().await.is_empty());
    common::send_system_info(
        &mut f.sink,
        &mut f._reader,
        "capability-restored",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        security_event::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty(),
        "restoring a capability cannot revive canceled detections"
    );
}

#[tokio::test]
async fn security_durable_admission_rechecks_authority_after_restart() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .with_test_writer()
        .try_init();
    let mut f = OnceOnlyFixture::new("port_scan").await;
    f.assert_production_wal().await;
    f.fault("intent").await;
    f.send_once(&detection("port_scan", "203.0.113.42", 10, "alice", true))
        .await;
    let original = f.event(true, false).await;
    // A real competing writer makes the old read-then-upgrade path fail in
    // WAL. The admission fault stays installed throughout the HTTP request.
    use sea_orm::TransactionTrait;
    let writer = f.state.db.begin().await.unwrap();
    writer
        .execute_unprepared("UPDATE security_event SET push_intent=push_intent")
        .await
        .unwrap();
    let client = f.client.clone();
    let url = format!("{}/api/servers/{}/agent-authority", f.base, f.server);
    let revoke = tokio::spawn(async move {
        let response = client.delete(url).send().await.unwrap();
        let status = response.status();
        (status, response.text().await.unwrap())
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    let waited_for_writer = !revoke.is_finished();
    let stayed_online = f.state.agent_manager.is_online(&f.server);
    writer.commit().await.unwrap();
    let (status, body) = tokio::time::timeout(Duration::from_secs(8), revoke)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        status, 200,
        "DELETE agent-authority response: {body}; inspect captured database error/code above"
    );
    assert!(
        waited_for_writer,
        "revocation must serialize before reading authority"
    );
    assert!(
        stayed_online,
        "a blocked, uncommitted revocation cannot fence the socket"
    );
    let receipt: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(receipt["data"]["changed"], true);
    assert!(!f.state.agent_manager.is_online(&f.server));
    let stored = serverbee_server::entity::server::Entity::find_by_id(&f.server)
        .one(&f.state.db)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.token_hash.is_none());
    let history = serverbee_server::entity::agent_authority_event::Entity::find()
        .filter(serverbee_server::entity::agent_authority_event::Column::ServerId.eq(&f.server))
        .all(&f.state.db)
        .await
        .unwrap();
    assert!(
        history.iter().any(
            |event| event.authority_before == "claimed" && event.authority_after == "unclaimed"
        )
    );
    let url = format!(
        "{}/api/agent/ws?token={}",
        f.base.replace("http://", "ws://"),
        f.token
    );
    let error = match tokio_tungstenite::connect_async(url).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => response.status(),
        other => panic!(
            "old-token handshake must be rejected by HTTP authorization: {}",
            if other.is_ok() {
                "accepted"
            } else {
                "unexpected transport failure"
            }
        ),
    };
    assert_eq!(error.as_u16(), 401);
    assert!(
        f.event(true, false).await.admission_payload.is_some(),
        "fault remains installed through durable revocation"
    );
    f.connected = false;
    f.restart().await;
    f.assert_production_wal().await;
    f.clear_fault().await;
    let recovered = f.event(false, false).await;
    assert_eq!(recovered.id, original.id);
    assert_eq!(recovered.created_at, original.created_at);
    assert!(
        alert_state::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(jobs(&f.state).await.is_empty());
    assert!(
        block_list::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.external.lock().await.is_empty());
}

async fn maintenance_boundary_recovery(initially_active: bool) {
    let mut f = OnceOnlyFixture::new("port_scan").await;
    f.fault("intent").await;
    let boundary = Utc::now() + ChronoDuration::seconds(3);
    let (start, end) = if initially_active {
        (Utc::now() - ChronoDuration::seconds(60), boundary)
    } else {
        (boundary, boundary + ChronoDuration::seconds(60))
    };
    assert_eq!(f.client.post(format!("{}/api/maintenances", f.base)).json(&json!({
        "title":"original-event-window", "start_at":start,"end_at":end,"server_ids_json":[f.server.clone()]
    })).send().await.unwrap().status(), 200);
    let mut browser = f.state.browser_tx.subscribe();
    let payload = detection("port_scan", "203.0.113.42", 10, "alice", true);
    f.send_once(&payload).await;
    let original = f.event(true, false).await;
    assert!(
        original.created_at < boundary,
        "fixture event is received before the boundary"
    );
    assert_eq!(original.maintenance_at_admission, Some(initially_active));
    let broadcast = tokio::time::timeout(Duration::from_secs(3), async {
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
    assert!(jobs(&f.state).await.is_empty());
    assert!(
        block_list::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.external.lock().await.is_empty());
    while Utc::now() <= boundary + ChronoDuration::milliseconds(100) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        serverbee_server::service::maintenance::MaintenanceService::is_in_maintenance(
            &f.state.db,
            &f.server
        )
        .await
        .unwrap(),
        !initially_active
    );
    f.restart().await;
    f.clear_fault().await;
    let recovered = f.event(false, false).await;
    assert_eq!(recovered.id, original.id);
    assert_eq!(recovered.created_at, original.created_at);
    assert_eq!(recovered.maintenance_at_admission, Some(initially_active));
    if initially_active {
        assert!(jobs(&f.state).await.is_empty());
        assert!(
            alert_state::Entity::find()
                .all(&f.state.db)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            block_list::Entity::find()
                .all(&f.state.db)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(f.external.lock().await.is_empty());
        assert!(f.relay.requests().await.is_empty());
    } else {
        f.assert_original(&original, 2).await;
        let blocks = block_list::Entity::find().all(&f.state.db).await.unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].origin_event_id.as_deref(),
            Some(original.id.as_str())
        );
        f.relay
            .status
            .store(200, std::sync::atomic::Ordering::SeqCst);
        let worker = serverbee_server::service::mobile_push_outbox::start(f.state.clone());
        wait_jobs(&f.state, 2, "accepted").await;
        worker.abort();
        let _ = worker.await;
        assert_eq!(f.relay.requests().await.len(), 2);
        for request in f.relay.requests().await {
            let content = decrypted_delivery(&request);
            assert_eq!(content["event_id"], original.id);
            assert_eq!(content["created_at"], original.created_at.timestamp());
            assert_eq!(
                content["expires_at"],
                original.created_at.timestamp() + 1800
            );
        }
    }
}

#[tokio::test]
async fn security_once_only_maintenance_end_keeps_original_suppression_after_restart() {
    maintenance_boundary_recovery(true).await;
}

#[tokio::test]
async fn security_once_only_maintenance_start_keeps_original_admission_after_restart() {
    maintenance_boundary_recovery(false).await;
}

#[tokio::test]
async fn security_maintenance_lookup_failure_keeps_raw_browser_and_blocks_effects() {
    let mut f = OnceOnlyFixture::new("port_scan").await;
    assert_eq!(
        f.client
            .post(format!("{}/api/maintenances", f.base))
            .json(&json!({
                "title":"lookup-fault-window", "start_at":Utc::now()-ChronoDuration::minutes(1),
                "end_at":Utc::now()+ChronoDuration::minutes(1),"server_ids_json":[f.server.clone()]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // A real SQLite lookup failure, with raw history still writable.
    f.state
        .db
        .execute_unprepared("ALTER TABLE maintenance RENAME TO unavailable_maintenance")
        .await
        .unwrap();
    let mut browser = f.state.browser_tx.subscribe();
    let payload = detection("port_scan", "203.0.113.42", 10, "alice", true);
    f.send_once(&payload).await;
    let original = f.event(true, false).await;
    assert_eq!(original.maintenance_at_admission, None);
    let broadcast = tokio::time::timeout(Duration::from_secs(3), async {
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
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        alert_state::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(jobs(&f.state).await.is_empty());
    assert!(
        block_list::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.external.lock().await.is_empty());
    f.state
        .db
        .execute_unprepared("ALTER TABLE unavailable_maintenance RENAME TO maintenance")
        .await
        .unwrap();
    let recovered = f.event(false, false).await;
    assert_eq!(recovered.id, original.id);
    assert_eq!(recovered.created_at, original.created_at);
    assert_eq!(recovered.maintenance_at_admission, Some(true));
    assert!(jobs(&f.state).await.is_empty());
    assert!(
        block_list::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.external.lock().await.is_empty());
}

#[tokio::test]
async fn security_wal_failed_revocation_preserves_token_history_and_live_connection() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .with_test_writer()
        .try_init();
    let mut f = OnceOnlyFixture::new("port_scan").await;
    f.assert_production_wal().await;
    f.fault("intent").await;
    f.send_once(&detection("port_scan", "203.0.113.42", 10, "alice", true))
        .await;
    let original = f.event(true, false).await;
    let before = serverbee_server::entity::server::Entity::find_by_id(&f.server)
        .one(&f.state.db)
        .await
        .unwrap()
        .unwrap();
    let history_before = serverbee_server::entity::agent_authority_event::Entity::find()
        .filter(serverbee_server::entity::agent_authority_event::Column::ServerId.eq(&f.server))
        .all(&f.state.db)
        .await
        .unwrap();
    f.state
        .db
        .execute_unprepared(
            "CREATE TRIGGER fail_revocation_history BEFORE INSERT ON agent_authority_events
         WHEN NEW.transition='authority_revoked'
         BEGIN SELECT RAISE(ABORT, 'fixture revocation history failure'); END",
        )
        .await
        .unwrap();
    let (status, body) = f.revoke_response().await;
    assert_eq!(status, 500, "injected history fault response: {body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"]["message"],
        "Internal error: Database error"
    );
    assert!(
        f.state.agent_manager.is_online(&f.server),
        "rollback must preserve the authorized socket"
    );
    let after = serverbee_server::entity::server::Entity::find_by_id(&f.server)
        .one(&f.state.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.token_hash, before.token_hash);
    assert_eq!(
        serverbee_server::entity::agent_authority_event::Entity::find()
            .filter(serverbee_server::entity::agent_authority_event::Column::ServerId.eq(&f.server))
            .all(&f.state.db)
            .await
            .unwrap(),
        history_before
    );
    common::send_system_info(
        &mut f.sink,
        &mut f._reader,
        "after-rollback",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    let (mut replacement, mut reader) = common::connect_agent(&f.base, &f.token).await;
    common::send_system_info(
        &mut replacement,
        &mut reader,
        "unchanged-token-after-rollback",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    assert!(jobs(&f.state).await.is_empty());
    assert!(f.external.lock().await.is_empty());
    f.state
        .db
        .execute_unprepared("DROP TRIGGER fail_revocation_history")
        .await
        .unwrap();
    let (status, body) = f.revoke_response().await;
    assert_eq!(
        status, 200,
        "repaired storage response: {body}; inspect database error above if failing"
    );
    assert!(!f.state.agent_manager.is_online(&f.server));
    f.connected = false;
    let _ = replacement.close().await;
    f.restart().await;
    f.clear_fault().await;
    let recovered = f.event(false, false).await;
    assert_eq!(recovered.id, original.id);
    assert_eq!(recovered.created_at, original.created_at);
    assert!(jobs(&f.state).await.is_empty());
    assert!(
        block_list::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.external.lock().await.is_empty());
}

async fn finished_originals(f: &OnceOnlyFixture, usernames: &[&str]) -> Vec<security_event::Model> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let rows = security_event::Entity::find()
                .all(&f.state.db)
                .await
                .unwrap();
            let originals: Option<Vec<_>> = usernames
                .iter()
                .map(|name| {
                    rows.iter()
                        .find(|event| {
                            event.username.as_deref() == Some(*name)
                                && event.admission_payload.is_none()
                                && event.push_intent.is_none()
                        })
                        .cloned()
                })
                .collect();
            if let Some(originals) = originals {
                break originals;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("originals finish automatic ordered admission")
}

async fn assert_original_broadcasts(
    browser: &mut tokio::sync::broadcast::Receiver<serverbee_common::protocol::BrowserMessage>,
    expected: &[security_event::Model],
) {
    tokio::time::timeout(Duration::from_secs(8), async {
        let mut seen = std::collections::HashSet::new();
        while seen.len() < expected.len() {
            if let serverbee_common::protocol::BrowserMessage::SecurityEvent(event) =
                browser.recv().await.unwrap()
            {
                let original = expected
                    .iter()
                    .find(|original| original.id == event.event_id)
                    .expect("no new detection or UUID");
                assert!(seen.insert(event.event_id));
                assert_eq!(event.event.username, original.username);
                assert_eq!(event.event.source_ip, original.source_ip);
                assert_eq!(event.event.ended_at, original.ended_at.timestamp());
            }
        }
    })
    .await
    .unwrap();
}

async fn assert_ordered_notifications(
    f: &mut OnceOnlyFixture,
    a: &security_event::Model,
    b: &security_event::Model,
    unrelated: &security_event::Model,
    original_c: &security_event::Model,
) {
    let rows = finished_originals(
        f,
        &[
            "first-original",
            "second-original",
            "unrelated-original",
            "third-original",
        ],
    )
    .await;
    for original in [a, b, unrelated, original_c] {
        let recovered = rows.iter().find(|event| event.id == original.id).unwrap();
        let mut expected = original.clone();
        expected.admission_payload = None;
        expected.push_intent = None;
        expected.maintenance_at_admission = Some(false);
        assert_eq!(
            recovered, &expected,
            "original UUID/time/payload facts never change"
        );
    }
    assert_eq!(
        security_event::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .len(),
        4
    );
    let states = alert_state::Entity::find().all(&f.state.db).await.unwrap();
    assert_eq!(states.len(), 4, "two rules with two distinct source keys");
    for state in &states {
        if state.event_key == a.source_ip {
            assert_eq!(state.count, 3);
            assert_eq!(state.first_triggered_at, a.created_at);
            assert_eq!(state.last_notified_at, original_c.created_at);
            assert_eq!(state.updated_at, original_c.created_at);
        } else {
            assert_eq!(state.event_key, unrelated.source_ip);
            assert_eq!(state.count, 1);
            assert_eq!(state.last_notified_at, unrelated.created_at);
        }
    }
    let queued = jobs(&f.state).await;
    assert_eq!(
        queued.len(),
        6,
        "A and B notify, C is suppressed, unrelated key progresses"
    );
    for original in [a, b, unrelated] {
        let recipients: Vec<_> = queued
            .iter()
            .filter(|job| job.event_id == original.id)
            .collect();
        assert_eq!(recipients.len(), 2);
        assert_ne!(recipients[0].installation_id, recipients[1].installation_id);
        for job in recipients {
            assert_eq!(job.category, "security");
            assert_eq!(job.created_at, original.created_at.timestamp());
            assert_eq!(job.expires_at, original.created_at.timestamp() + 1800);
        }
    }
    assert!(queued.iter().all(|job| job.event_id != original_c.id));
    f.external_count(3).await;
    let external = f.external.lock().await;
    assert_eq!(
        external
            .iter()
            .filter(|body| body.contains(&a.source_ip))
            .count(),
        2
    );
    assert_eq!(
        external
            .iter()
            .filter(|body| body.contains(&unrelated.source_ip))
            .count(),
        1
    );
    drop(external);
    let blocks = block_list::Entity::find().all(&f.state.db).await.unwrap();
    assert_eq!(blocks.len(), 2);
    assert!(
        blocks
            .iter()
            .any(|block| block.origin_event_id.as_deref() == Some(a.id.as_str()))
    );
    assert!(
        blocks
            .iter()
            .any(|block| block.origin_event_id.as_deref() == Some(unrelated.id.as_str()))
    );
    f.relay
        .status
        .store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(f.state.clone());
    wait_jobs(&f.state, 6, "accepted").await;
    worker.abort();
    let _ = worker.await;
    assert_eq!(f.relay.requests().await.len(), 6);
    for request in f.relay.requests().await {
        let content = decrypted_delivery(&request);
        let original = [a, b, unrelated]
            .into_iter()
            .find(|event| content["event_id"] == event.id)
            .unwrap();
        assert_eq!(content["created_at"], original.created_at.timestamp());
        assert_eq!(
            content["expires_at"],
            original.created_at.timestamp() + 1800
        );
        assert_eq!(content["server_id"], f.server);
    }
    // Verify persistent monotonic cooldown after another database reopen; a
    // cache-only guard or replayed effects cannot satisfy these assertions.
    f.restart().await;
    let reopened = alert_state::Entity::find().all(&f.state.db).await.unwrap();
    assert_eq!(reopened.len(), states.len());
    for original in &states {
        assert_eq!(
            reopened.iter().find(|state| state.id == original.id),
            Some(original)
        );
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(jobs(&f.state).await.len(), 6);
    assert_eq!(f.external.lock().await.len(), 3);
}

async fn ordered_multi_event_recovery(memory_raw_fault: bool) {
    let mut f = OnceOnlyFixture::with_window("port_scan", 4).await;
    f.assert_production_wal().await;
    let mut browser = f.state.browser_tx.subscribe();
    f.state.db.execute_unprepared(
        "CREATE TRIGGER fail_first_admission BEFORE UPDATE ON security_event
         WHEN OLD.username='first-original' AND OLD.admission_payload IS NOT NULL AND NEW.admission_payload IS NULL
         BEGIN SELECT RAISE(ABORT, 'fixture oldest admission failure'); END"
    ).await.unwrap();
    if memory_raw_fault {
        f.state
            .db
            .execute_unprepared(
                "CREATE TRIGGER fail_first_raw BEFORE INSERT ON security_event
             WHEN NEW.username='first-original'
             BEGIN SELECT RAISE(ABORT, 'fixture oldest raw failure'); END",
            )
            .await
            .unwrap();
    } else {
        f.state
            .db
            .execute_unprepared("ALTER TABLE maintenance RENAME TO unavailable_maintenance")
            .await
            .unwrap();
    }
    f.send_once(&detection(
        "port_scan",
        "203.0.113.42",
        10,
        "first-original",
        true,
    ))
    .await;
    common::send_system_info(
        &mut f.sink,
        &mut f._reader,
        "first-original-retained",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    let accepted_before = Utc::now();
    let mut a = if memory_raw_fault {
        None
    } else {
        Some(f.original_for("first-original").await)
    };
    if let Some(original) = &a {
        assert!(original.admission_payload.is_some());
        assert_eq!(original.maintenance_at_admission, None);
        assert_original_broadcasts(&mut browser, std::slice::from_ref(original)).await;
        f.stop_recovery().await;
        f.state
            .db
            .execute_unprepared("ALTER TABLE unavailable_maintenance RENAME TO maintenance")
            .await
            .unwrap();
    }
    // Use a real short rule window and original wall-clock reception times.
    // No timestamp rewrite, private service call or retransmitted detection.
    tokio::time::sleep(Duration::from_millis(4200)).await;
    f.send_once(&detection(
        "port_scan",
        "203.0.113.42",
        10,
        "second-original",
        true,
    ))
    .await;
    f.send_once(&detection(
        "port_scan",
        "203.0.113.43",
        10,
        "unrelated-original",
        true,
    ))
    .await;
    common::send_system_info(
        &mut f.sink,
        &mut f._reader,
        "later-originals-retained",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    if f.recovery.is_none() {
        f.recovery = Some(f.state.security_service.start_recovery());
    }
    let b = f.original_for("second-original").await;
    let unrelated = finished_originals(&f, &["unrelated-original"])
        .await
        .remove(0);
    wait_jobs(&f.state, 2, "pending").await;
    f.external_count(1).await;
    let pending_b = f.original_for("second-original").await;
    assert!(
        pending_b.admission_payload.is_some(),
        "newer same-key original cannot overtake the head"
    );
    assert_eq!(pending_b.created_at, b.created_at);
    assert!(
        jobs(&f.state)
            .await
            .iter()
            .all(|job| job.event_id == unrelated.id)
    );
    assert!(
        alert_state::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .iter()
            .all(|state| state.event_key == unrelated.source_ip)
    );
    if memory_raw_fault {
        assert!(
            security_event::Entity::find()
                .filter(security_event::Column::Username.eq("first-original"))
                .all(&f.state.db)
                .await
                .unwrap()
                .is_empty()
        );
        f.state
            .db
            .execute_unprepared("DROP TRIGGER fail_first_raw")
            .await
            .unwrap();
        a = Some(f.original_for("first-original").await);
    }
    let a = a.unwrap();
    assert!(a.created_at <= accepted_before);
    assert!(b.created_at - a.created_at >= ChronoDuration::seconds(4));
    // Once A is durable, its own admission fault keeps B blocked while the
    // unrelated key already has both recipients and its existing effects.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        f.original_for("second-original")
            .await
            .admission_payload
            .is_some()
    );
    assert_eq!(jobs(&f.state).await.len(), 2);
    f.restart().await;
    f.assert_production_wal().await;
    f.state
        .db
        .execute_unprepared("DROP TRIGGER fail_first_admission")
        .await
        .unwrap();
    let completed = finished_originals(
        &f,
        &["first-original", "second-original", "unrelated-original"],
    )
    .await;
    wait_jobs(&f.state, 6, "pending").await;
    if memory_raw_fault {
        assert_original_broadcasts(&mut browser, &completed).await;
    } else {
        let later: Vec<_> = completed
            .iter()
            .filter(|event| event.id != a.id)
            .cloned()
            .collect();
        assert_original_broadcasts(&mut browser, &later).await;
    }
    // Reconnect the unchanged valid token to the
    // restarted production state before sending one new original C.
    let (mut sink, mut reader) = common::connect_agent(&f.base, &f.token).await;
    common::send_system_info(
        &mut sink,
        &mut reader,
        "third-original-after-reopen",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    f.sink = sink;
    f._reader = reader;
    f.connected = true;
    let mut after_restart_browser = f.state.browser_tx.subscribe();
    f.send_once(&detection(
        "port_scan",
        "203.0.113.42",
        10,
        "third-original",
        true,
    ))
    .await;
    let c = finished_originals(&f, &["third-original"]).await.remove(0);
    assert!(
        c.created_at - b.created_at < ChronoDuration::seconds(4),
        "C is genuinely inside B's rule cooldown"
    );
    assert_original_broadcasts(&mut after_restart_browser, std::slice::from_ref(&c)).await;
    assert_ordered_notifications(&mut f, &a, &b, &unrelated, &c).await;
}

#[tokio::test]
async fn security_ordered_missing_snapshot_recovers_multiple_originals_across_wal_restart() {
    ordered_multi_event_recovery(false).await;
}

#[tokio::test]
async fn security_ordered_memory_and_durable_faults_preserve_keys_without_blocking_other_recipients()
 {
    ordered_multi_event_recovery(true).await;
}

#[tokio::test]
async fn security_wal_reenrollment_transfers_authority_and_cancels_old_pending_original() {
    let mut f = OnceOnlyFixture::new("port_scan").await;
    f.assert_production_wal().await;
    f.fault("intent").await;
    f.send_once(&detection("port_scan", "203.0.113.42", 10, "alice", true))
        .await;
    let original = f.event(true, false).await;
    let before = serverbee_server::entity::server::Entity::find_by_id(&f.server)
        .one(&f.state.db)
        .await
        .unwrap()
        .unwrap();
    let offer = f
        .client
        .post(format!(
            "{}/api/servers/{}/agent-authority/re-enrollment",
            f.base, f.server
        ))
        .json(&json!({"mode":"graceful","ttl_secs":3600}))
        .send()
        .await
        .unwrap();
    assert_eq!(offer.status(), 200);
    let offer: Value = offer.json().await.unwrap();
    assert!(
        f.state.agent_manager.is_online(&f.server),
        "graceful offer keeps current authority"
    );
    assert_eq!(
        serverbee_server::entity::server::Entity::find_by_id(&f.server)
            .one(&f.state.db)
            .await
            .unwrap()
            .unwrap()
            .token_hash,
        before.token_hash
    );
    let replacement_token = format!("replacement-token-{}", uuid::Uuid::new_v4());
    let claim = reqwest::Client::new()
        .post(format!("{}/api/agent/register", f.base))
        .bearer_auth(offer["data"]["enrollment"]["code"].as_str().unwrap())
        .json(&json!({"proposed_run_token":replacement_token}))
        .send()
        .await
        .unwrap();
    assert_eq!(claim.status(), 200);
    assert_eq!(
        claim.json::<Value>().await.unwrap()["data"]["server_id"],
        f.server
    );
    assert!(!f.state.agent_manager.is_online(&f.server));
    let old_url = format!(
        "{}/api/agent/ws?token={}",
        f.base.replace("http://", "ws://"),
        f.token
    );
    let old = tokio_tungstenite::connect_async(old_url).await;
    assert!(
        matches!(old, Err(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status().as_u16()==401)
    );
    f.connected = false;
    f.token = replacement_token;
    f.restart().await;
    let (mut sink, mut reader) = common::connect_agent(&f.base, &f.token).await;
    common::send_system_info(
        &mut sink,
        &mut reader,
        "replacement-authority-after-reopen",
        Some(CAP_SECURITY_EVENTS),
    )
    .await;
    f.sink = sink;
    f._reader = reader;
    f.connected = true;
    f.clear_fault().await;
    let recovered = f.event(false, false).await;
    assert_eq!(recovered.id, original.id);
    assert_eq!(recovered.created_at, original.created_at);
    assert!(
        f.state.agent_manager.is_online(&f.server),
        "old recovery cannot fence replacement authority"
    );
    assert!(jobs(&f.state).await.is_empty());
    assert!(
        block_list::Entity::find()
            .all(&f.state.db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(f.external.lock().await.is_empty());
}

#[tokio::test]
async fn security_ordered_recovery_rotates_blocked_intent_pages_without_starving_ready_installations()
 {
    let mut f = OnceOnlyFixture::new("ssh_login").await;
    f.state.db.execute_unprepared(
        "CREATE TRIGGER fail_blocked_intents BEFORE INSERT ON mobile_push_outbox
         WHEN EXISTS (SELECT 1 FROM security_event WHERE id=NEW.event_id AND username LIKE 'blocked-%')
         BEGIN SELECT RAISE(ABORT, 'fixture blocked intent backlog'); END"
    ).await.unwrap();
    for i in 1..=65 {
        f.send_once(&detection(
            "ssh_login",
            &format!("203.0.113.{i}"),
            0,
            &format!("blocked-{i}"),
            true,
        ))
        .await;
    }
    f.send_once(&detection(
        "ssh_login",
        "203.0.113.100",
        0,
        "ready-last",
        true,
    ))
    .await;
    let ready = f.original_for("ready-last").await;
    wait_jobs(&f.state, 2, "pending").await;
    assert!(
        jobs(&f.state)
            .await
            .iter()
            .all(|job| job.event_id == ready.id),
        "later ready recipients bypass over 64 failed intents"
    );
    f.external_count(66).await;
    let originals = security_event::Entity::find()
        .all(&f.state.db)
        .await
        .unwrap();
    assert_eq!(originals.len(), 66);
    assert!(
        originals
            .iter()
            .all(|event| event.admission_payload.is_none())
    );
    assert_eq!(
        originals
            .iter()
            .filter(|event| event.push_intent.is_some())
            .count(),
        65
    );
    f.state
        .db
        .execute_unprepared("DROP TRIGGER fail_blocked_intents")
        .await
        .unwrap();
    wait_jobs(&f.state, 132, "pending").await;
    let queued = jobs(&f.state).await;
    for original in &originals {
        let jobs: Vec<_> = queued
            .iter()
            .filter(|job| job.event_id == original.id)
            .collect();
        assert_eq!(jobs.len(), 2);
        for job in jobs {
            assert_eq!(job.created_at, original.created_at.timestamp());
            assert_eq!(job.expires_at, original.created_at.timestamp() + 1800);
        }
    }
    assert_eq!(
        f.external.lock().await.len(),
        66,
        "push recovery does not replay rule effects"
    );
}
