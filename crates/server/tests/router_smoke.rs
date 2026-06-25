//! Smoke test validating the shared `common` harness end to end.
mod common;

use common::{
    connect_agent, create_server, http_client, login_admin, login_as_new_user, recv_agent_text,
    register_agent, send_system_info, start_test_server,
};
use serde_json::Value;

#[tokio::test]
async fn harness_login_create_server_and_member_role() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    // Admin login succeeds and reports the admin role.
    let me = login_admin(&client, &base_url).await;
    assert_eq!(me["data"]["role"].as_str(), Some("admin"));

    // create_server returns a usable server id, visible via GET /api/servers.
    let server_id = create_server(&client, &base_url, "smoke-server").await;
    assert!(!server_id.is_empty());

    let list: Value = client
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ids: Vec<&str> = list["data"]
        .as_array()
        .expect("servers list array")
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    assert!(ids.contains(&server_id.as_str()), "created server should be listed");

    // A freshly minted member can log in but is forbidden from admin writes.
    let member = login_as_new_user(&client, &base_url, "member1", "member").await;
    let resp = member
        .post(format!("{}/api/servers", base_url))
        .json(&serde_json::json!({ "name": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "member must not create servers");
}

#[tokio::test]
async fn harness_mock_agent_handshake() {
    // A mock agent can register, connect over WS, receive the welcome frame, and
    // complete the SystemInfo handshake (server Ack). Validates the agent helpers.
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_server_id, token) = register_agent(&client, &base_url).await;
    let (mut sink, mut reader) = connect_agent(&base_url, &token).await;

    let welcome = recv_agent_text(&mut reader).await;
    assert_eq!(welcome["type"], "welcome");

    // Completes only if the server returns an Ack for the SystemInfo message.
    send_system_info(&mut sink, &mut reader, "sysinfo-1", None).await;
}
