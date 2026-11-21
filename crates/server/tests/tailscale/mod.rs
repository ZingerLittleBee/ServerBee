//! End-to-end integration tests for Tailscale & Headscale server support:
//! - Module B: Host `TailscaleStatus` WS persistence, cache, offline fallback,
//!   capability gating (`CAP_TAILSCALE`), `ServerResponse` & `FullSync` fields,
//!   and `GET /api/servers/{id}/tailscale/status`.
//! - Module C: Global `/api/tailscale/{overview,config,test}`, runtime config
//!   persistence, API key redaction, audit logging, SSRF blocking, 30s caching,
//!   cache invalidation, and device-to-server auto-linking.
//! - Module D: `Tailscale-User-Login` loopback Serve auth over HTTP and Browser
//!   WebSocket — disabled by default, loopback-only, rejects `X-Forwarded-For` /
//!   `Forwarded` / non-loopback `ConnectInfo`, respects `must_change_password`,
//!   auto-provisions `member` (never `admin`), and verifies `tailscaled` Unix
//!   socket `/localapi/v0/whois` when `socket_path` is configured.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tower::ServiceExt;
use utoipa::OpenApi;

use crate::common::{
    connect_agent, http_client, is_first_connect_noise, login_admin, login_as_new_user,
    recv_agent_text, register_agent, send_system_info, start_test_server_with_state,
};
use serverbee_common::constants::{CAP_DEFAULT, CAP_TAILSCALE};
use serverbee_server::config::AppConfig;
use serverbee_server::entity::{audit_log, oauth_account, user};
use serverbee_server::openapi::ApiDoc;
use serverbee_server::router::create_router;
use serverbee_server::service::tailscale_control::{
    TailnetDeviceItem, TailscaleControlService, TailscaleUpstreamTransport, UpstreamQuery,
    parse_headscale_devices_json, parse_tailscale_devices_json,
};
use serverbee_server::state::AppState;

// ---------------------------------------------------------------------------
// Module D — Tailscale Serve Loopback Identity Auth (HTTP & WS)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tailscale_serve_auth_disabled_by_default_rejects_with_401() {
    let (base_url, _tmp, _state) = start_test_server_with_state().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{base_url}/api/auth/me"))
        .header("Tailscale-User-Login", "admin")
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        401,
        "Tailscale-User-Login must be rejected with 401 when auth_enabled=false"
    );

    // Browser WS also rejects with 401 when auth_enabled=false.
    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut ws_req = ws_url.into_client_request().unwrap();
    ws_req
        .headers_mut()
        .insert("Tailscale-User-Login", HeaderValue::from_static("admin"));
    let err = tokio_tungstenite::connect_async(ws_req)
        .await
        .expect_err("WS upgrade must fail when auth_enabled=false");
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(resp.status(), 401),
        other => panic!("expected HTTP 401 on WS handshake, got {other:?}"),
    }
}

