//! Agent replies must be bound to the agent they were requested from.
//!
//! Correlation ids (exec task ids, request msg_ids) are carried in the agent's
//! own frame, so the server has to check that the connected agent is the one
//! the request was sent to. These tests drive two real agents over WebSocket
//! and have one of them answer on behalf of the other.

mod common;

use common::{
    AgentReader, AgentSink, connect_agent, http_client, login_admin, recv_agent_text,
    register_agent, send_system_info, start_test_server,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use serverbee_common::constants::{CAP_DEFAULT, CAP_EXEC};
use tokio_tungstenite::tungstenite;

async fn bring_up_exec_agent(
    client: &reqwest::Client,
    base_url: &str,
    hostname: &str,
) -> (String, AgentSink, AgentReader) {
    let (server_id, token) = register_agent(client, base_url).await;
    let (mut sink, mut reader) = connect_agent(base_url, &token).await;
    assert_eq!(recv_agent_text(&mut reader).await["type"], "welcome");
    send_system_info(
        &mut sink,
        &mut reader,
        hostname,
        Some(CAP_DEFAULT | CAP_EXEC),
    )
    .await;
    while let Ok(Some(Ok(_))) =
        tokio::time::timeout(std::time::Duration::from_millis(250), reader.next()).await
    {}
    (server_id, sink, reader)
}

async fn send_agent_frame(sink: &mut AgentSink, frame: Value) {
    sink.send(tungstenite::Message::Text(frame.to_string().into()))
        .await
        .expect("failed to send agent frame");
}

async fn next_exec_task_id(reader: &mut AgentReader) -> String {
    loop {
        let msg = recv_agent_text(reader).await;
        if msg["type"] == "exec" {
            return msg["task_id"]
                .as_str()
                .expect("exec task_id missing")
                .to_string();
        }
    }
}

/// Regression for GitHub issue #187: agent A answers the scheduled exec that
/// was sent to agent B. The forged reply must be dropped and B's own result
/// must be the one recorded for B.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_agent_cannot_answer_scheduled_exec_of_another_server() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (a_id, mut a_sink, mut a_reader) = bring_up_exec_agent(&client, &base_url, "a").await;
    let (b_id, mut b_sink, mut b_reader) = bring_up_exec_agent(&client, &base_url, "b").await;

    let created: Value = client
        .post(format!("{base_url}/api/tasks"))
        .json(&json!({
            "command": "/usr/local/bin/nightly-backup",
            "server_ids": [a_id, b_id],
            "task_type": "scheduled",
            "name": "nightly backup",
            "cron_expression": "0 0 3 * * *"
        }))
        .send()
        .await
        .expect("create task failed")
        .json()
        .await
        .expect("parse task response");
    let task_id = created["data"]["id"]
        .as_str()
        .expect("task id missing")
        .to_string();

    // A waits until B holds its pending exec, then replies with B's correlation id.
    let (b_got_exec, a_may_forge) = tokio::sync::oneshot::channel::<()>();
    let (a_forged, b_may_reply) = tokio::sync::oneshot::channel::<()>();
    let (a_for_task, b_for_task) = (a_id.clone(), b_id.clone());
    let agent_a = tokio::spawn(async move {
        let own = next_exec_task_id(&mut a_reader).await;
        let _ = a_may_forge.await;
        let forged = own.replacen(&format!(":{a_for_task}:"), &format!(":{b_for_task}:"), 1);
        assert_ne!(forged, own, "correlation id should embed the server id");
        send_agent_frame(
            &mut a_sink,
            json!({"type": "task_result", "msg_id": "a1", "task_id": forged,
                "output": "backup OK (forged by agent A)\n", "exit_code": 0}),
        )
        .await;
        // The Ack proves the forged frame was fully handled before B replies.
        loop {
            let msg = recv_agent_text(&mut a_reader).await;
            if msg["type"] == "ack" && msg["msg_id"] == "a1" {
                break;
            }
        }
        let _ = a_forged.send(());
        send_agent_frame(
            &mut a_sink,
            json!({"type": "task_result", "msg_id": "a2", "task_id": own,
                "output": "A: backup OK\n", "exit_code": 0}),
        )
        .await;
        (a_sink, a_reader)
    });
    let agent_b = tokio::spawn(async move {
        let own = next_exec_task_id(&mut b_reader).await;
        let _ = b_got_exec.send(());
        let _ = b_may_reply.await;
        send_agent_frame(
            &mut b_sink,
            json!({"type": "task_result", "msg_id": "b1", "task_id": own,
                "output": "B: disk full, backup FAILED\n", "exit_code": 1}),
        )
        .await;
        (b_sink, b_reader)
    });

    let run = client
        .post(format!("{base_url}/api/tasks/{task_id}/run"))
        .send()
        .await
        .expect("run task failed");
    assert_eq!(run.status(), 200);
    let _a = agent_a.await.expect("agent A task panicked");
    let _b = agent_b.await.expect("agent B task panicked");

    // Poll until both servers have a result for this run.
    let mut rows = Vec::new();
    for _ in 0..40 {
        let body: Value = client
            .get(format!("{base_url}/api/tasks/{task_id}/results"))
            .send()
            .await
            .expect("GET task results failed")
            .json()
            .await
            .expect("parse task results");
        rows = body["data"].as_array().cloned().unwrap_or_default();
        if rows.len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let row_for = |server_id: &str| {
        rows.iter()
            .find(|row| row["server_id"] == server_id)
            .unwrap_or_else(|| panic!("missing result row for {server_id}: {rows:?}"))
    };
    let b_row = row_for(&b_id);
    assert_eq!(b_row["exit_code"], 1, "B's own failure must be recorded");
    assert_eq!(b_row["output"], "B: disk full, backup FAILED\n");
    let a_row = row_for(&a_id);
    assert_eq!(a_row["output"], "A: backup OK\n");
    assert!(
        rows.iter()
            .all(|row| !row["output"].as_str().unwrap_or("").contains("forged")),
        "forged output must not be persisted: {rows:?}"
    );
}

async fn wait_for_ack(reader: &mut AgentReader, msg_id: &str) {
    loop {
        let msg = recv_agent_text(reader).await;
        if msg["type"] == "ack" && msg["msg_id"] == msg_id {
            return;
        }
    }
}

async fn task_results(client: &reqwest::Client, base_url: &str, task_id: &str) -> Vec<Value> {
    let body: Value = client
        .get(format!("{base_url}/api/tasks/{task_id}/results"))
        .send()
        .await
        .expect("GET task results failed")
        .json()
        .await
        .expect("parse task results");
    body["data"].as_array().cloned().unwrap_or_default()
}

async fn exec_finished_audits_for(
    client: &reqwest::Client,
    base_url: &str,
    task_id: &str,
) -> Vec<Value> {
    let body: Value = client
        .get(format!(
            "{base_url}/api/audit-logs?action=exec_finished&limit=200"
        ))
        .send()
        .await
        .expect("GET audit logs failed")
        .json()
        .await
        .expect("parse audit logs");
    body["data"]["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| serde_json::from_str::<Value>(entry["detail"].as_str()?).ok())
        .filter(|detail| detail["task_id"] == task_id)
        .collect()
}

/// A result with no pending waiter is only accepted for a one-shot task that
/// was sent to the reporting agent. Agent A must not be able to attach a
/// result, a capability-denied row, or an exec_finished audit entry to a task
/// that only targets agent B; B itself still can.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_unsolicited_task_result_for_other_server_is_dropped() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_a_id, mut a_sink, mut a_reader) = bring_up_exec_agent(&client, &base_url, "a").await;
    let (b_id, mut b_sink, mut b_reader) = bring_up_exec_agent(&client, &base_url, "b").await;

    // One-shot task targeting only B, created but never run.
    let created: Value = client
        .post(format!("{base_url}/api/tasks"))
        .json(&json!({
            "command": "uptime",
            "server_ids": [b_id],
            "task_type": "oneshot",
            "name": "b only"
        }))
        .send()
        .await
        .expect("create task failed")
        .json()
        .await
        .expect("parse task response");
    let task_id = created["data"]["id"]
        .as_str()
        .expect("task id missing")
        .to_string();

    send_agent_frame(
        &mut a_sink,
        json!({
            "type": "capability_denied",
            "msg_id": task_id,
            "session_id": null,
            "capability": "exec",
            "reason": "agent_capability_disabled"
        }),
    )
    .await;
    send_agent_frame(
        &mut a_sink,
        json!({"type": "task_result", "msg_id": "a1", "task_id": task_id,
            "output": "forged by agent A\n", "exit_code": 0}),
    )
    .await;
    // Still acked so a well-behaved agent stops retrying.
    wait_for_ack(&mut a_reader, "a1").await;

    assert!(
        task_results(&client, &base_url, &task_id).await.is_empty(),
        "results from a non-target agent must not be persisted"
    );
    assert!(
        exec_finished_audits_for(&client, &base_url, &task_id)
            .await
            .is_empty(),
        "a non-target agent must not produce exec_finished audit entries"
    );

    // Positive control: the targeted agent's result is still accepted.
    send_agent_frame(
        &mut b_sink,
        json!({"type": "task_result", "msg_id": "b1", "task_id": task_id,
            "output": "up 3 days\n", "exit_code": 0}),
    )
    .await;
    wait_for_ack(&mut b_reader, "b1").await;

    let rows = task_results(&client, &base_url, &task_id).await;
    assert_eq!(rows.len(), 1, "only B's result should be stored: {rows:?}");
    assert_eq!(rows[0]["server_id"], b_id.as_str());
    assert_eq!(rows[0]["output"], "up 3 days\n");
    assert_eq!(
        exec_finished_audits_for(&client, &base_url, &task_id)
            .await
            .len(),
        1,
        "B's result should be audited once"
    );
}
