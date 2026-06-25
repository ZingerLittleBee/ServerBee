//! Integration tests for the file-manager control-plane HTTP handlers in
//! `crates/server/src/router/api/file.rs`.
//!
//! Each test stands up the shared test server, registers a mock agent over the
//! agent WebSocket, completes the SystemInfo handshake (advertising CAP_FILE so
//! the server lets the request through), then spawns a responder task that
//! answers the single forwarded file-op request by echoing its correlation id.
//! The HTTP request blocks until the responder replies, so the responder is
//! always spawned BEFORE issuing the request and awaited after asserting.
//!
//! Authorization and gating outcomes (member 403, unauthenticated 401,
//! capability-denied 403, agent-offline 404) need no responder because the
//! handler short-circuits before any agent round-trip.

mod common;

use common::{
    AgentReader, AgentSink, connect_agent, http_client, login_admin, login_as_new_user,
    recv_agent_text, register_agent, send_system_info, start_test_server,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use serverbee_common::constants::{CAP_DEFAULT, CAP_FILE};
use tokio_tungstenite::tungstenite;

// ── Shared helpers ─────────────────────────────────────────────────────────

/// Connect a mock agent, drain the welcome frame, and complete the SystemInfo
/// handshake advertising the given capability bitmask. Returns the split socket
/// halves so the caller can drive a responder.
async fn connect_agent_with_caps(
    base_url: &str,
    token: &str,
    caps: u32,
) -> (AgentSink, AgentReader) {
    let (mut sink, mut reader) = connect_agent(base_url, token).await;
    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome", "first agent frame should be welcome");
    send_system_info(&mut sink, &mut reader, "file-system-info", Some(caps)).await;
    (sink, reader)
}

/// First-connect pushes a default agent (which reports CAP_FIREWALL_BLOCK) must
/// tolerate and ignore. These are unrelated to the file flow under test.
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

/// Spawn a responder that waits for a single forwarded request whose `type`
/// equals `request_type`, then sends `build_response(msg_id)` back to the
/// server. Ignorable first-connect pushes are skipped; any other command is a
/// hard failure so protocol drift surfaces loudly.
fn spawn_single_response<F>(
    mut sink: AgentSink,
    mut reader: AgentReader,
    request_type: &'static str,
    build_response: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(&str) -> serde_json::Value + Send + 'static,
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
// POST /api/files/{server_id}/list  →  ServerMessage::FileList
//                                   ←  AgentMessage::FileListResult
// ===========================================================================

#[tokio::test]
async fn test_list_files_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_list", |msg_id| {
        json!({
            "type": "file_list_result",
            "msg_id": msg_id,
            "path": "/etc",
            "entries": [{
                "name": "hosts",
                "path": "/etc/hosts",
                "file_type": "File",
                "size": 200,
                "modified": 1_700_000_000_i64,
                "permissions": "rw-r--r--",
                "owner": "root",
                "group": "root"
            }],
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/list"))
        .json(&json!({ "path": "/etc" }))
        .send()
        .await
        .expect("list request failed");
    assert_eq!(resp.status(), 200, "list should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse list response");
    assert_eq!(body["data"]["entries"][0]["name"], "hosts");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test]
async fn test_list_files_agent_error_maps_to_bad_request() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Agent returns an error string (not the capability-disabled sentinel) →
    // the handler maps it to 400 Bad Request.
    let agent_task = spawn_single_response(sink, reader, "file_list", |msg_id| {
        json!({
            "type": "file_list_result",
            "msg_id": msg_id,
            "path": "/root",
            "entries": [],
            "error": "Permission denied"
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/list"))
        .json(&json!({ "path": "/root" }))
        .send()
        .await
        .expect("list request failed");
    assert_eq!(resp.status(), 400, "agent error should surface as 400");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test]
async fn test_list_files_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, _token) = register_agent(&client, &base_url).await;

    // Fresh client with no session cookie.
    let anon = http_client();
    let resp = anon
        .post(format!("{base_url}/api/files/{server_id}/list"))
        .json(&json!({ "path": "/" }))
        .send()
        .await
        .expect("list request failed");
    assert_eq!(resp.status(), 401, "unauthenticated list should be 401");
}

#[tokio::test]
async fn test_list_files_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let member = login_as_new_user(&admin, &base_url, "member-list", "member").await;
    let resp = member
        .post(format!("{base_url}/api/files/{server_id}/list"))
        .json(&json!({ "path": "/" }))
        .send()
        .await
        .expect("list request failed");
    assert_eq!(resp.status(), 403, "member list should be 403 (admin-only)");
}

#[tokio::test]
async fn test_list_files_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    // Agent connects WITHOUT CAP_FILE → capability check fails before any
    // round-trip; no responder is needed.
    let (mut sink, _reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT).await;

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/list"))
        .json(&json!({ "path": "/" }))
        .send()
        .await
        .expect("list request failed");
    assert_eq!(resp.status(), 403, "missing CAP_FILE should be 403");
    let body: serde_json::Value = resp.json().await.expect("parse error response");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("agent_capability_disabled"),
        "capability denial reason should be preserved, got {body:?}"
    );

    let _ = sink.close().await;
}

// ===========================================================================
// POST /api/files/{server_id}/stat  →  ServerMessage::FileStat
//                                   ←  AgentMessage::FileStatResult
// ===========================================================================

#[tokio::test]
async fn test_stat_file_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_stat", |msg_id| {
        json!({
            "type": "file_stat_result",
            "msg_id": msg_id,
            "entry": {
                "name": "hosts",
                "path": "/etc/hosts",
                "file_type": "File",
                "size": 200,
                "modified": 1_700_000_000_i64,
                "permissions": "rw-r--r--",
                "owner": "root",
                "group": "root"
            },
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/stat"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("stat request failed");
    assert_eq!(resp.status(), 200, "stat should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse stat response");
    assert_eq!(body["data"]["entry"]["path"], "/etc/hosts");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test]
async fn test_stat_file_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, _reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT).await;

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/stat"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("stat request failed");
    assert_eq!(resp.status(), 403, "missing CAP_FILE should be 403");

    let _ = sink.close().await;
}