#[tokio::test]
async fn tailscale_serve_auth_loopback_succeeds_for_http_and_ws_when_enabled() {
    let (base_url, _tmp, state) = start_test_server_with_state().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Enable Tailscale Serve loopback auth via the admin config endpoint.
    let put_resp = admin
        .put(format!("{base_url}/api/tailscale/config"))
        .json(&json!({
            "enabled": false,
            "provider": "tailscale",
            "api_base_url": "https://api.tailscale.com",
            "tailnet": "-",
            "auth_enabled": true,
            "auto_provision_role": ""
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_resp.status(), 200);

    // 1. Loopback HTTP request with exact username ("admin") succeeds.
    let ts_client = http_client();
    let me_resp = ts_client
        .get(format!("{base_url}/api/auth/me"))
        .header("Tailscale-User-Login", "admin")
        .send()
        .await
        .unwrap();
    assert_eq!(me_resp.status(), 200);
    let me_body: Value = me_resp.json().await.unwrap();
    assert_eq!(me_body["data"]["username"], "admin");
    assert_eq!(me_body["data"]["role"], "admin");

    // 2. Unique local-part match ("admin@tailnet.ts.net" -> "admin") succeeds.
    let local_part_resp = ts_client
        .get(format!("{base_url}/api/auth/me"))
        .header("Tailscale-User-Login", "admin@tailnet.ts.net")
        .send()
        .await
        .unwrap();
    assert_eq!(local_part_resp.status(), 200);
    let local_part_body: Value = local_part_resp.json().await.unwrap();
    assert_eq!(local_part_body["data"]["username"], "admin");

    // 3. Linked OAuth identity email match succeeds.
    let member_client = login_as_new_user(&admin, &base_url, "ts_linked_user", "member").await;
    let member_me: Value = member_client
        .get(format!("{base_url}/api/auth/me"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let member_id = member_me["data"]["user_id"].as_str().unwrap().to_string();
    oauth_account::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        user_id: Set(member_id),
        provider: Set("github".to_string()),
        provider_user_id: Set("gh-999".to_string()),
        email: Set(Some("linked.person@corp.example".to_string())),
        display_name: Set(Some("Linked Person".to_string())),
        created_at: Set(chrono::Utc::now()),
    }
    .insert(&state.db)
    .await
    .unwrap();

    let oauth_match = ts_client
        .get(format!("{base_url}/api/auth/me"))
        .header("Tailscale-User-Login", "linked.person@corp.example")
        .send()
        .await
        .unwrap();
    assert_eq!(oauth_match.status(), 200);
    let oauth_body: Value = oauth_match.json().await.unwrap();
    assert_eq!(oauth_body["data"]["username"], "ts_linked_user");

    // 4. Loopback Browser WebSocket (`/api/ws/servers`) upgrades and receives `full_sync`,
    //    and closes promptly if `auth_enabled` is disabled at runtime!
    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut ws_req = ws_url.into_client_request().unwrap();
    ws_req
        .headers_mut()
        .insert("Tailscale-User-Login", HeaderValue::from_static("admin"));
    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_req)
        .await
        .expect("loopback WS with Tailscale-User-Login should succeed");
    let (_sink, mut reader) = ws_stream.split();

    let first_frame = tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match first_frame {
        tungstenite::Message::Text(text) => {
            let parsed: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(parsed["type"], "full_sync");
        }
        other => panic!("expected full_sync text frame, got {other:?}"),
    }

    // Disabling auth_enabled revokes the active Tailscale WS lease.
    let disable_resp = admin
        .put(format!("{base_url}/api/tailscale/config"))
        .json(&json!({
            "enabled": false,
            "provider": "tailscale",
            "api_base_url": "https://api.tailscale.com",
            "tailnet": "-",
            "auth_enabled": false,
            "auto_provision_role": ""
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(disable_resp.status(), 200);

    let close_msg = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(frame)) = reader.next().await {
            if matches!(frame, tungstenite::Message::Close(_)) {
                return true;
            }
        }
        false
    })
    .await
    .expect("WS should close after disabling auth_enabled");
    assert!(close_msg);
}

#[tokio::test]
async fn tailscale_serve_auth_rejects_non_loopback_and_forwarded_headers_with_401() {
    let (base_url, _tmp, state) = start_test_server_with_state().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Enable auth_enabled via runtime config.
    let resp = admin
        .put(format!("{base_url}/api/tailscale/config"))
        .json(&json!({
            "enabled": false,
            "provider": "tailscale",
            "api_base_url": "https://api.tailscale.com",
            "tailnet": "-",
            "auth_enabled": true,
            "auto_provision_role": "member"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let anon = http_client();

    // (a) Loopback + X-Forwarded-For -> 401!
    for xff in ["203.0.113.99", "127.0.0.1", "100.64.0.5"] {
        let r = anon
            .get(format!("{base_url}/api/auth/me"))
            .header("Tailscale-User-Login", "admin")
            .header("X-Forwarded-For", xff)
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.status(),
            401,
            "X-Forwarded-For ({xff}) with Tailscale-User-Login must be rejected with 401"
        );
    }

    // (b) Loopback + Forwarded -> 401!
    let r_fwd = anon
        .get(format!("{base_url}/api/auth/me"))
        .header("Tailscale-User-Login", "admin")
        .header("Forwarded", "for=203.0.113.99;proto=https")
        .send()
        .await
        .unwrap();
    assert_eq!(r_fwd.status(), 401);

    // (c) Browser WS with X-Forwarded-For -> 401!
    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut ws_req = ws_url.into_client_request().unwrap();
    ws_req
        .headers_mut()
        .insert("Tailscale-User-Login", HeaderValue::from_static("admin"));
    ws_req
        .headers_mut()
        .insert("X-Forwarded-For", HeaderValue::from_static("203.0.113.99"));
    let ws_err = tokio_tungstenite::connect_async(ws_req)
        .await
        .expect_err("WS with X-Forwarded-For must fail");
    match ws_err {
        tungstenite::Error::Http(resp) => assert_eq!(resp.status(), 401),
        other => panic!("expected HTTP 401, got {other:?}"),
    }

    // (d) Non-loopback TCP `ConnectInfo<SocketAddr>` (public IP, LAN IP, CGNAT IP, or missing ConnectInfo) -> 401!
    let app = create_router(state.clone());
    for non_loopback in [
        Some("203.0.113.42:51111"),
        Some("10.0.0.25:51111"),
        Some("100.64.0.77:51111"),
        None,
    ] {
        let mut req = Request::builder()
            .method("GET")
            .uri("/api/auth/me")
            .header("Tailscale-User-Login", "admin")
            .body(Body::empty())
            .unwrap();
        if let Some(raw_addr) = non_loopback {
            let sock: SocketAddr = raw_addr.parse().unwrap();
            req.extensions_mut().insert(ConnectInfo(sock));
        }
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Peer {non_loopback:?} must be rejected with 401"
        );
    }
}

#[tokio::test]
async fn tailscale_serve_auth_respects_must_change_password_and_auto_provisions_only_member() {
    let (base_url, _tmp, state) = start_test_server_with_state().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;

    // Enable auth_enabled WITHOUT auto-provisioning first.
    admin
        .put(format!("{base_url}/api/tailscale/config"))
        .json(&json!({
            "enabled": false,
            "provider": "tailscale",
            "api_base_url": "https://api.tailscale.com",
            "tailnet": "-",
            "auth_enabled": true,
            "auto_provision_role": ""
        }))
        .send()
        .await
        .unwrap();

    let anon = http_client();

    // Unknown user is 401 when auto_provision_role is empty.
    let unknown = anon
        .get(format!("{base_url}/api/auth/me"))
        .header("Tailscale-User-Login", "stranger@tailnet.ts.net")
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 401);

    // Enable auto_provision_role = "member" -> auto-creates member (never admin!).
    admin
        .put(format!("{base_url}/api/tailscale/config"))
        .json(&json!({
            "enabled": false,
            "provider": "tailscale",
            "api_base_url": "https://api.tailscale.com",
            "tailnet": "-",
            "auth_enabled": true,
            "auto_provision_role": "member"
        }))
        .send()
        .await
        .unwrap();

    let provisioned = anon
        .get(format!("{base_url}/api/auth/me"))
        .header("Tailscale-User-Login", "NewMember@Tailnet.TS.Net")
        .send()
        .await
        .unwrap();
    assert_eq!(provisioned.status(), 200);
    let prov_body: Value = provisioned.json().await.unwrap();
    assert_eq!(prov_body["data"]["username"], "newmember@tailnet.ts.net");
    assert_eq!(prov_body["data"]["role"], "member");

    // Auto-provisioned member cannot access admin endpoints -> 403!
    let admin_check = anon
        .get(format!("{base_url}/api/tailscale/config"))
        .header("Tailscale-User-Login", "newmember@tailnet.ts.net")
        .send()
        .await
        .unwrap();
    assert_eq!(admin_check.status(), 403);

    // Flagging must_change_password blocks non-onboarding routes with 403 MUST_CHANGE_PASSWORD
    // and blocks WS upgrades with 401, while whitelisting GET /api/auth/me.
    let db_user = user::Entity::find()
        .filter(user::Column::Username.eq("newmember@tailnet.ts.net"))
        .one(&state.db)
        .await
        .unwrap()
        .unwrap();
    let mut active: user::ActiveModel = db_user.into();
    active.must_change_password = Set(true);
    active.update(&state.db).await.unwrap();

    let blocked_servers = anon
        .get(format!("{base_url}/api/servers"))
        .header("Tailscale-User-Login", "newmember@tailnet.ts.net")
        .send()
        .await
        .unwrap();
    assert_eq!(blocked_servers.status(), 403);
    let err_json: Value = blocked_servers.json().await.unwrap();
    assert_eq!(err_json["error"]["code"], "MUST_CHANGE_PASSWORD");

    let allowed_me = anon
        .get(format!("{base_url}/api/auth/me"))
        .header("Tailscale-User-Login", "newmember@tailnet.ts.net")
        .send()
        .await
        .unwrap();
    assert_eq!(allowed_me.status(), 200);

    // Even if static server.toml had `auto_provision_role = "admin"` (with no runtime override),
    // it refuses to auto-provision admin.
    serverbee_server::entity::config::Entity::delete_by_id(
        serverbee_server::service::tailscale_control::CONFIG_KEY_TAILSCALE,
    )
    .exec(&state.db)
    .await
    .unwrap();
    let mut bad_cfg = AppConfig::default();
    bad_cfg.tailscale.auth_enabled = true;
    bad_cfg.tailscale.auto_provision_role = "admin".to_string();
    let bad_state = AppState::new(state.db.clone(), bad_cfg).await.unwrap();
    let bad_app = create_router(bad_state);
    let mut req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header("Tailscale-User-Login", "would-be-admin@tailnet.ts.net")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:42222".parse::<SocketAddr>().unwrap(),
    ));
    let bad_resp = bad_app.oneshot(req).await.unwrap();
    assert_eq!(bad_resp.status(), StatusCode::UNAUTHORIZED);
}

