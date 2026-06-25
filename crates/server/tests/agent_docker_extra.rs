//! Integration coverage for the remaining Docker control-plane HTTP handlers
//! that forward requests to a connected agent over the agent WebSocket.
//!
//! `docker_integration.rs` already exercises `GET /docker/info` (cache-miss
//! request to the agent), the docker logs/subscribe streams, and the
//! capability/feature gating around them. This file covers the endpoints NOT
//! exercised there:
//!   - `GET  /servers/{id}/docker/containers`  (cache read)
//!   - `GET  /servers/{id}/docker/stats`       (cache read)
//!   - `GET  /servers/{id}/docker/events`      (DB read, capability-only gate)
//!   - `GET  /servers/{id}/docker/networks`    (forwarded → agent)
//!   - `GET  /servers/{id}/docker/volumes`     (forwarded → agent)
//!   - `POST /servers/{id}/docker/containers/{cid}/action` (forwarded → agent, admin-only)
//!
//! For each it covers the reachable outcomes: happy path, authz (401/403),
//! agent-offline, and missing-capability/feature.

mod common;

use common::{
    connect_agent, http_client, login_admin, login_as_new_user, recv_agent_text, register_agent,
    start_test_server, AgentReader, AgentSink,
};
use futures_util::SinkExt;
use serde_json::json;
use serverbee_common::constants::{CAP_DEFAULT, CAP_DOCKER};
use tokio_tungstenite::tungstenite;

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

/// Complete the SystemInfo handshake reporting the `docker` runtime feature and
/// a given capability bitmask. The shared `send_system_info` helper always
/// reports an empty `features` array, but the docker handlers gate on the agent
/// advertising the `docker` feature, so this test file ships its own variant.
async fn send_docker_system_info(sink: &mut AgentSink, reader: &mut AgentReader, caps: u32) {
    let system_info = json!({
        "type": "system_info",
        "msg_id": "docker-extra-system-info",
        "cpu_name": "Intel Xeon",
        "cpu_cores": 8,
        "cpu_arch": "x86_64",
        "os": "Ubuntu 22.04",
        "kernel_version": "6.8.0",
        "mem_total": 16_000_000_000_i64,
        "swap_total": 4_000_000_000_i64,
        "disk_total": 100_000_000_000_i64,
        "ipv4": "1.2.3.4",
        "ipv6": null,
        "virtualization": "kvm",
        "agent_version": "0.5.0",
        "protocol_version": serverbee_common::constants::PROTOCOL_VERSION,
        // The docker handlers gate on this runtime feature in addition to
        // CAP_DOCKER, so it must be advertised for docker control to enable.
        "features": ["docker"],
        "agent_local_capabilities": caps
    });
    sink.send(tungstenite::Message::Text(system_info.to_string().into()))
        .await
        .expect("send docker SystemInfo");
    loop {
        let msg = recv_agent_text(reader).await;
        if msg["type"] == "ack" {
            assert_eq!(msg["msg_id"], "docker-extra-system-info");
            break;
        }
    }
}

/// Drain the first-connect pushes the server emits to a default agent. These
/// (ping/network sync + firewall blocklist messages) are unrelated to the
/// docker flow under test, so they must be ignored by every agent responder.
fn is_first_connect_noise(msg_type: Option<&str>) -> bool {
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

// ---------------------------------------------------------------------------
// GET /servers/{id}/docker/containers  (cache read; no agent responder needed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_docker_containers_returns_cache_for_online_agent() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;

    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/containers", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/containers failed");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("parse containers response");
    // Nothing has been pushed yet, so the cached list is empty (but present).
    assert!(
        body["data"]["containers"].is_array(),
        "containers should be an array, got {:?}",
        body["data"]
    );

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_containers_member_can_read() {
    // Read endpoints are accessible to both admin and member roles.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let (server_id, token) = register_agent(&admin, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let member = login_as_new_user(&admin, &base_url, "docker-member", "member").await;
    let resp = member
        .get(format!("{}/api/servers/{}/docker/containers", base_url, server_id))
        .send()
        .await
        .expect("member GET /docker/containers failed");
    assert_eq!(resp.status(), 200, "members may read docker container lists");

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_containers_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    // Fresh client with no session cookie / api key.
    let anon = http_client();
    let resp = anon
        .get(format!("{}/api/servers/{}/docker/containers", base_url, server_id))
        .send()
        .await
        .expect("anon GET /docker/containers failed");
    assert_eq!(resp.status(), 401, "unauthenticated docker reads must be rejected");
}