// ===========================================================================
// POST /api/files/{server_id}/read  →  ServerMessage::FileRead
//                                   ←  AgentMessage::FileReadResult
// ===========================================================================

#[tokio::test]
async fn test_read_file_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_read", |msg_id| {
        json!({
            "type": "file_read_result",
            "msg_id": msg_id,
            "content": "127.0.0.1 localhost\n",
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/read"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("read request failed");
    assert_eq!(resp.status(), 200, "read should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse read response");
    assert_eq!(body["data"]["content"], "127.0.0.1 localhost\n");

    agent_task.await.expect("agent responder failed");
}

#[tokio::test]
async fn test_read_file_capability_disabled_sentinel_maps_to_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // The agent advertises CAP_FILE (so the handler forwards) but reports the
    // exact "File capability disabled" sentinel, which `agent_error` maps to 403.
    let agent_task = spawn_single_response(sink, reader, "file_read", |msg_id| {
        json!({
            "type": "file_read_result",
            "msg_id": msg_id,
            "content": null,
            "error": "File capability disabled"
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/read"))
        .json(&json!({ "path": "/etc/shadow" }))
        .send()
        .await
        .expect("read request failed");
    assert_eq!(
        resp.status(),
        403,
        "agent 'File capability disabled' should map to 403"
    );

    agent_task.await.expect("agent responder failed");
}

#[tokio::test]
async fn test_read_file_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let member = login_as_new_user(&admin, &base_url, "member-read", "member").await;
    let resp = member
        .post(format!("{base_url}/api/files/{server_id}/read"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("read request failed");
    assert_eq!(resp.status(), 403, "member read should be 403 (admin-only)");
}

// ===========================================================================
// POST /api/files/{server_id}/write  →  ServerMessage::FileWrite
//                                    ←  AgentMessage::FileOpResult
// ===========================================================================

#[tokio::test]
async fn test_write_file_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_write", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": true,
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/write"))
        .json(&json!({ "path": "/tmp/test.txt", "content": "hello" }))
        .send()
        .await
        .expect("write request failed");
    assert_eq!(resp.status(), 200, "write should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse write response");
    assert_eq!(body["data"]["success"], true);

    agent_task.await.expect("agent responder failed");
}

#[tokio::test]
async fn test_write_file_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let anon = http_client();
    let resp = anon
        .post(format!("{base_url}/api/files/{server_id}/write"))
        .json(&json!({ "path": "/tmp/x", "content": "y" }))
        .send()
        .await
        .expect("write request failed");
    assert_eq!(resp.status(), 401, "unauthenticated write should be 401");
}

// ===========================================================================
// POST /api/files/{server_id}/mkdir  →  ServerMessage::FileMkdir
//                                    ←  AgentMessage::FileOpResult
// ===========================================================================

#[tokio::test]
async fn test_mkdir_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_mkdir", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": true,
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/mkdir"))
        .json(&json!({ "path": "/tmp/newdir" }))
        .send()
        .await
        .expect("mkdir request failed");
    assert_eq!(resp.status(), 200, "mkdir should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse mkdir response");
    assert_eq!(body["data"]["success"], true);

    agent_task.await.expect("agent responder failed");
}

#[tokio::test]
async fn test_mkdir_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let member = login_as_new_user(&admin, &base_url, "member-mkdir", "member").await;
    let resp = member
        .post(format!("{base_url}/api/files/{server_id}/mkdir"))
        .json(&json!({ "path": "/tmp/newdir" }))
        .send()
        .await
        .expect("mkdir request failed");
    assert_eq!(resp.status(), 403, "member mkdir should be 403 (admin-only)");
}

// ===========================================================================
// POST /api/files/{server_id}/delete  →  ServerMessage::FileDelete
//                                     ←  AgentMessage::FileOpResult
// ===========================================================================

#[tokio::test]
async fn test_delete_file_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_delete", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": true,
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/delete"))
        .json(&json!({ "path": "/tmp/test.txt", "recursive": false }))
        .send()
        .await
        .expect("delete request failed");
    assert_eq!(resp.status(), 200, "delete should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse delete response");
    assert_eq!(body["data"]["success"], true);

    agent_task.await.expect("agent responder failed");
}

