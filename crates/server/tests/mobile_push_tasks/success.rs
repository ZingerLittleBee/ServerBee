//! Success opt-in exercises the same HTTP/Agent/scheduler/outbox seam as failures.
use super::*;

pub(super) async fn preferences(
    client: &reqwest::Client,
    base: &str,
    access: &str,
    failure: bool,
    success: bool,
) {
    let confirmed = status_http(client, base, access).await;
    let mut prefs = confirmed["preferences"].clone();
    prefs["task_failure"] = json!(failure);
    prefs["task_success"] = json!(success);
    let response = preferences_http(
        client,
        base,
        access,
        confirmed["revision"].as_i64().unwrap(),
        prefs.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["preferences"], prefs);
    assert_eq!(body["data"]["tasks_allowed"], true);
}

async fn dispatch(state: &Arc<AppState>, expected: &str) {
    let worker = serverbee_server::service::mobile_push_outbox::start(state.clone());
    for _ in 0..200 {
        let queued = jobs(state).await;
        if !queued.is_empty() && queued.iter().all(|j| j.outcome == expected) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    worker.abort();
    let _ = worker.await;
    let queued = jobs(state).await;
    assert!(!queued.is_empty());
    assert!(queued.iter().all(|j| j.outcome == expected));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_success_silence_and_failed_save_do_not_enable_delivery() {
    let (base, state, _tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let owner = login_http(&client, &base, "admin", "default-owner").await;
    let access = owner["access_token"].as_str().unwrap();
    let initial = status_http(&client, &base, access).await;
    assert_eq!(initial["preferences"]["task_success"], false);
    subscribe(&client, &base, access, "device-a").await;
    let mut desired = intent(false, true);
    desired["task_success"] = json!(true);
    assert_eq!(
        preferences_http(&client, &base, access, 2, desired)
            .await
            .status(),
        409
    );
    assert_eq!(
        status_http(&client, &base, access).await["preferences"]["task_success"],
        false
    );
    let (target, mut sink, mut reader) = agent(&client, &base, access).await;
    let id = task(&client, &base, access, &[target], 0, "0 0 0 * * *").await;
    run(&client, &base, access, &id).await;
    let execution = exec(&mut reader).await;
    reply(&mut sink, &execution, 0, "private-success-output").await;
    let finished = completed(&state, &id).await;
    assert_eq!(
        serde_json::from_str::<Value>(finished.summary_json.as_deref().unwrap()).unwrap()["failed"],
        0
    );
    assert!(jobs(&state).await.is_empty());
    assert!(relay.requests().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_success_waits_for_all_targets_and_only_opted_in_initiator_devices() {
    for last_code in [0, 7] {
        let (base, state, _tmp, relay) = queued_setup().await;
        let client = reqwest::Client::new();
        let creator = login_http(&client, &base, "admin", "creator").await;
        let creator_access = creator["access_token"].as_str().unwrap();
        subscribe(&client, &base, creator_access, "creator-device").await;
        preferences(&client, &base, creator_access, true, true).await;
        AuthService::create_user(&state.db, "initiator", "testpass", "admin")
            .await
            .unwrap();
        let owner = login_http(&client, &base, "initiator", "owner-a").await;
        let second = login_http(&client, &base, "initiator", "owner-b").await;
        let access = owner["access_token"].as_str().unwrap();
        subscribe(&client, &base, access, "device-a").await;
        // A success-only installation proves success never relies on task_failure.
        preferences(&client, &base, access, false, true).await;
        subscribe(
            &client,
            &base,
            second["access_token"].as_str().unwrap(),
            "device-b",
        )
        .await;
        let (first, mut first_sink, mut first_reader) = agent(&client, &base, creator_access).await;
        let (last, mut last_sink, mut last_reader) = agent(&client, &base, creator_access).await;
        let id = task(
            &client,
            &base,
            creator_access,
            &[first, last],
            0,
            "0 0 0 * * *",
        )
        .await;
        run(&client, &base, access, &id).await;
        let a = exec(&mut first_reader).await;
        reply(&mut first_sink, &a, 0, "private-first-output").await;
        let b = exec(&mut last_reader).await;
        assert!(
            jobs(&state).await.is_empty(),
            "An intermediate success is not a final summary"
        );
        reply(&mut last_sink, &b, last_code, "private-last-output").await;
        let finished = completed(&state, &id).await;
        assert_eq!(finished.owner_id, owner["user"]["id"].as_str().unwrap());
        let queued = jobs(&state).await;
        assert_eq!(
            queued.len(),
            1,
            "Exactly one category for the final outcome"
        );
        let job = &queued[0];
        assert_eq!(
            job.installation_id,
            if last_code == 0 { "owner-a" } else { "owner-b" }
        );
        assert_eq!(job.event_id, finished.run_id);
        let content = plaintext(&state, job).await;
        assert_eq!(
            content["kind"],
            if last_code == 0 {
                "task_success"
            } else {
                "task_failure"
            }
        );
        assert_eq!(
            content["task_run"],
            json!({"task_id":id,"run_id":finished.run_id,"total":2,
            "failed":usize::from(last_code != 0),"timed_out":0,"offline":0,"denied":0})
        );
        for secret in [
            "sensitive-command",
            "sensitive-name",
            "private-first-output",
            "private-last-output",
        ] {
            assert!(!content.to_string().contains(secret));
            assert!(!job.envelope.as_deref().unwrap().contains(secret));
        }
        relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
        dispatch(&state, "accepted").await;
        dispatch(&state, "accepted").await;
        assert_eq!(relay.requests().await.len(), 1);
        let receipts = jobs(&state).await;
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].attempts, 1);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_success_revalidates_opt_out_and_current_role() {
    for mutation in ["preference", "role"] {
        let (base, state, _tmp, relay) = queued_setup().await;
        let client = reqwest::Client::new();
        let owner = login_http(&client, &base, "admin", "queued-success").await;
        let access = owner["access_token"].as_str().unwrap();
        subscribe(&client, &base, access, "device-a").await;
        preferences(&client, &base, access, true, true).await;
        let (target, mut sink, mut reader) = agent(&client, &base, access).await;
        let id = task(&client, &base, access, &[target], 0, "0 0 0 * * *").await;
        run(&client, &base, access, &id).await;
        let execution = exec(&mut reader).await;
        reply(&mut sink, &execution, 0, "private-output").await;
        let _ = completed(&state, &id).await;
        let queued = jobs(&state).await;
        assert_eq!(queued.len(), 1);
        assert_eq!(plaintext(&state, &queued[0]).await["kind"], "task_success");
        if mutation == "preference" {
            // Failure remains enabled; opting out of success must still cancel it.
            preferences(&client, &base, access, true, false).await;
        } else {
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
        }
        relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
        dispatch(&state, "permanent").await;
        assert!(relay.requests().await.is_empty());
        let stopped = jobs(&state).await;
        assert_eq!(stopped[0].reason, "Ineligible");
        assert!(stopped[0].envelope.is_none());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn drained_success_recovers_after_sqlite_restart_with_original_identity_and_deadline() {
    let (base, state, tmp, relay) = queued_setup().await;
    let client = reqwest::Client::new();
    let owner = login_http(&client, &base, "admin", "recover-success").await;
    let access = owner["access_token"].as_str().unwrap();
    subscribe(&client, &base, access, "device-a").await;
    preferences(&client, &base, access, false, true).await;
    fail_admission(&state).await;
    let (target, mut sink, mut reader) = agent(&client, &base, access).await;
    let id = task(&client, &base, access, &[target], 0, "0 0 0 * * *").await;
    run(&client, &base, access, &id).await;
    let execution = exec(&mut reader).await;
    reply(&mut sink, &execution, 0, "private-output").await;
    let drained = run_status(&state, &id, "drained").await;
    disable(&client, &base, access, &id).await;
    assert_eq!(
        run_status(&state, &id, "drained").await.run_id,
        drained.run_id
    );
    assert!(jobs(&state).await.is_empty());
    let snapshot = tmp.path().join("success-restart.db");
    state
        .db
        .execute(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "VACUUM INTO ?",
            [snapshot.to_str().unwrap().into()],
        ))
        .await
        .unwrap();
    let mut config = state.config.clone();
    config.database.path = "success-restart.db".into();
    let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", snapshot.display()));
    options.max_connections(5).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let reopened = AppState::new(db, config).await.unwrap();
    serverbee_server::service::task_scheduler::restore_and_start(reopened.clone()).await;
    let saved = run_status(&reopened, &id, "drained").await;
    assert_eq!(saved.summary_json, drained.summary_json);
    assert_eq!(saved.completed_at, drained.completed_at);
    reopened
        .db
        .execute_unprepared("DROP TRIGGER fail_task_outbox")
        .await
        .unwrap();
    relay.status.store(200, std::sync::atomic::Ordering::SeqCst);
    dispatch(&reopened, "accepted").await;
    dispatch(&reopened, "accepted").await;
    let queued = jobs(&reopened).await;
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].event_id, drained.run_id);
    assert_eq!(queued[0].created_at, drained.completed_at.unwrap());
    assert_eq!(queued[0].expires_at, drained.completed_at.unwrap() + 1800);
    assert_eq!(relay.requests().await.len(), 1);
    assert_eq!(completed(&reopened, &id).await.run_id, drained.run_id);
}
