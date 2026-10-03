//! Real HTTP, scheduler, Agent WS, subscriptions and migrated SQLite. Only
//! Agent execution and Apple/Relay HTTP are external boundary substitutes.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::SinkExt;
use sea_orm::QueryOrder;
use serde_json::{Value, json};
use serverbee_common::constants::{CAP_DEFAULT, CAP_EXEC};
use serverbee_server::entity::{
    mobile_push_outbox as outbox, mobile_push_registration as registration, task_run,
};
use tokio_tungstenite::tungstenite::Message;

#[path = "../common/mod.rs"]
mod common;

async fn subscribe(client: &reqwest::Client, base: &str, access: &str, device: &str) {
    queued_register(client, base, access, device).await;
    let mut prefs = intent(false, true);
    prefs["task_failure"] = json!(true);
    let response = preferences_http(client, base, access, 2, prefs).await;
    assert_eq!(response.status(), 200);
    let data: Value = response.json().await.unwrap();
    assert_eq!(data["data"]["tasks_allowed"], true);
    assert_eq!(data["data"]["preferences"]["task_failure"], true);
}

async fn server(client: &reqwest::Client, base: &str, access: &str) -> (String, String) {
    let response = client
        .post(format!("{base}/api/servers"))
        .bearer_auth(access)
        .json(&json!({"onboarding_request_id":uuid::Uuid::new_v4(),"name":"Task target"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let data: Value = response.json().await.unwrap();
    let id = data["data"]["server_id"].as_str().unwrap().to_string();
    let code = data["data"]["enrollment"]["code"].as_str().unwrap();
    let token = format!("fixture-{}", uuid::Uuid::new_v4());
    assert_eq!(
        client
            .post(format!("{base}/api/agent/register"))
            .bearer_auth(code)
            .json(&json!({"proposed_run_token":token}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    (id, token)
}

async fn agent(
    client: &reqwest::Client,
    base: &str,
    access: &str,
) -> (String, common::AgentSink, common::AgentReader) {
    let (id, token) = server(client, base, access).await;
    let (mut sink, mut reader) = common::connect_agent(base, &token).await;
    assert_eq!(
        common::recv_agent_text(&mut reader).await["type"],
        "welcome"
    );
    common::send_system_info(
        &mut sink,
        &mut reader,
        "task-handshake",
        Some(CAP_DEFAULT | CAP_EXEC),
    )
    .await;
    (id, sink, reader)
}

async fn exec(reader: &mut common::AgentReader) -> Value {
    loop {
        let message = common::recv_agent_text(reader).await;
        if message["type"] == "exec" {
            return message;
        }
    }
}
async fn reply(sink: &mut common::AgentSink, message: &Value, code: i32, output: &str) {
    sink.send(Message::Text(
        json!({"type":"task_result","msg_id":uuid::Uuid::new_v4(),
            "task_id":message["task_id"],"exit_code":code,"output":output
        })
        .to_string()
        .into(),
    ))
    .await
    .unwrap();
}
async fn task(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    targets: &[String],
    retries: i32,
    cron: &str,
) -> String {
    let response = client.post(format!("{base}/api/tasks")).bearer_auth(access)
        .json(&json!({"command":"sensitive-command","name":"sensitive-name","server_ids":targets,
            "task_type":"scheduled","cron_expression":cron,"timeout":1,"retry_count":retries,"retry_interval":1}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let data: Value = response.json().await.unwrap();
    data["data"]["id"].as_str().unwrap().to_string()
}
async fn run(client: &reqwest::Client, base: &str, access: &str, id: &str) {
    assert_eq!(
        client
            .post(format!("{base}/api/tasks/{id}/run"))
            .bearer_auth(access)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}
async fn completed(state: &AppState, id: &str) -> task_run::Model {
    for _ in 0..800 {
        if let Some(run) = task_run::Entity::find()
            .filter(task_run::Column::TaskId.eq(id))
            .filter(task_run::Column::Status.eq("completed"))
            .one(&state.db)
            .await
            .unwrap()
        {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("run did not reach completed state");
}
async fn jobs(state: &AppState) -> Vec<outbox::Model> {
    outbox::Entity::find()
        .order_by_asc(outbox::Column::InstallationId)
        .all(&state.db)
        .await
        .unwrap()
}
async fn plaintext(state: &AppState, job: &outbox::Model) -> Value {
    use ring::aead;
    let row = registration::Entity::find_by_id(&job.installation_id)
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let envelope: Value = serde_json::from_str(job.envelope.as_deref().unwrap()).unwrap();
    let secret = STANDARD.decode(row.content_key.unwrap()).unwrap();
    let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &secret).unwrap());
    let nonce: [u8; 12] = STANDARD
        .decode(envelope["nonce"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let mut bytes = STANDARD
        .decode(envelope["ciphertext"].as_str().unwrap())
        .unwrap();
    let aad = format!(
        "ServerBee.Push.v1|{}|{}",
        envelope["key_id"].as_str().unwrap(),
        envelope["identity"].as_str().unwrap()
    );
    let decrypted = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad.as_bytes()),
            &mut bytes,
        )
        .unwrap();
    serde_json::from_slice(decrypted).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_summary_waits_for_all_targets_selects_initiator_and_filters_run() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let creator = login_http(&client, &base, "admin", "creator").await;
    let creator_access = creator["access_token"].as_str().unwrap();
    assert_eq!(
        client
            .post(format!("{base}/api/users"))
            .bearer_auth(creator_access)
            .json(&json!({"username":"initiator","password":"testpass","role":"admin"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let owner = login_http(&client, &base, "initiator", "owner-a").await;
    let second = login_http(&client, &base, "initiator", "owner-b").await;
    let owner_access = owner["access_token"].as_str().unwrap();
    subscribe(&client, &base, creator_access, "creator-grant").await;
    subscribe(&client, &base, owner_access, "device-a").await;
    subscribe(
        &client,
        &base,
        second["access_token"].as_str().unwrap(),
        "device-b",
    )
    .await;
    let (denied, _) = server(&client, &base, creator_access).await;
    let (offline, mut offline_sink, offline_reader) = agent(&client, &base, creator_access).await;
    offline_sink.close().await.unwrap();
    drop(offline_reader);
    for _ in 0..100 {
        if !state.agent_manager.is_online(&offline) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!state.agent_manager.is_online(&offline));
    let (failure, mut fail_sink, mut fail_reader) = agent(&client, &base, creator_access).await;
    let (timeout, mut timeout_sink, mut timeout_reader) =
        agent(&client, &base, creator_access).await;
    let (held, mut held_sink, mut held_reader) = agent(&client, &base, creator_access).await;
    let id = task(
        &client,
        &base,
        creator_access,
        &[denied, offline, failure, timeout, held],
        0,
        "0 0 0 * * *",
    )
    .await;
    run(&client, &base, owner_access, &id).await;
    let failing = exec(&mut fail_reader).await;
    reply(&mut fail_sink, &failing, 7, "sensitive-output").await;
    let timing = exec(&mut timeout_reader).await;
    reply(&mut timeout_sink, &timing, -1, "Command timed out after 1s").await;
    let holding = exec(&mut held_reader).await;
    assert!(
        jobs(&state).await.is_empty(),
        "partial results must stay silent"
    );
    reply(&mut held_sink, &holding, 0, "sensitive-success").await;
    let first = completed(&state, &id).await;
    assert_eq!(first.owner_id, owner["user"]["id"].as_str().unwrap());
    let queued = jobs(&state).await;
    assert_eq!(queued.len(), 2);
    for job in &queued {
        assert_eq!(job.event_id, first.run_id);
        assert_eq!(job.expires_at - job.created_at, 1800);
        let content = plaintext(&state, job).await;
        assert_eq!(content["kind"], "task_failure");
        assert_eq!(
            content["task_run"],
            json!({"task_id":id,"run_id":first.run_id,"total":5,"failed":1,"timed_out":1,"offline":1,"denied":1})
        );
        for secret in [
            "sensitive-command",
            "sensitive-output",
            "sensitive-name",
            "sensitive-success",
        ] {
            assert!(!content.to_string().contains(secret));
            assert!(!job.envelope.as_deref().unwrap().contains(secret));
        }
    }
    // Independent devices receive one logical delivery each via the production worker.
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    for _ in 0..200 {
        if jobs(&state).await.iter().all(|j| j.outcome == "accepted") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    worker.abort();
    let _ = worker.await;
    assert_eq!(relay.requests().await.len(), 2);
    assert!(
        jobs(&state)
            .await
            .iter()
            .all(|j| j.outcome == "accepted" && j.attempts == 1)
    );
    // A later all-denied run must not contaminate the exact-run result route.
    let new_denied = server(&client, &base, creator_access).await.0;
    assert_eq!(
        client
            .put(format!("{base}/api/tasks/{id}"))
            .bearer_auth(creator_access)
            .json(&json!({"server_ids":[new_denied]}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    run(&client, &base, creator_access, &id).await;
    for _ in 0..100 {
        if jobs(&state).await.len() == 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let response = client
        .get(format!(
            "{base}/api/tasks/{id}/results?run_id={}",
            first.run_id
        ))
        .bearer_auth(owner_access)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|r| r["run_id"] == first.run_id));
    assert_eq!(
        client
            .get(format!(
                "{base}/api/tasks/{id}/results?run_id={}",
                uuid::Uuid::new_v4()
            ))
            .bearer_auth(owner_access)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

#[tokio::test]
async fn task_preferences_require_current_admin_and_reject_failed_save() {
    let (base, _, _tmp) = setup_http().await;
    let client = reqwest::Client::new();
    let member = login_http(&client, &base, "member", "member-task").await;
    let access = member["access_token"].as_str().unwrap();
    for category in ["task_failure", "task_success"] {
        let mut prefs = intent(false, true);
        prefs[category] = json!(true);
        assert_eq!(
            preferences_http(&client, &base, access, 0, prefs)
                .await
                .status(),
            403
        );
        let confirmed = status_http(&client, &base, access).await;
        assert_eq!(confirmed["revision"], 0);
        assert_eq!(confirmed["tasks_allowed"], false);
        assert_eq!(confirmed["preferences"][category], false);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_runs_select_creator_and_only_exhausted_final_attempt_notifies() {
    use chrono::Timelike;
    for final_code in [0, 9] {
        let (base, state, _tmp, _) = queued_setup().await;
        let client = reqwest::Client::new();
        let creator = login_http(&client, &base, "admin", "automatic-owner").await;
        let access = creator["access_token"].as_str().unwrap();
        subscribe(&client, &base, access, "device-a").await;
        AuthService::create_user(&state.db, "other-admin", "testpass", "admin")
            .await
            .unwrap();
        let other = login_http(&client, &base, "other-admin", "other-admin").await;
        subscribe(
            &client,
            &base,
            other["access_token"].as_str().unwrap(),
            "other-device",
        )
        .await;
        let (target, mut sink, mut reader) = agent(&client, &base, access).await;
        let trigger = Utc::now() + ChronoDuration::seconds(3);
        let cron = format!(
            "{} {} {} * * *",
            trigger.second(),
            trigger.minute(),
            trigger.hour()
        );
        let id = task(&client, &base, access, &[target], 1, &cron).await;
        serverbee_server::service::task_scheduler::restore_and_start(state.clone()).await;
        let first = exec(&mut reader).await;
        reply(&mut sink, &first, 1, "sensitive-retry").await;
        let last = exec(&mut reader).await;
        assert!(
            jobs(&state).await.is_empty(),
            "intermediate failure stays silent"
        );
        reply(&mut sink, &last, final_code, "sensitive-final").await;
        let run = completed(&state, &id).await;
        assert!(!run.manual);
        assert_eq!(run.owner_id, creator["user"]["id"].as_str().unwrap());
        let queued = jobs(&state).await;
        assert_eq!(queued.len(), usize::from(final_code != 0));
        if let Some(job) = queued.first() {
            let content = plaintext(&state, job).await;
            assert_eq!(content["task_run"]["failed"], 1);
            assert_eq!(content["task_run"]["total"], 1);
            assert!(!content.to_string().contains("sensitive-"));
        }
        let rows: Value = client
            .get(format!(
                "{base}/api/tasks/{id}/results?run_id={}",
                run.run_id
            ))
            .bearer_auth(access)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(rows["data"].as_array().unwrap().len(), 2);
        assert_eq!(
            client
                .put(format!("{base}/api/tasks/{id}"))
                .bearer_auth(access)
                .json(&json!({"enabled":false}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduler_deadline_counts_timeout_and_cancellation_never_completes() {
    for cancel in [false, true] {
        let (base, state, _tmp, _) = queued_setup().await;
        let client = reqwest::Client::new();
        let owner = login_http(&client, &base, "admin", "timeout-owner").await;
        let access = owner["access_token"].as_str().unwrap();
        subscribe(&client, &base, access, "device-a").await;
        let (target, _sink, mut reader) = agent(&client, &base, access).await;
        let id = task(&client, &base, access, &[target], 0, "0 0 0 * * *").await;
        run(&client, &base, access, &id).await;
        let _waiting = exec(&mut reader).await;
        if cancel {
            assert_eq!(
                client
                    .put(format!("{base}/api/tasks/{id}"))
                    .bearer_auth(access)
                    .json(&json!({"enabled":false}))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            );
            let run = task_run::Entity::find()
                .filter(task_run::Column::TaskId.eq(&id))
                .one(&state.db)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(run.status, "incomplete");
            assert!(jobs(&state).await.is_empty());
        } else {
            let _ = completed(&state, &id).await;
            let queued = jobs(&state).await;
            assert_eq!(queued.len(), 1);
            assert_eq!(
                plaintext(&state, &queued[0]).await["task_run"]["timed_out"],
                1
            );
        }
    }
}

#[tokio::test]
async fn all_denied_summaries_revalidate_access_subscription_owner_and_deleted_targets() {
    for mutation in ["role", "preference", "delete", "owner"] {
        let (base, state, _tmp, relay) = queued_setup().await;
        let client = reqwest::Client::new();
        let owner = login_http(&client, &base, "admin", "revoked-owner").await;
        let access = owner["access_token"].as_str().unwrap();
        subscribe(&client, &base, access, "device-a").await;
        let denied = server(&client, &base, access).await.0;
        let id = task(&client, &base, access, &[denied], 0, "0 0 0 * * *").await;
        run(&client, &base, access, &id).await;
        let run = completed(&state, &id).await;
        let queued = jobs(&state).await;
        assert_eq!(queued.len(), 1);
        assert_eq!(plaintext(&state, &queued[0]).await["task_run"]["denied"], 1);
        match mutation {
            "role" => {
                // An independent administrator changes the current persisted role.
                AuthService::create_user(&state.db, "operator", "testpass", "admin")
                    .await
                    .unwrap();
                let operator = login_http(&client, &base, "operator", "operator").await;
                assert_eq!(
                    client
                        .put(format!(
                            "{base}/api/users/{}",
                            owner["user"]["id"].as_str().unwrap()
                        ))
                        .bearer_auth(operator["access_token"].as_str().unwrap())
                        .json(&json!({"role":"member"}))
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    200
                );
                assert_eq!(
                    client
                        .get(format!(
                            "{base}/api/tasks/{id}/results?run_id={}",
                            run.run_id
                        ))
                        .bearer_auth(access)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    403
                );
            }
            "preference" => {
                assert_eq!(
                    preferences_http(&client, &base, access, 3, intent(false, true))
                        .await
                        .status(),
                    200
                );
            }
            "delete" => {
                assert_eq!(
                    client
                        .delete(format!("{base}/api/tasks/{id}"))
                        .bearer_auth(access)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    200
                );
                assert_eq!(
                    client
                        .get(format!(
                            "{base}/api/tasks/{id}/results?run_id={}",
                            run.run_id
                        ))
                        .bearer_auth(access)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    404
                );
            }
            "owner" => {
                // Inject corrupt queued ownership, never fake policy or persistence.
                // The dispatch boundary must refuse a job selected for another actor.
                let member = login_http(&client, &base, "member", "wrong-owner").await;
                let mut job: outbox::ActiveModel = queued[0].clone().into();
                job.user_id = Set(member["user"]["id"].as_str().unwrap().into());
                job.update(&state.db).await.unwrap();
            }
            _ => unreachable!(),
        }
        relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
        let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
        for _ in 0..200 {
            if jobs(&state).await[0].outcome == "permanent" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        worker.abort();
        let _ = worker.await;
        let refused = jobs(&state).await;
        assert_eq!(refused[0].reason, "Ineligible", "{mutation}");
        assert!(relay.requests().await.is_empty(), "{mutation}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_revocation_before_completion_prevents_queue_admission() {
    let (base, state, _tmp, _) = queued_setup().await;
    let client = reqwest::Client::new();
    let owner = login_http(&client, &base, "admin", "admission-owner").await;
    let access = owner["access_token"].as_str().unwrap();
    subscribe(&client, &base, access, "device-a").await;
    AuthService::create_user(&state.db, "operator", "testpass", "admin")
        .await
        .unwrap();
    let operator = login_http(&client, &base, "operator", "admission-operator").await;
    let (target, mut sink, mut reader) = agent(&client, &base, access).await;
    let id = task(&client, &base, access, &[target], 0, "0 0 0 * * *").await;
    run(&client, &base, access, &id).await;
    let waiting = exec(&mut reader).await;
    assert_eq!(
        client
            .put(format!(
                "{base}/api/users/{}",
                owner["user"]["id"].as_str().unwrap()
            ))
            .bearer_auth(operator["access_token"].as_str().unwrap())
            .json(&json!({"role":"member"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    reply(&mut sink, &waiting, 1, "private").await;
    let _ = completed(&state, &id).await;
    assert!(jobs(&state).await.is_empty());
}