/// A registered-but-never-connected server has its persisted capability mirror
/// at `CAP_DEFAULT`, which lacks CAP_FILE, so the capability gate (which runs
/// before the online check) fires with 403.
#[tokio::test]
async fn test_delete_file_agent_never_connected_is_403_capability() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Register but never open the agent WebSocket.
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/delete"))
        .json(&json!({ "path": "/tmp/x", "recursive": false }))
        .send()
        .await
        .expect("delete request failed");
    assert_eq!(
        resp.status(),
        403,
        "never-connected server has mirror CAP_DEFAULT (no CAP_FILE) → 403"
    );
}

// ===========================================================================
// POST /api/files/{server_id}/move  →  ServerMessage::FileMove
//                                   ←  AgentMessage::FileOpResult
// ===========================================================================

#[tokio::test]
async fn test_move_file_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (sink, reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    let agent_task = spawn_single_response(sink, reader, "file_move", |msg_id| {
        json!({
            "type": "file_op_result",
            "msg_id": msg_id,
            "success": true,
            "error": null
        })
    });

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/move"))
        .json(&json!({ "from": "/tmp/a.txt", "to": "/tmp/b.txt" }))
        .send()
        .await
        .expect("move request failed");
    assert_eq!(resp.status(), 200, "move should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse move response");
    assert_eq!(body["data"]["success"], true);

    agent_task.await.expect("agent responder failed");
}

/// After an agent that advertised CAP_FILE disconnects, the in-memory cap entry
/// is cleared but the persisted mirror keeps CAP_FILE — so the capability gate
/// passes and the *online* check is what fails, yielding 404 "Server offline".
#[tokio::test]
async fn test_move_file_agent_offline_after_disconnect_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    // Connect with CAP_FILE so the persisted mirror is updated to include it,
    // then drop the socket so the server marks the agent offline.
    let (mut sink, mut reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;
    sink.close().await.expect("close agent socket");
    // Drain until the stream ends so the server processes the disconnect.
    while reader.next().await.is_some() {}
    // Give the server a moment to run connection cleanup.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/move"))
        .json(&json!({ "from": "/tmp/a", "to": "/tmp/b" }))
        .send()
        .await
        .expect("move request failed");
    assert_eq!(
        resp.status(),
        404,
        "mirror keeps CAP_FILE so the offline check fires → 404"
    );
}

