//! Supplementary integration tests for `crates/server/src/router/api/file.rs`.
//!
//! `agent_file_ops.rs` already covers each endpoint's happy path plus the
//! short-circuit auth/capability/offline arms. This file targets the branches
//! it leaves uncovered:
//!   * server-not-found (`validate_file_access` → `get_server` 404),
//!   * agent-error → BadRequest mapping for stat/read/write/delete/mkdir/move,
//!   * the `success:false` / empty-content / missing-entry response shapes,
//!   * the "unexpected agent response type" → 500 Internal arm,
//!   * delete with `recursive:true`,
//!   * the full download transfer lifecycle (start → ready → stream the bytes),
//!   * download-by-id not-ready (400) and cross-user ownership (404),
//!   * `cancel_transfer` success (owned, in-progress download → agent cancel),
//!   * `list_transfers` returning an active transfer,
//!   * upload validation arms: missing `path` field, empty `file` field, and
//!     the agent rejecting the upload start with FileUploadError.
//!
//! Tests whose HTTP handler blocks on an agent reply while a spawned responder
//! must make progress use the multi-thread runtime so the responder is never
//! starved (see the CRITICAL RUNTIME RULE in the harness docs).

mod common;

use common::{
    AgentReader, AgentSink, connect_agent, http_client, login_admin, login_as_new_user,
    recv_agent_text, register_agent, send_system_info, start_test_server,
};
use futures_util::SinkExt;
use serde_json::{Value, json};
use serverbee_common::constants::{CAP_DEFAULT, CAP_FILE};
use tokio_tungstenite::tungstenite;

// ── Shared helpers ─────────────────────────────────────────────────────────

/// Connect a mock agent, drain the welcome frame, and complete the SystemInfo
/// handshake advertising the given capability bitmask.
async fn connect_agent_with_caps(
    base_url: &str,
    token: &str,
    caps: u32,
) -> (AgentSink, AgentReader) {
    let (mut sink, mut reader) = connect_agent(base_url, token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome", "first agent frame should be welcome");
    send_system_info(&mut sink, &mut reader, "file-extra-system-info", Some(caps)).await;
    (sink, reader)
}

/// First-connect pushes the server emits to a default agent that the file flow
/// has no interest in. Ignore them so protocol drift on the real commands still
/// surfaces loudly.
fn is_ignorable_push(msg_type: Option<&str>) -> bool {
    matches!(
        msg_type,
        Some("ping_tasks_sync")
            | Some("network_probe_sync")
            | Some("blocklist_reset")
            | Some("blocklist_sync")
            | Some("blocklist_add")
            | Some("blocklist_remove")
    )
}

/// Spawn a responder that waits for one forwarded request whose `type` equals
/// `request_type`, then sends `build_response(msg_id)` back to the server.
fn spawn_single_response<F>(
    mut sink: AgentSink,
    mut reader: AgentReader,
    request_type: &'static str,
    build_response: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(&str) -> Value + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some(t) if t == request_type => {
                    let msg_id = msg["msg_id"]
                        .as_str()
                        .unwrap_or_else(|| panic!("{request_type} missing msg_id"));
                    let response = build_response(msg_id);
                    sink.send(tungstenite::Message::Text(response.to_string().into()))
                        .await
                        .expect("send file-op response");
                    return;
                }
                other if is_ignorable_push(other) => {}
                Some(other) => panic!("unexpected agent command: {other}"),
                None => {}
            }
        }
    })
}

// ===========================================================================
// validate_file_access: server not found (get_server → NotFound 404)
// This arm precedes the capability/online checks and is hit by an unknown id.
// ===========================================================================

#[tokio::test]
async fn test_list_files_unknown_server_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{base_url}/api/files/no-such-server/list"))
        .json(&json!({ "path": "/" }))
        .send()
        .await
        .expect("list request failed");
    assert_eq!(resp.status(), 404, "unknown server should be 404");
}

#[tokio::test]
async fn test_write_file_unknown_server_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{base_url}/api/files/no-such-server/write"))
        .json(&json!({ "path": "/tmp/x", "content": "y" }))
        .send()
        .await
        .expect("write request failed");
    assert_eq!(resp.status(), 404, "unknown server should be 404");
}