#[cfg(unix)]
#[tokio::test]
async fn tailscale_serve_auth_verifies_localapi_whois_when_socket_configured() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (_base_url, tmp, state) = start_test_server_with_state().await;
    // Use a short socket path in /tmp so macOS SUN_LEN (104 bytes) is never exceeded.
    let sock_path = std::env::temp_dir().join(format!(
        "sb-ts-{}.sock",
        &uuid::Uuid::new_v4().to_string()[..8]
    ));
    let _ = std::fs::remove_file(&sock_path);
    let listener = tokio::net::UnixListener::bind(&sock_path).expect("bind mock tailscaled sock");

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req_str = String::from_utf8_lossy(&buf[..n]);
                if req_str.contains("addr=127.0.0.1:50001") {
                    let body = r#"{"UserProfile":{"LoginName":"admin"}}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                } else {
                    let _ = stream
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                        .await;
                }
            });
        }
    });

    let mut cfg = AppConfig::default();
    cfg.server.data_dir = tmp.path().to_string_lossy().to_string();
    cfg.tailscale.auth_enabled = true;
    cfg.tailscale.socket_path = sock_path.to_string_lossy().to_string();
    let custom_state = AppState::new(state.db.clone(), cfg).await.unwrap();
    let app = create_router(custom_state);

    // Matching peer + matching whois LoginName -> 200 OK!
    let mut ok_req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header("Tailscale-User-Login", "admin")
        .body(Body::empty())
        .unwrap();
    ok_req.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:50001".parse::<SocketAddr>().unwrap(),
    ));
    let ok_resp = app.clone().oneshot(ok_req).await.unwrap();
    assert_eq!(ok_resp.status(), StatusCode::OK);

    // Mismatched LoginName -> 401!
    let mut spoof_login_req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header("Tailscale-User-Login", "someone-else")
        .body(Body::empty())
        .unwrap();
    spoof_login_req.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:50001".parse::<SocketAddr>().unwrap(),
    ));
    let spoof_resp = app.clone().oneshot(spoof_login_req).await.unwrap();
    assert_eq!(spoof_resp.status(), StatusCode::UNAUTHORIZED);

    // Unknown peer port (404 from whois) -> 401!
    let mut non_ts_port_req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header("Tailscale-User-Login", "admin")
        .body(Body::empty())
        .unwrap();
    non_ts_port_req.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:59999".parse::<SocketAddr>().unwrap(),
    ));
    let non_ts_resp = app.oneshot(non_ts_port_req).await.unwrap();
    assert_eq!(non_ts_resp.status(), StatusCode::UNAUTHORIZED);

    let _ = std::fs::remove_file(&sock_path);
}