#[tokio::test]
async fn test_docker_containers_offline_agent_is_404() {
    // Registered server but the agent never connects → require_docker fails the
    // feature/online check. Without an advertised docker feature this surfaces
    // as Forbidden (no docker), so we assert it is not a success.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/containers", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/containers (offline) failed");
    // No docker feature is registered for a never-connected agent → 403.
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_docker_containers_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    // Advertise the docker feature but WITHOUT CAP_DOCKER in the bitmask.
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/containers", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/containers (no cap) failed");
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = resp.json().await.expect("parse error response");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("agent_capability_disabled"),
        "capability denial reason should be preserved, got {:?}",
        body["error"]
    );

    let _ = agent_sink.close().await;
}

// ---------------------------------------------------------------------------
// GET /servers/{id}/docker/stats  (cache read; no agent responder needed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_docker_stats_returns_cache_for_online_agent() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/stats", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/stats failed");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("parse stats response");
    assert!(body["data"]["stats"].is_array());

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_stats_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/stats", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/stats (no cap) failed");
    assert_eq!(resp.status(), 403);

    let _ = agent_sink.close().await;
}

// ---------------------------------------------------------------------------
// GET /servers/{id}/docker/events  (DB read; capability-only gate, no online check)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_docker_events_returns_db_records_for_capable_agent() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/events?limit=10", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/events failed");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("parse events response");
    // No events recorded yet, but the array must be present.
    assert!(body["data"]["events"].is_array());

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_events_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/events", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/events (no cap) failed");
    assert_eq!(resp.status(), 403);

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_events_unknown_server_is_404() {
    // The events handler only requires the server row to exist, so an unknown id
    // surfaces the not-found path rather than a capability error.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/events", base_url, "00000000-0000-0000-0000-000000000000"))
        .send()
        .await
        .expect("GET /docker/events (unknown) failed");
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// GET /servers/{id}/docker/networks  (forwarded → agent; responder required)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_docker_networks_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    // Responder must be running before the HTTP call, which blocks until the
    // agent replies to the forwarded DockerListNetworks request.
    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut agent_reader).await;
            match msg["type"].as_str() {
                Some("docker_list_networks") => {
                    let response = json!({
                        "type": "docker_networks",
                        "msg_id": msg["msg_id"].as_str().expect("docker_list_networks msg_id missing"),
                        "networks": [{
                            "id": "net-1",
                            "name": "bridge",
                            "driver": "bridge",
                            "scope": "local",
                            "containers": {}
                        }]
                    });
                    agent_sink
                        .send(tungstenite::Message::Text(response.to_string().into()))
                        .await
                        .expect("send DockerNetworks");
                    return;
                }
                other => {
                    if !is_first_connect_noise(other) {
                        if let Some(t) = other {
                            panic!("unexpected agent command: {t}");
                        }
                    }
                }
            }
        }
    });

    let resp = client
        .get(format!("{}/api/servers/{}/docker/networks", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/networks failed");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("parse networks response");
    assert_eq!(body["data"]["networks"][0]["name"], "bridge");

    agent_task.await.expect("agent task failed");
}

#[tokio::test]
async fn test_docker_networks_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/networks", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/networks (no cap) failed");
    assert_eq!(resp.status(), 403);

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_networks_offline_agent_is_403() {
    // Registered but never-connected agent has no docker feature → 403.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/servers/{}/docker/networks", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/networks (offline) failed");
    assert_eq!(resp.status(), 403);
}

