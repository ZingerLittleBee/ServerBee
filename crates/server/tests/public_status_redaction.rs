//! Integration test: server-identity fields are redacted from the
//! public-status surface even when the corresponding `servers` row holds
//! non-null `ipv4`/`ipv6` values.
//!
//! Verifies defense-in-depth at the DTO boundary by recursing the response
//! JSON and asserting both key absence and substring absence.

use std::time::Duration;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectOptions, ConnectionTrait, Database, EntityTrait, Set};
use sea_orm_migration::MigratorTrait;
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::entity::{server, status_page};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

const SENSITIVE_IPV4: &str = "1.2.3.4";
const SENSITIVE_IPV6: &str = "fe80::1";

/// Keys that must never appear anywhere in a public-status response tree.
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
        name: Set("redaction-target".to_string()),
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

/// Recursively walk a JSON value and panic if any forbidden key appears.
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

async fn fetch_json(client: &reqwest::Client, url: &str) -> (serde_json::Value, String) {
    let resp = client
        .get(url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url} transport error: {e}"));
    let status = resp.status();
    let text = resp.text().await.expect("body text");
    assert_eq!(status, 200, "expected 200 for {url}, got {status}: {text}");
    let value: serde_json::Value =
        serde_json::from_str(&text).expect("response should be JSON");
    (value, text)
}

#[tokio::test]
async fn public_status_redacts_server_identity_fields() {
    let (base_url, state, _tmp) = start_test_server().await;
    let server_id = "srv-redact-1";

    insert_server_with_ips(&state.db, server_id, SENSITIVE_IPV4, SENSITIVE_IPV6).await;
    enable_status_page(&state.db, vec![server_id.to_string()]).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client");

    // /api/status — list of summaries
    let (list_body, list_raw) =
        fetch_json(&client, &format!("{}/api/status", base_url)).await;
    let list_data = &list_body["data"];
    assert!(list_data.is_array(), "data should be array");
    assert!(!list_data.as_array().unwrap().is_empty(), "should contain the seeded server");
    assert_no_keys(list_data, FORBIDDEN_KEYS);
    assert!(
        !list_raw.contains(SENSITIVE_IPV4),
        "/api/status body must not contain the sensitive ipv4 substring; got: {list_raw}"
    );
    assert!(
        !list_raw.contains(SENSITIVE_IPV6),
        "/api/status body must not contain the sensitive ipv6 substring; got: {list_raw}"
    );

    // /api/status/servers/{id} — detail
    let (detail_body, detail_raw) = fetch_json(
        &client,
        &format!("{}/api/status/servers/{}", base_url, server_id),
    )
    .await;
    assert_no_keys(&detail_body["data"], FORBIDDEN_KEYS);
    assert!(
        !detail_raw.contains(SENSITIVE_IPV4),
        "/api/status/servers/{{id}} body must not contain the sensitive ipv4 substring; got: {detail_raw}"
    );
    assert!(
        !detail_raw.contains(SENSITIVE_IPV6),
        "/api/status/servers/{{id}} body must not contain the sensitive ipv6 substring; got: {detail_raw}"
    );
}