// ===========================================================================
// POST /api/files/{server_id}/download  →  ServerMessage::FileDownloadStart
//   (fire-and-forget; the handler returns immediately with a transfer id)
// ===========================================================================

#[tokio::test]
async fn test_start_download_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // The download endpoint does not wait for an agent reply — it only sends
    // FileDownloadStart and returns a pending transfer. Spawn a task that just
    // observes the forwarded command so the test confirms it was sent.
    let observer = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some("file_download_start") => {
                    assert_eq!(msg["path"], "/var/log/syslog");
                    return msg["transfer_id"].as_str().map(str::to_string);
                }
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
        .expect("download request failed");
    assert_eq!(resp.status(), 200, "download start should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse download response");
    assert_eq!(body["data"]["status"], "pending");
    assert!(
        body["data"]["transfer_id"].as_str().is_some(),
        "download should return a transfer id"
    );

    observer.await.expect("download observer failed");
    let _ = sink.close().await;
}

#[tokio::test]
async fn test_start_download_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, _reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT).await;

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/download"))
        .json(&json!({ "path": "/etc/shadow" }))
        .send()
        .await
        .expect("download request failed");
    assert_eq!(resp.status(), 403, "missing CAP_FILE should be 403");

    let _ = sink.close().await;
}

#[tokio::test]
async fn test_start_download_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let member = login_as_new_user(&admin, &base_url, "member-download", "member").await;
    let resp = member
        .post(format!("{base_url}/api/files/{server_id}/download"))
        .json(&json!({ "path": "/etc/hosts" }))
        .send()
        .await
        .expect("download request failed");
    assert_eq!(
        resp.status(),
        403,
        "member download should be 403 (admin-only)"
    );
}

// ===========================================================================
// POST /api/files/{server_id}/upload  →  multipart, then FileUploadStart →
//   FileUploadChunk(s) → FileUploadEnd, with per-step FileUploadAck /
//   FileUploadComplete handshake keyed by upload-ack-/upload-complete-.
// ===========================================================================

