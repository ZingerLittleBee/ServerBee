//! Integration test: redaction on `/api/status/*` is unconditional — admin
//! session cookies and admin API keys must NOT unmask any field.
//!
//! Pairs with `public_status_redaction.rs` which verifies the anonymous path.

use std::time::Duration;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectOptions, ConnectionTrait, Database, EntityTrait, Set};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::entity::{server, status_page};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

const SENSITIVE_IPV4: &str = "1.2.3.4";
const SENSITIVE_IPV6: &str = "fe80::1";

const FORBIDDEN_KEYS: &[&str] = &[
    "ipv4",
    "ipv6",
    "interfaces",
    "public_ip",
    "mac_address",
    "network_interface",
    "network_interfaces",
];

async fn start_test_server() -> (String, std::sync::Arc<AppState>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tmp");
    let data_dir = tmp.path().to_str().unwrap().to_string();

    let config = AppConfig {
        server: ServerConfig {
            listen: "127.0.0.1:0".to_string(),
            data_dir: data_dir.clone(),
            trusted_proxies: Vec::new(),
        },
        database: DatabaseConfig {
            path: "test.db".to_string(),
            max_connections: 5,
        },
        auth: AuthConfig {
            session_ttl: 86400,
            secure_cookie: false,
            max_servers: 0,
        },
        ..AppConfig::default()
    };

    let db_path = format!("{}/test.db", data_dir);
    let db_url = format!("sqlite://{}?mode=rwc", db_path);
    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(5);
    opt.sqlx_logging(false);
    let db = Database::connect(opt).await.expect("connect db");
    db.execute_unprepared("PRAGMA journal_mode=WAL")
        .await
        .unwrap();
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .unwrap();
    Migrator::up(&db, None).await.expect("migrations");

    AuthService::create_user(&db, "admin", "testpass", "admin")
        .await
        .expect("seed admin");

    let state = AppState::new(db, config).await.expect("AppState");
    let state_arc = state.clone();
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (base_url, state_arc, tmp)
}

async fn insert_server_with_ips(
    db: &sea_orm::DatabaseConnection,
    id: &str,
    ipv4: &str,
    ipv6: &str,
) {
    let now = Utc::now();
    server::ActiveModel {
        id: Set(id.to_string()),
        token_hash: Set(None),
        token_prefix: Set(None),
        name: Set("auth-redact-target".to_string()),
        cpu_name: Set(Some("Intel Xeon".to_string())),
        cpu_cores: Set(Some(8)),
        cpu_arch: Set(Some("x86_64".to_string())),
        os: Set(Some("Ubuntu 22.04".to_string())),
        kernel_version: Set(Some("5.15".to_string())),
        mem_total: Set(Some(16_000_000_000)),
        swap_total: Set(Some(4_000_000_000)),
        disk_total: Set(Some(100_000_000_000)),
        ipv4: Set(Some(ipv4.to_string())),
        ipv6: Set(Some(ipv6.to_string())),
        region: Set(Some("US".to_string())),
        country_code: Set(Some("US".to_string())),
        geo_manual: Set(false),
        virtualization: Set(Some("kvm".to_string())),
        agent_version: Set(Some("0.1.0".to_string())),
        group_id: Set(None),
        weight: Set(0),
        hidden: Set(false),
        remark: Set(None),
        public_remark: Set(Some("public note".to_string())),
        price: Set(None),
        billing_cycle: Set(None),
        currency: Set(None),
        expired_at: Set(None),
        traffic_limit: Set(None),
        traffic_limit_type: Set(None),
        billing_start_day: Set(None),
        capabilities: Set(0),
        protocol_version: Set(1),
        features: Set("[]".to_string()),
        last_remote_addr: Set(None),
        fingerprint: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .expect("insert server");
}

async fn enable_status_page(db: &sea_orm::DatabaseConnection, server_ids: Vec<String>) {
    let existing = status_page::Entity::find()
        .one(db)
        .await
        .unwrap()
        .expect("singleton row");
    let mut m: status_page::ActiveModel = existing.into();
    m.enabled = Set(true);
    m.server_ids_json = Set(serde_json::to_string(&server_ids).unwrap());
    m.show_server_detail = Set(true);
    m.show_network = Set(true);
    m.show_ip_quality = Set(true);
    m.show_incidents = Set(true);
    m.show_maintenance = Set(true);
    m.updated_at = Set(Utc::now());
    m.update(db).await.expect("update status page");
}

fn assert_no_keys(v: &serde_json::Value, forbidden: &[&str]) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, vv) in map {
                assert!(
                    !forbidden.contains(&k.as_str()),
                    "forbidden key `{k}` found in response: value={vv}"
                );
                assert_no_keys(vv, forbidden);
            }
        }
        serde_json::Value::Array(arr) => arr.iter().for_each(|vv| assert_no_keys(vv, forbidden)),
        _ => {}
    }
}