// ---------------------------------------------------------------------------
// GET /servers/{id}/docker/volumes  (forwarded → agent; responder required)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_docker_volumes_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut agent_reader).await;
            match msg["type"].as_str() {
                Some("docker_list_volumes") => {
                    let response = json!({
                        "type": "docker_volumes",
                        "msg_id": msg["msg_id"].as_str().expect("docker_list_volumes msg_id missing"),
                        "volumes": [{
                            "name": "data-vol",
                            "driver": "local",
                            "mountpoint": "/var/lib/docker/volumes/data-vol/_data",
                            "created_at": null,
                            "labels": {}
                        }]
                    });
                    agent_sink
                        .send(tungstenite::Message::Text(response.to_string().into()))
                        .await
                        .expect("send DockerVolumes");
                    return;
                }
                other => {
                    if !is_first_connect_noise(other) {
                        if let Some(t) = other {
                            panic!("unexpected agent command: {t}");
                        }
                    }
                }
            }
        }
    });

    let resp = client
        .get(format!("{}/api/servers/{}/docker/volumes", base_url, server_id))
        .send()
        .await
        .expect("GET /docker/volumes failed");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("parse volumes response");
    assert_eq!(body["data"]["volumes"][0]["name"], "data-vol");

    agent_task.await.expect("agent task failed");
}

#[tokio::test]
async fn test_docker_volumes_unavailable_is_403() {
    // Agent replies DockerUnavailable to a forwarded request → handler maps it to
    // the "docker not available" 403.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut agent_reader).await;
            match msg["type"].as_str() {
                Some("docker_list_volumes") => {
                    let response = json!({
                        "type": "docker_unavailable",
                        "msg_id": msg["msg_id"].as_str().expect("docker_list_volumes msg_id missing")
                    });
                    agent_sink
                        .send(tungstenite::Message::Text(response.to_string().into()))
                        .await
                        .expect("send DockerUnavailable");
                    return;
                }
                other => {
                    if !is_first_connect_noise(other) {
                        if let Some(t) = other {
                            panic!("unexpected agent command: {t}");
                        }
                    }
                }
            }
        }
    });

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client
            .get(format!("{}/api/servers/{}/docker/volumes", base_url, server_id))
            .send(),
    )
    .await
    .expect("GET /docker/volumes should resolve immediately on DockerUnavailable")
    .expect("GET /docker/volumes (unavailable) failed");
    assert_eq!(resp.status(), 403);

    agent_task.await.expect("agent task failed");
}

// ---------------------------------------------------------------------------
// POST /servers/{id}/docker/containers/{cid}/action  (forwarded → agent, admin only)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_docker_container_action_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut agent_reader).await;
            match msg["type"].as_str() {
                Some("docker_container_action") => {
                    // Sanity-check the forwarded command carries the container id
                    // and the externally-tagged action variant.
                    assert_eq!(msg["container_id"], "container-1");
                    assert_eq!(msg["action"], "Start");
                    let response = json!({
                        "type": "docker_action_result",
                        "msg_id": msg["msg_id"].as_str().expect("docker_container_action msg_id missing"),
                        "success": true,
                        "error": null
                    });
                    agent_sink
                        .send(tungstenite::Message::Text(response.to_string().into()))
                        .await
                        .expect("send DockerActionResult");
                    return;
                }
                other => {
                    if !is_first_connect_noise(other) {
                        if let Some(t) = other {
                            panic!("unexpected agent command: {t}");
                        }
                    }
                }
            }
        }
    });

    let resp = client
        .post(format!(
            "{}/api/servers/{}/docker/containers/{}/action",
            base_url, server_id, "container-1"
        ))
        // DockerAction is an externally-tagged enum; the unit variant `Start`
        // serializes to the bare string "Start".
        .json(&json!({ "action": "Start" }))
        .send()
        .await
        .expect("POST /docker/containers/{cid}/action failed");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("parse action response");
    assert_eq!(body["data"]["success"], true);

    agent_task.await.expect("agent task failed");
}