// ---------------------------------------------------------------------------
// Module B — Host Tailscale Status WS Relay, Persistence, Cache & Capability Gate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_tailscale_status_persists_relays_caches_offline_and_gates_capability() {
    let (base_url, _tmp, _state) = start_test_server_with_state().await;
    let admin = http_client();
    let (server_id, token) = register_agent(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "ts_reader", "member").await;

    let (mut agent_sink, mut agent_reader) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut agent_reader).await["type"], "welcome");

    // Send SystemInfo with CAP_DEFAULT (which includes CAP_TAILSCALE) and feature "tailscale".
    let sys_info = json!({
        "type": "system_info",
        "msg_id": "ts-init-1",
        "cpu_name": "AMD EPYC",
        "cpu_cores": 4,
        "cpu_arch": "x86_64",
        "os": "Ubuntu 24.04",
        "kernel_version": "6.8.0",
        "mem_total": 8_000_000_000_i64,
        "swap_total": 0_i64,
        "disk_total": 80_000_000_000_i64,
        "ipv4": "203.0.113.25",
        "ipv6": null,
        "virtualization": "kvm",
        "agent_version": "0.2.0",
        "protocol_version": serverbee_common::constants::PROTOCOL_VERSION,
        "features": ["tailscale"],
        "agent_local_capabilities": CAP_DEFAULT
    });
    agent_sink
        .send(tungstenite::Message::Text(sys_info.to_string().into()))
        .await
        .unwrap();
    loop {
        let msg = recv_agent_text(&mut agent_reader).await;
        if msg["type"] == "ack" && msg["msg_id"] == "ts-init-1" {
            break;
        }
    }

    // Push unsolicited TailscaleStatus from agent -> persists summary on `servers`.
    let initial_push = json!({
        "type": "tailscale_status",
        "status": {
            "summary": {
                "backend_state": "Running",
                "tailscale_ips": ["100.77.88.99", "fd7a:115c:a1e0::99"],
                "dns_name": "edge-01.tail99.ts.net.",
                "tailnet_name": "tail99.ts.net",
                "version": "1.76.1",
                "hostname": "edge-01",
                "online": true,
                "exit_node": false,
                "exit_node_option": true,
                "key_expiry": "2027-06-01T00:00:00Z"
            },
            "advertised_routes": ["10.10.0.0/16"],
            "tags": ["tag:edge"],
            "derp_region": "tok",
            "peers": [
                {
                    "id": "peer-1",
                    "host_name": "core-db",
                    "dns_name": "core-db.tail99.ts.net",
                    "os": "linux",
                    "tailscale_ips": ["100.77.88.10"],
                    "online": true,
                    "active": true,
                    "connection_mode": "direct",
                    "cur_addr": "198.51.100.10:41641",
                    "relay": "tok",
                    "rx_bytes": 4096,
                    "tx_bytes": 8192,
                    "last_handshake": "2026-11-21T03:30:00Z",
                    "last_seen": null,
                    "exit_node": false,
                    "exit_node_option": false,
                    "tags": []
                }
            ],
            "health_warnings": [],
            "collected_at": "2026-11-21T03:35:00Z"
        }
    });
    agent_sink
        .send(tungstenite::Message::Text(initial_push.to_string().into()))
        .await
        .unwrap();

    // Synchronize via a second SystemInfo ack so the push is guaranteed processed.
    send_system_info(
        &mut agent_sink,
        &mut agent_reader,
        "ts-sync-barrier",
        Some(CAP_DEFAULT),
    )
    .await;

    // Verify ServerResponse (`GET /api/servers/{id}`) exposes persisted tailscale_* fields.
    let srv_resp: Value = member
        .get(format!("{base_url}/api/servers/{server_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(srv_resp["data"]["tailscale_ipv4"], "100.77.88.99");
    assert_eq!(srv_resp["data"]["tailscale_ipv6"], "fd7a:115c:a1e0::99");
    assert_eq!(
        srv_resp["data"]["tailscale_dns_name"],
        "edge-01.tail99.ts.net"
    );
    assert_eq!(srv_resp["data"]["tailscale_tailnet"], "tail99.ts.net");

    // Spawn responder task on mock agent to answer `tailscale_get_status` with fresh data.
    let responder = tokio::spawn(async move {
        loop {
            let msg = recv_agent_text(&mut agent_reader).await;
            let msg_type = msg["type"].as_str();
            if is_first_connect_noise(msg_type) {
                continue;
            }
            if msg_type == Some("tailscale_get_status") {
                let msg_id = msg["msg_id"].as_str().unwrap().to_string();
                let reply = json!({
                    "type": "tailscale_status",
                    "msg_id": msg_id,
                    "status": {
                        "summary": {
                            "backend_state": "Running",
                            "tailscale_ips": ["100.77.88.99", "fd7a:115c:a1e0::99"],
                            "dns_name": "edge-01.tail99.ts.net",
                            "tailnet_name": "tail99.ts.net",
                            "version": "1.78.0",
                            "hostname": "edge-01",
                            "online": true,
                            "exit_node": false,
                            "exit_node_option": true,
                            "key_expiry": "2027-06-01T00:00:00Z"
                        },
                        "advertised_routes": ["10.10.0.0/16"],
                        "tags": ["tag:edge"],
                        "derp_region": "sfo",
                        "peers": [],
                        "health_warnings": [],
                        "collected_at": "2026-11-21T03:36:00Z"
                    }
                });
                agent_sink
                    .send(tungstenite::Message::Text(reply.to_string().into()))
                    .await
                    .unwrap();
                return agent_sink;
            }
        }
    });

    // Member queries live `GET /api/servers/{id}/tailscale/status`.
    let live_status_resp = member
        .get(format!(
            "{base_url}/api/servers/{server_id}/tailscale/status"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(live_status_resp.status(), 200);
    let live_body: Value = live_status_resp.json().await.unwrap();
    assert_eq!(live_body["data"]["available"], true);
    assert_eq!(live_body["data"]["online"], true);
    assert_eq!(live_body["data"]["cached"], false);
    assert_eq!(live_body["data"]["status"]["derp_region"], "sfo");
    assert_eq!(live_body["data"]["status"]["summary"]["version"], "1.78.0");

    let mut agent_sink = responder.await.unwrap();

    // Disconnect agent and verify offline cached fallback (`online: false, cached: true`).
    let _ = agent_sink.close().await;
    tokio::time::sleep(Duration::from_millis(80)).await;

    let offline_resp = member
        .get(format!(
            "{base_url}/api/servers/{server_id}/tailscale/status"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(offline_resp.status(), 200);
    let offline_body: Value = offline_resp.json().await.unwrap();
    assert_eq!(offline_body["data"]["available"], true);
    assert_eq!(offline_body["data"]["online"], false);
    assert_eq!(offline_body["data"]["cached"], true);
    assert_eq!(offline_body["data"]["tailscale_ipv4"], "100.77.88.99");
    assert_eq!(offline_body["data"]["status"]["derp_region"], "sfo");

    // Reconnect agent WITHOUT CAP_TAILSCALE -> 403 Forbidden!
    let (mut sink2, mut reader2) = connect_agent(&base_url, &token).await;
    assert_eq!(recv_agent_text(&mut reader2).await["type"], "welcome");
    let caps_without_ts = CAP_DEFAULT & !CAP_TAILSCALE;
    send_system_info(
        &mut sink2,
        &mut reader2,
        "ts-disabled",
        Some(caps_without_ts),
    )
    .await;

    let denied = member
        .get(format!(
            "{base_url}/api/servers/{server_id}/tailscale/status"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    let _ = sink2.close().await;
}

// ---------------------------------------------------------------------------
// Module C — Global Control Plane Config, Audit, SSRF Guard, Overview & Caching
// ---------------------------------------------------------------------------

struct MockUpstream {
    calls: AtomicUsize,
}

#[async_trait]
impl TailscaleUpstreamTransport for MockUpstream {
    async fn fetch_devices(&self, query: &UpstreamQuery) -> Result<Vec<TailnetDeviceItem>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match query.provider.as_str() {
            "tailscale" => parse_tailscale_devices_json(
                r#"{
                    "devices": [
                        {
                            "id": "dev-1",
                            "name": "linked-srv.tail99.ts.net",
                            "hostname": "linked-srv",
                            "os": "linux",
                            "addresses": ["100.90.1.5"],
                            "authorized": true,
                            "connectedToControl": true,
                            "keyExpiryDisabled": false,
                            "expires": "2026-11-23T00:00:00Z",
                            "clientVersion": "1.76.1",
                            "tags": ["tag:srv"]
                        },
                        {
                            "id": "dev-2",
                            "name": "laptop.tail99.ts.net",
                            "hostname": "laptop",
                            "os": "macos",
                            "addresses": ["100.90.1.9"],
                            "authorized": true,
                            "connectedToControl": false,
                            "keyExpiryDisabled": true
                        }
                    ]
                }"#,
            ),
            "headscale" => parse_headscale_devices_json(
                r#"{
                    "nodes": [
                        {
                            "id": 7,
                            "name": "linked-srv",
                            "givenName": "linked-srv.hs.internal",
                            "ipAddresses": ["100.90.1.5"],
                            "online": true,
                            "expiry": "0001-01-01T00:00:00Z"
                        }
                    ]
                }"#,
            ),
            other => Err(format!("unexpected provider {other}")),
        }
    }
}

#[tokio::test]
async fn global_tailscale_config_ssrf_redaction_audit_overview_and_cache() {
    let (base_url, _tmp, state) = start_test_server_with_state().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "ts_member", "member").await;

    // 1. RBAC: member cannot read/write `/api/tailscale/config` or `/api/tailscale/test`,
    //    but CAN read `/api/tailscale/overview`.
    assert_eq!(
        member
            .get(format!("{base_url}/api/tailscale/config"))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        member
            .post(format!("{base_url}/api/tailscale/test"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let unconfigured_overview: Value = member
        .get(format!("{base_url}/api/tailscale/overview"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unconfigured_overview["data"]["configured"], false);

    // 2. SSRF protection on PUT /api/tailscale/config and POST /api/tailscale/test:
    //    loopback, localhost, and cloud metadata 169.254.169.254 are blocked with 422!
    for blocked_url in [
        "http://127.0.0.1:8080",
        "http://[::1]:8080",
        "http://localhost:8080",
        "http://169.254.169.254/latest/meta-data",
        "ftp://api.tailscale.com",
    ] {
        let put_blocked = admin
            .put(format!("{base_url}/api/tailscale/config"))
            .json(&json!({
                "enabled": true,
                "provider": "headscale",
                "api_base_url": blocked_url,
                "tailnet": "-",
                "api_key": "hs-secret"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            put_blocked.status(),
            422,
            "PUT /api/tailscale/config must block SSRF URL {blocked_url}"
        );

        let test_blocked = admin
            .post(format!("{base_url}/api/tailscale/test"))
            .json(&json!({
                "provider": "headscale",
                "api_base_url": blocked_url,
                "tailnet": "-",
                "api_key": "hs-secret"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            test_blocked.status(),
            422,
            "POST /api/tailscale/test must block SSRF URL {blocked_url}"
        );
    }

    // 3. Save config with secret api_key -> verify secret NEVER leaks in GET/PUT responses or audit logs!
    let secret = "tskey-api-VERY-SECRET-VALUE-999";
    let saved = admin
        .put(format!("{base_url}/api/tailscale/config"))
        .json(&json!({
            "enabled": true,
            "provider": "tailscale",
            "api_base_url": "https://api.tailscale.com",
            "tailnet": "example.com",
            "api_key": secret,
            "preferred_server_url": "https://bee.tail99.ts.net",
            "auth_enabled": false,
            "auto_provision_role": ""
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), 200);
    let saved_text = saved.text().await.unwrap();
    assert!(
        !saved_text.contains(secret),
        "PUT response must never leak api_key"
    );
    let saved_json: Value = serde_json::from_str(&saved_text).unwrap();
    assert_eq!(saved_json["data"]["has_api_key"], true);

    let got_text = admin
        .get(format!("{base_url}/api/tailscale/config"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !got_text.contains(secret),
        "GET response must never leak api_key"
    );

    let audits = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq("tailscale.config.update"))
        .all(&state.db)
        .await
        .unwrap();
    assert!(
        !audits.is_empty(),
        "tailscale.config.update must be audit-logged"
    );
    for row in &audits {
        assert!(
            !row.detail.as_deref().unwrap_or_default().contains(secret),
            "Audit log must never contain plaintext api_key"
        );
    }

    // 4. Exercise TailscaleControlService overview + auto-linking + 30s cache + invalidation
    //    with a seeded ServerBee server matching `100.90.1.5`.
    let srv_id = crate::common::create_server(&admin, &base_url, "Prod Node").await;
    serverbee_server::service::server::ServerService::update_tailscale_info(
        &state.db,
        &srv_id,
        Some("100.90.1.5"),
        None,
        Some("linked-srv.tail99.ts.net"),
        Some("tail99.ts.net"),
    )
    .await
    .unwrap();

    let mock_transport = Arc::new(MockUpstream {
        calls: AtomicUsize::new(0),
    });
    let svc = TailscaleControlService::with_transport(
        AppConfig::default().tailscale,
        "0.0.0.0:9527".to_string(),
        mock_transport.clone(),
    );

    let ov1 = svc.get_overview(&state.db).await.unwrap();
    assert!(ov1.configured);
    assert_eq!(
        ov1.server_tailnet_url.as_deref(),
        Some("https://bee.tail99.ts.net")
    );
    assert_eq!(ov1.total_devices, 2);
    assert_eq!(ov1.online_devices, 1);
    assert_eq!(ov1.linked_servers, 1);
    assert_eq!(
        ov1.devices[0].matched_server_id.as_deref(),
        Some(srv_id.as_str())
    );
    assert_eq!(
        ov1.devices[0].matched_server_name.as_deref(),
        Some("Prod Node")
    );
    assert_eq!(mock_transport.calls.load(Ordering::SeqCst), 1);

    // Second call within 30s hits the in-memory cache without re-querying upstream.
    let ov2 = svc.get_overview(&state.db).await.unwrap();
    assert_eq!(ov2.total_devices, 2);
    assert_eq!(mock_transport.calls.load(Ordering::SeqCst), 1);

    // Switching to self-hosted Headscale on LAN IP invalidates cache and queries Headscale.
    svc.update_config(
        &state.db,
        serverbee_server::service::tailscale_control::UpdateTailscaleConfigRequest {
            enabled: true,
            provider: "headscale".into(),
            api_base_url: "http://10.20.30.40:8080".into(),
            tailnet: "default".into(),
            api_key: None,
            preferred_server_url: None,
            auth_enabled: None,
            auto_provision_role: None,
        },
    )
    .await
    .unwrap();

    let hs_test = svc
        .test_connection(
            &state.db,
            serverbee_server::service::tailscale_control::TestTailscaleConfigRequest::default(),
        )
        .await
        .unwrap();
    assert!(hs_test.success);
    assert_eq!(hs_test.device_count, 1);

    let ov_hs = svc.get_overview(&state.db).await.unwrap();
    assert_eq!(ov_hs.provider, "headscale");
    assert_eq!(ov_hs.total_devices, 1);
    assert_eq!(ov_hs.linked_servers, 1);
    assert_eq!(mock_transport.calls.load(Ordering::SeqCst), 3);

    // 5. Verify all 5 Tailscale OpenAPI paths and schemas are registered in ApiDoc.
    let openapi_json = serde_json::to_value(ApiDoc::openapi()).unwrap();
    for expected_path in [
        "/api/servers/{id}/tailscale/status",
        "/api/tailscale/overview",
        "/api/tailscale/config",
        "/api/tailscale/test",
    ] {
        assert!(
            openapi_json["paths"].get(expected_path).is_some(),
            "OpenAPI spec missing path {expected_path}"
        );
    }
    for expected_schema in [
        "ServerTailscaleStatusResponse",
        "TailscaleStatusReport",
        "TailscaleNodeSummary",
        "TailscalePeerInfo",
        "TailnetOverviewResponse",
        "TailnetDeviceItem",
        "TailscaleAdminConfigResponse",
        "UpdateTailscaleConfigRequest",
        "TestTailscaleConfigRequest",
        "TailscaleConnectionTestResponse",
    ] {
        assert!(
            openapi_json["components"]["schemas"]
                .get(expected_schema)
                .is_some(),
            "OpenAPI spec missing schema {expected_schema}"
        );
    }
}