/// Hit the public-status surface with the given request builder, verify
/// status 200 and absence of forbidden keys/substrings, and return the parsed
/// JSON body.
async fn fetch_redacted(builder: reqwest::RequestBuilder, label: &str) -> serde_json::Value {
    let resp = builder
        .send()
        .await
        .unwrap_or_else(|e| panic!("[{label}] transport error: {e}"));
    let status = resp.status();
    let text = resp.text().await.expect("body");
    assert_eq!(status, 200, "[{label}] expected 200, got {status}: {text}");
    let value: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_no_keys(&value["data"], FORBIDDEN_KEYS);
    assert!(
        !text.contains(SENSITIVE_IPV4),
        "[{label}] body must not contain `{SENSITIVE_IPV4}`: {text}"
    );
    assert!(
        !text.contains(SENSITIVE_IPV6),
        "[{label}] body must not contain `{SENSITIVE_IPV6}`: {text}"
    );
    value
}

#[tokio::test]
async fn public_status_redaction_is_unconditional_for_authenticated_callers() {
    let (base_url, state, _tmp) = start_test_server().await;
    let server_id = "srv-auth-redact-1";

    insert_server_with_ips(&state.db, server_id, SENSITIVE_IPV4, SENSITIVE_IPV6).await;
    enable_status_page(&state.db, vec![server_id.to_string()]).await;

    // Build three clients:
    //   1. anonymous (baseline)
    //   2. authenticated via session cookie (admin)
    //   3. authenticated via X-API-Key (admin)
    let anon = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let session = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let login = session
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "admin", "password": "testpass" }))
        .send()
        .await
        .expect("login");
    assert_eq!(login.status(), 200, "admin login should succeed");

    let api_key_resp = session
        .post(format!("{}/api/auth/api-keys", base_url))
        .json(&json!({ "name": "redaction-test-key" }))
        .send()
        .await
        .expect("create api key");
    assert_eq!(api_key_resp.status(), 200);
    let api_key_body: serde_json::Value = api_key_resp.json().await.unwrap();
    let api_key = api_key_body["data"]["key"]
        .as_str()
        .expect("api key string")
        .to_string();

    let urls = [
        ("/api/status", format!("{}/api/status", base_url)),
        (
            "/api/status/servers/{id}",
            format!("{}/api/status/servers/{}", base_url, server_id),
        ),
    ];

    // Fetch each URL via all three transports; bodies must be byte-identical
    // shape (same key set on `data`) regardless of auth.
    for (label, url) in urls.iter() {
        let anon_body = fetch_redacted(anon.get(url), &format!("anon {label}")).await;
        let session_body = fetch_redacted(session.get(url), &format!("session {label}")).await;
        let key_body = fetch_redacted(
            anon.get(url).header("X-API-Key", &api_key),
            &format!("api-key {label}"),
        )
        .await;

        // Equality of `data` is the strongest assertion we can make: admin
        // cookies/api keys must not inject extra fields.
        assert_eq!(
            anon_body["data"], session_body["data"],
            "[{label}] session-authenticated body diverges from anonymous body"
        );
        assert_eq!(
            anon_body["data"], key_body["data"],
            "[{label}] api-key-authenticated body diverges from anonymous body"
        );
    }
}