#[tokio::test]
async fn test_read_file_unknown_server_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{base_url}/api/files/no-such-server/read"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("read request failed");
    assert_eq!(resp.status(), 404, "unknown server should be 404");
}

// ===========================================================================
// stat: agent error → BadRequest, and entry:None+no-error → Internal 500
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stat_file_agent_error_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_stat", |msg_id| {
        json!({
            "type": "file_stat_result",
            "msg_id": msg_id,
            "entry": null,
            "error": "No such file or directory"
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/stat"))
        .json(&json!({ "path": "/nope" }))
        .send()
        .await
        .expect("stat request failed");
    assert_eq!(resp.status(), 400, "agent stat error should be 400");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stat_file_missing_entry_no_error_is_500() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // No error string but also no entry → handler's `ok_or(Internal)` fires.
    let agent_task = spawn_single_response(sink, reader, "file_stat", |msg_id| {
        json!({
            "type": "file_stat_result",
            "msg_id": msg_id,
            "entry": null,
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/stat"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("stat request failed");
    assert_eq!(resp.status(), 500, "missing entry with no error should be 500");

    agent_task.await.expect("agent responder failed");
}

// ===========================================================================
// read: agent non-sentinel error → 400; null content → empty content 200
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_read_file_agent_error_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_read", |msg_id| {
        json!({
            "type": "file_read_result",
            "msg_id": msg_id,
            "content": null,
            "error": "Is a directory"
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/read"))
        .json(&json!({ "path": "/etc" }))
        .send()
        .await
        .expect("read request failed");
    assert_eq!(resp.status(), 400, "agent read error should be 400");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_read_file_null_content_returns_empty_string() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Empty file: no error, content omitted → handler uses `unwrap_or_default()`.
    let agent_task = spawn_single_response(sink, reader, "file_read", |msg_id| {
        json!({
            "type": "file_read_result",
            "msg_id": msg_id,
            "content": null,
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/read"))
        .json(&json!({ "path": "/tmp/empty" }))
        .send()
        .await
        .expect("read request failed");
    assert_eq!(resp.status(), 200, "empty read should succeed");
    let body: Value = resp.json().await.expect("parse read response");
    assert_eq!(body["data"]["content"], "", "null content becomes empty string");

    agent_task.await.expect("agent responder failed");
}

// ===========================================================================
// read: unexpected agent response type → 500 Internal
// (handler only matches FileReadResult; any other variant falls into Ok(Ok(_)))
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_read_file_unexpected_response_type_is_500() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // The agent echoes the read msg_id but with a file_op_result body — the read
    // handler only accepts FileReadResult, so this hits the Internal arm.
    let agent_task = spawn_single_response(sink, reader, "file_read", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": true,
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/read"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("read request failed");
    assert_eq!(resp.status(), 500, "wrong response variant should be 500");

    agent_task.await.expect("agent responder failed");
}

// ===========================================================================
// write: agent error → 400, and success:false echoed back
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_write_file_agent_error_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_write", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": false,
            "error": "Permission denied"
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/write"))
        .json(&json!({ "path": "/etc/readonly", "content": "x" }))
        .send()
        .await
        .expect("write request failed");
    assert_eq!(resp.status(), 400, "agent write error should be 400");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_write_file_success_false_is_200() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // No error but success:false — handler returns 200 with success=false body.
    let agent_task = spawn_single_response(sink, reader, "file_write", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": false,
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/write"))
        .json(&json!({ "path": "/tmp/test.txt", "content": "hello" }))
        .send()
        .await
        .expect("write request failed");
    assert_eq!(resp.status(), 200, "write with no error should be 200");
    let body: Value = resp.json().await.expect("parse write response");
    assert_eq!(body["data"]["success"], false, "success flag echoed through");

    agent_task.await.expect("agent responder failed");
}

// ===========================================================================
// delete: recursive=true path + agent error → 403 sentinel mapping
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_dir_recursive_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Assert the recursive flag is forwarded as true.
    let agent_task = tokio::spawn(async move {
        let mut reader = reader;
        let mut sink = sink;
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some("file_delete") => {
                    assert_eq!(msg["recursive"], true, "recursive flag must reach the agent");
                    let msg_id = msg["msg_id"].as_str().expect("msg_id");
                    sink.send(tungstenite::Message::Text(
                        json!({
                            "type": "file_op_result",
                            "msg_id": msg_id,
                            "success": true,
                            "error": null
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .expect("send file_op_result");
                    return;
                }
                other if is_ignorable_push(other) => {}
                Some(other) => panic!("unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/delete"))
        .json(&json!({ "path": "/tmp/dir", "recursive": true }))
        .send()
        .await
        .expect("delete request failed");
    assert_eq!(resp.status(), 200, "recursive delete should succeed");
    let body: Value = resp.json().await.expect("parse delete response");
    assert_eq!(body["data"]["success"], true);

    agent_task.await.expect("agent responder failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_delete_file_agent_capability_sentinel_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Agent advertised CAP_FILE so the request forwards, but replies with the
    // exact sentinel → agent_error maps to 403 even past the gate.
    let agent_task = spawn_single_response(sink, reader, "file_delete", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": false,
            "error": "File capability disabled"
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/delete"))
        .json(&json!({ "path": "/tmp/x", "recursive": false }))
        .send()
        .await
        .expect("delete request failed");
    assert_eq!(resp.status(), 403, "sentinel error should map to 403");

    agent_task.await.expect("agent responder failed");
}

// ===========================================================================
// mkdir + move: agent error → 400
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_mkdir_agent_error_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_mkdir", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": false,
            "error": "File exists"
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/mkdir"))
        .json(&json!({ "path": "/tmp/exists" }))
        .send()
        .await
        .expect("mkdir request failed");
    assert_eq!(resp.status(), 400, "agent mkdir error should be 400");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_move_agent_error_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_move", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": false,
            "error": "Cross-device link"
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/move"))
        .json(&json!({ "from": "/tmp/a", "to": "/mnt/b" }))
        .send()
        .await
        .expect("move request failed");
    assert_eq!(resp.status(), 400, "agent move error should be 400");

    agent_task.await.expect("agent responder failed");
}

// ===========================================================================
// Full download lifecycle: start → ready → stream bytes via GET by id.
// Then re-fetch the same id → 404 (the stream endpoint removes the transfer).
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_download_full_lifecycle_streams_bytes() {
    use base64::Engine;

    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let file_bytes = b"download payload contents".to_vec();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&file_bytes);
    let size = file_bytes.len() as u64;

    // Drive the agent side of the download: on FileDownloadStart, send Ready,
    // one chunk, then End so the server marks the transfer "ready".
    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some("file_download_start") => {
                    let transfer_id =
                        msg["transfer_id"].as_str().expect("transfer_id").to_string();
                    for frame in [
                        json!({
                            "type": "file_download_ready",
                            "transfer_id": transfer_id,
                            "size": size
                        }),
                        json!({
                            "type": "file_download_chunk",
                            "transfer_id": transfer_id,
                            "offset": 0,
                            "data": encoded
                        }),
                        json!({
                            "type": "file_download_end",
                            "transfer_id": transfer_id
                        }),
                    ] {
                        sink.send(tungstenite::Message::Text(frame.to_string().into()))
                            .await
                            .expect("send download frame");
                    }
                    return;
                }
                other if is_ignorable_push(other) => {}
                Some(other) => panic!("unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/download"))
        .json(&json!({ "path": "/var/log/payload.bin" }))
        .send()
        .await
        .expect("download start failed");
    assert_eq!(resp.status(), 200, "download start should succeed");
    let body: Value = resp.json().await.expect("parse download response");
    let transfer_id = body["data"]["transfer_id"]
        .as_str()
        .expect("transfer_id")
        .to_string();

    agent_task.await.expect("download agent responder failed");

    // Poll the stream endpoint until the transfer flips to "ready". Before that
    // it returns 400 ("Transfer not ready"); once ready it streams 200.
    let mut streamed = None;
    for _ in 0..40 {
        let dl = client
            .get(format!("{base_url}/api/files/download/{transfer_id}"))
            .send()
            .await
            .expect("download-by-id request failed");
        if dl.status() == 200 {
            let disposition = dl
                .headers()
                .get(reqwest::header::CONTENT_DISPOSITION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            assert!(
                disposition.contains("payload.bin"),
                "filename should derive from the remote path, got {disposition}"
            );
            streamed = Some(dl.bytes().await.expect("read download bytes"));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let streamed = streamed.expect("transfer never became ready");
    assert_eq!(
        streamed.as_ref(),
        file_bytes.as_slice(),
        "streamed bytes should match what the agent sent"
    );

    // The stream endpoint removes the transfer, so a second GET is 404.
    let again = client
        .get(format!("{base_url}/api/files/download/{transfer_id}"))
        .send()
        .await
        .expect("second download-by-id request failed");
    assert_eq!(again.status(), 404, "consumed transfer should be 404");
}

// ===========================================================================
// download-by-id: a freshly-started transfer is "pending", not "ready" → 400.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_download_by_id_not_ready_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Observe the FileDownloadStart but never reply, so the transfer stays
    // "pending" — the stream endpoint must reject it with 400.
    let observer = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some("file_download_start") => return,
                other if is_ignorable_push(other) => {}
                Some(other) => panic!("unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/download"))
        .json(&json!({ "path": "/var/log/syslog" }))
        .send()
        .await
        .expect("download start failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse download response");
    let transfer_id = body["data"]["transfer_id"].as_str().expect("transfer_id");

    let dl = client
        .get(format!("{base_url}/api/files/download/{transfer_id}"))
        .send()
        .await
        .expect("download-by-id request failed");
    assert_eq!(dl.status(), 400, "pending transfer should be 400 not-ready");

    observer.await.expect("observer failed");
    let _ = sink.close().await;
}

// ===========================================================================
// download-by-id + cancel ownership: another admin user cannot touch a
// transfer they do not own → 404 (ownership is masked as not-found).
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_download_by_id_cross_user_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let owner = http_client();
    login_admin(&owner, &base_url).await;

    let (server_id, token) = register_agent(&owner, &base_url).await;
    let (mut sink, mut reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let observer = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some("file_download_start") => return,
                other if is_ignorable_push(other) => {}
                Some(other) => panic!("unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let resp = owner
        .post(format!("{base_url}/api/files/{server_id}/download"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("download start failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse download response");
    let transfer_id = body["data"]["transfer_id"].as_str().expect("transfer_id");

    // A different admin must not see another user's transfer.
    let other_admin = login_as_new_user(&owner, &base_url, "file-other-admin", "admin").await;
    let dl = other_admin
        .get(format!("{base_url}/api/files/download/{transfer_id}"))
        .send()
        .await
        .expect("cross-user download request failed");
    assert_eq!(dl.status(), 404, "non-owner download should be masked as 404");

    // Same masking on cancel.
    let cancel = other_admin
        .delete(format!("{base_url}/api/files/transfers/{transfer_id}"))
        .send()
        .await
        .expect("cross-user cancel request failed");
    assert_eq!(cancel.status(), 404, "non-owner cancel should be masked as 404");

    observer.await.expect("observer failed");
    let _ = sink.close().await;
}

// ===========================================================================
// list_transfers returns an in-flight transfer, and the owner can cancel it
// (in-progress download → server forwards FileDownloadCancel to the agent).
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_list_then_cancel_owned_download_transfer() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Channel reports when the agent has observed a FileDownloadCancel command.
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    let agent_task = tokio::spawn(async move {
        let mut cancel_tx = Some(cancel_tx);
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                // Leave the transfer "pending"; do not send Ready so it stays
                // active and shows up in list_transfers.
                Some("file_download_start") => {}
                Some("file_download_cancel") => {
                    if let Some(tx) = cancel_tx.take() {
                        let _ = tx.send(());
                    }
                    return;
                }
                other if is_ignorable_push(other) => {}
                Some(other) => panic!("unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/download"))
        .json(&json!({ "path": "/var/log/big.log" }))
        .send()
        .await
        .expect("download start failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse download response");
    let transfer_id = body["data"]["transfer_id"]
        .as_str()
        .expect("transfer_id")
        .to_string();

    // The active transfer should now appear in the owner's transfer list.
    let listed = client
        .get(format!("{base_url}/api/files/transfers"))
        .send()
        .await
        .expect("list transfers failed");
    assert_eq!(listed.status(), 200);
    let listed_body: Value = listed.json().await.expect("parse transfers response");
    let transfers = listed_body["data"]["transfers"]
        .as_array()
        .expect("transfers array");
    assert!(
        transfers
            .iter()
            .any(|t| t["transfer_id"] == json!(transfer_id)),
        "active download should be listed, got {transfers:?}"
    );

    // Cancel it: owner + download direction + active status → agent gets the
    // FileDownloadCancel, transfer is removed, handler returns success.
    let cancel = client
        .delete(format!("{base_url}/api/files/transfers/{transfer_id}"))
        .send()
        .await
        .expect("cancel request failed");
    assert_eq!(cancel.status(), 200, "owner cancel should succeed");
    let cancel_body: Value = cancel.json().await.expect("parse cancel response");
    assert_eq!(cancel_body["data"]["success"], true);

    // Confirm the agent actually received the cancel command.
    tokio::time::timeout(std::time::Duration::from_secs(5), cancel_rx)
        .await
        .expect("timed out waiting for FileDownloadCancel")
        .expect("cancel signal dropped");

    // Transfer is gone now → a follow-up cancel is 404.
    let again = client
        .delete(format!("{base_url}/api/files/transfers/{transfer_id}"))
        .send()
        .await
        .expect("second cancel request failed");
    assert_eq!(again.status(), 404, "removed transfer cancel should be 404");

    agent_task.await.expect("agent responder failed");
    let _ = sink.close().await;
}

// ===========================================================================
// upload: multipart validation arms (missing path / empty file) → 400.
// These short-circuit inside the handler after validate_file_access passes.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_upload_missing_path_field_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, _reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Only a file part, no "path" field → handler rejects with 400.
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"data".to_vec()).file_name("x"),
    );

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/upload"))
        .multipart(form)
        .send()
        .await
        .expect("upload request failed");
    assert_eq!(resp.status(), 400, "missing path field should be 400");

    let _ = sink.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_upload_empty_file_field_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, _reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Path present but the file part is empty → file_size==0 → 400.
    let form = reqwest::multipart::Form::new()
        .text("path", "/tmp/empty.txt")
        .part(
            "file",
            reqwest::multipart::Part::bytes(Vec::new()).file_name("empty.txt"),
        );

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/upload"))
        .multipart(form)
        .send()
        .await
        .expect("upload request failed");
    assert_eq!(resp.status(), 400, "empty file field should be 400");

    let _ = sink.close().await;
}

// ===========================================================================
// upload: agent rejects the upload start with FileUploadError → agent_error.
// A non-sentinel error maps to 400 BadRequest.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_upload_agent_rejects_start_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // On FileUploadStart, reply with FileUploadError (keyed by transfer_id via
    // the upload-ack/upload-complete dispatch) so the handler aborts.
    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some("file_upload_start") => {
                    let transfer_id =
                        msg["transfer_id"].as_str().expect("transfer_id").to_string();
                    sink.send(tungstenite::Message::Text(
                        json!({
                            "type": "file_upload_error",
                            "transfer_id": transfer_id,
                            "error": "No space left on device"
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .expect("send upload_error");
                    return;
                }
                other if is_ignorable_push(other) => {}
                Some(other) => panic!("unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let form = reqwest::multipart::Form::new()
        .text("path", "/tmp/rejected.txt")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"some bytes".to_vec()).file_name("rejected.txt"),
        );

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/upload"))
        .multipart(form)
        .send()
        .await
        .expect("upload request failed");
    assert_eq!(resp.status(), 400, "agent-rejected upload should be 400");

    agent_task.await.expect("agent responder failed");
}