#[tokio::test]
async fn test_upload_file_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) =
        connect_agent_with_caps(&base_url, &token, CAP_DEFAULT | CAP_FILE).await;

    // Drive the full upload handshake: ack the start, ack each chunk, then
    // confirm completion. Correlation here is by transfer_id, not msg_id.
    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut reader).await;
            match msg["type"].as_str() {
                Some("file_upload_start") => {
                    let transfer_id = msg["transfer_id"].as_str().expect("transfer_id").to_string();
                    // Accept the upload.
                    sink.send(tungstenite::Message::Text(
                        json!({
                            "type": "file_upload_ack",
                            "transfer_id": transfer_id,
                            "offset": 0
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .expect("send upload_ack (start)");
                }
                Some("file_upload_chunk") => {
                    let transfer_id = msg["transfer_id"].as_str().expect("transfer_id").to_string();
                    let offset = msg["offset"].as_u64().expect("offset");
                    // The handler waits on the next ack keyed by transfer_id;
                    // echo back the advanced offset.
                    use base64::Engine;
                    let decoded = base64::engine::general_purpose::STANDARD
                        .decode(msg["data"].as_str().expect("data"))
                        .expect("decode chunk");
                    sink.send(tungstenite::Message::Text(
                        json!({
                            "type": "file_upload_ack",
                            "transfer_id": transfer_id,
                            "offset": offset + decoded.len() as u64
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .expect("send upload_ack (chunk)");
                }
                Some("file_upload_end") => {
                    let transfer_id = msg["transfer_id"].as_str().expect("transfer_id").to_string();
                    sink.send(tungstenite::Message::Text(
                        json!({
                            "type": "file_upload_complete",
                            "transfer_id": transfer_id
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .expect("send upload_complete");
                    return;
                }
                other if is_ignorable_push(other) => {}
                Some(other) => panic!("unexpected agent command: {other}"),
                None => {}
            }
        }
    });

    let form = reqwest::multipart::Form::new()
        .text("path", "/tmp/uploaded.txt")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"hello upload".to_vec()).file_name("uploaded.txt"),
        );

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/upload"))
        .multipart(form)
        .send()
        .await
        .expect("upload request failed");
    assert_eq!(resp.status(), 200, "upload should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse upload response");
    assert_eq!(body["data"]["success"], true);

    agent_task.await.expect("agent responder failed");
}

#[tokio::test]
async fn test_upload_file_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, _reader) = connect_agent_with_caps(&base_url, &token, CAP_DEFAULT).await;

    let form = reqwest::multipart::Form::new()
        .text("path", "/tmp/x")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"data".to_vec()).file_name("x"),
        );

    let resp = client
        .post(format!("{base_url}/api/files/{server_id}/upload"))
        .multipart(form)
        .send()
        .await
        .expect("upload request failed");
    assert_eq!(resp.status(), 403, "missing CAP_FILE should be 403");

    let _ = sink.close().await;
}

#[tokio::test]
async fn test_upload_file_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let anon = http_client();
    let form = reqwest::multipart::Form::new().text("path", "/tmp/x").part(
        "file",
        reqwest::multipart::Part::bytes(b"data".to_vec()).file_name("x"),
    );
    let resp = anon
        .post(format!("{base_url}/api/files/{server_id}/upload"))
        .multipart(form)
        .send()
        .await
        .expect("upload request failed");
    assert_eq!(resp.status(), 401, "unauthenticated upload should be 401");
}

// ===========================================================================
// Transfer-session endpoints (no agent forwarding on the request path):
//   GET    /api/files/transfers
//   GET    /api/files/download/{transfer_id}
//   DELETE /api/files/transfers/{transfer_id}
// ===========================================================================

#[tokio::test]
async fn test_list_transfers_empty_for_admin() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{base_url}/api/files/transfers"))
        .send()
        .await
        .expect("list transfers request failed");
    assert_eq!(resp.status(), 200, "list transfers should succeed");
    let body: serde_json::Value = resp.json().await.expect("parse transfers response");
    assert_eq!(
        body["data"]["transfers"].as_array().map(Vec::len),
        Some(0),
        "no transfers should exist yet"
    );
}

#[tokio::test]
async fn test_list_transfers_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let member = login_as_new_user(&admin, &base_url, "member-transfers", "member").await;
    let resp = member
        .get(format!("{base_url}/api/files/transfers"))
        .send()
        .await
        .expect("list transfers request failed");
    assert_eq!(
        resp.status(),
        403,
        "member list transfers should be 403 (admin-only)"
    );
}

#[tokio::test]
async fn test_list_transfers_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let anon = http_client();
    let resp = anon
        .get(format!("{base_url}/api/files/transfers"))
        .send()
        .await
        .expect("list transfers request failed");
    assert_eq!(
        resp.status(),
        401,
        "unauthenticated list transfers should be 401"
    );
}

#[tokio::test]
async fn test_download_unknown_transfer_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{base_url}/api/files/download/does-not-exist"))
        .send()
        .await
        .expect("download-by-id request failed");
    assert_eq!(resp.status(), 404, "unknown transfer download should be 404");
}

#[tokio::test]
async fn test_cancel_unknown_transfer_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .delete(format!("{base_url}/api/files/transfers/does-not-exist"))
        .send()
        .await
        .expect("cancel transfer request failed");
    assert_eq!(resp.status(), 404, "unknown transfer cancel should be 404");
}

#[tokio::test]
async fn test_cancel_transfer_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let member = login_as_new_user(&admin, &base_url, "member-cancel", "member").await;
    let resp = member
        .delete(format!("{base_url}/api/files/transfers/does-not-exist"))
        .send()
        .await
        .expect("cancel transfer request failed");
    assert_eq!(
        resp.status(),
        403,
        "member cancel transfer should be 403 (admin-only)"
    );
}