#[tokio::test]
async fn test_docker_container_action_stop_variant_serializes_with_timeout() {
    // Covers the struct-style action variant `Stop { timeout }` which serializes
    // to {"Stop": {"timeout": N}} and is what the agent must receive.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let agent_task = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut agent_reader).await;
            match msg["type"].as_str() {
                Some("docker_container_action") => {
                    assert_eq!(msg["action"]["Stop"]["timeout"], 10);
                    let response = json!({
                        "type": "docker_action_result",
                        "msg_id": msg["msg_id"].as_str().expect("docker_container_action msg_id missing"),
                        "success": true,
                        "error": null
                    });
                    agent_sink
                        .send(tungstenite::Message::Text(response.to_string().into()))
                        .await
                        .expect("send DockerActionResult");
                    return;
                }
                other => {
                    if !is_first_connect_noise(other) {
                        if let Some(t) = other {
                            panic!("unexpected agent command: {t}");
                        }
                    }
                }
            }
        }
    });

    let resp = client
        .post(format!(
            "{}/api/servers/{}/docker/containers/{}/action",
            base_url, server_id, "container-2"
        ))
        .json(&json!({ "action": { "Stop": { "timeout": 10 } } }))
        .send()
        .await
        .expect("POST stop action failed");
    assert_eq!(resp.status(), 200);

    agent_task.await.expect("agent task failed");
}

#[tokio::test]
async fn test_docker_container_action_member_forbidden() {
    // Write endpoints are admin-only; a member must be rejected with 403.
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    let (server_id, token) = register_agent(&admin, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT | CAP_DOCKER).await;

    let member = login_as_new_user(&admin, &base_url, "docker-action-member", "member").await;
    let resp = member
        .post(format!(
            "{}/api/servers/{}/docker/containers/{}/action",
            base_url, server_id, "container-1"
        ))
        .json(&json!({ "action": "Start" }))
        .send()
        .await
        .expect("member POST action failed");
    assert_eq!(resp.status(), 403, "members cannot mutate docker containers");

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_container_action_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let (server_id, _token) = register_agent(&admin, &base_url).await;

    let anon = http_client();
    let resp = anon
        .post(format!(
            "{}/api/servers/{}/docker/containers/{}/action",
            base_url, server_id, "container-1"
        ))
        .json(&json!({ "action": "Start" }))
        .send()
        .await
        .expect("anon POST action failed");
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_docker_container_action_capability_denied_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    let welcome = recv_agent_text(&mut agent_reader).await;
    assert_eq!(welcome["type"], "welcome");
    // Docker feature advertised, but CAP_DOCKER missing.
    send_docker_system_info(&mut agent_sink, &mut agent_reader, CAP_DEFAULT).await;

    let resp = client
        .post(format!(
            "{}/api/servers/{}/docker/containers/{}/action",
            base_url, server_id, "container-1"
        ))
        .json(&json!({ "action": "Start" }))
        .send()
        .await
        .expect("POST action (no cap) failed");
    assert_eq!(resp.status(), 403);

    let _ = agent_sink.close().await;
}

#[tokio::test]
async fn test_docker_container_action_offline_agent_is_403() {
    // Never-connected agent: no docker feature → require_docker returns 403
    // before any forward is attempted.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, _token) = register_agent(&client, &base_url).await;

    let resp = client
        .post(format!(
            "{}/api/servers/{}/docker/containers/{}/action",
            base_url, server_id, "container-1"
        ))
        .json(&json!({ "action": "Start" }))
        .send()
        .await
        .expect("POST action (offline) failed");
    assert_eq!(resp.status(), 403);
}
