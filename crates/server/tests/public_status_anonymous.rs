//! Integration test: every public `/api/status/*` endpoint returns 200 to
//! anonymous callers when the singleton config has `enabled = true` and every
//! `show_*` toggle is on, and the in-scope server has at least one report.
//!
//! Counterpart to `public_status_gating.rs` (which exercises 403s). Together
//! they pin the contract that all eight public endpoints under
//! `/api/status/*` are reachable without auth and gated only by the singleton
//! config.

use std::time::Duration;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectOptions, ConnectionTrait, Database, EntityTrait, Set};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use serverbee_common::types::SystemReport;
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::entity::{incident, maintenance, server, status_page};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

// ---------------------------------------------------------------------------
// Test harness (mirrors integration.rs and ip_quality_integration.rs)
// ---------------------------------------------------------------------------

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

fn anon_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client")
}

/// Insert a `servers` row directly. We do not go through the full
/// agent-register flow because we want full control over `ipv4`/`ipv6`/etc.
/// for the redaction tests; this helper is reused across the public-status
/// integration suite.
pub(crate) async fn insert_server(
    db: &sea_orm::DatabaseConnection,
    id: &str,
    name: &str,
    ipv4: Option<&str>,
    ipv6: Option<&str>,
    hidden: bool,
) {
    let now = Utc::now();
    let model = server::ActiveModel {
        id: Set(id.to_string()),
        token_hash: Set(None),
        token_prefix: Set(None),
        name: Set(name.to_string()),
        cpu_name: Set(Some("Intel Xeon".to_string())),
        cpu_cores: Set(Some(8)),
        cpu_arch: Set(Some("x86_64".to_string())),
        os: Set(Some("Ubuntu 22.04".to_string())),
        kernel_version: Set(Some("5.15".to_string())),
        mem_total: Set(Some(16_000_000_000)),
        swap_total: Set(Some(4_000_000_000)),
        disk_total: Set(Some(100_000_000_000)),
        ipv4: Set(ipv4.map(String::from)),
        ipv6: Set(ipv6.map(String::from)),
        region: Set(Some("US".to_string())),
        country_code: Set(Some("US".to_string())),
        virtualization: Set(Some("kvm".to_string())),
        agent_version: Set(Some("0.1.0".to_string())),
        group_id: Set(None),
        weight: Set(0),
        hidden: Set(hidden),
        remark: Set(None),
        public_remark: Set(Some("public remark".to_string())),
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
    };
    model.insert(db).await.expect("insert server");
}

/// Configure the singleton status_page row. `enabled` plus all `show_*` flags
/// default to `true` for convenience; callers override the ones they need.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn configure_status_page(
    db: &sea_orm::DatabaseConnection,
    enabled: bool,
    server_ids: Vec<String>,
    show_server_detail: bool,
    show_network: bool,
    show_ip_quality: bool,
    show_incidents: bool,
    show_maintenance: bool,
) {
    let existing = status_page::Entity::find()
        .one(db)
        .await
        .expect("load singleton")
        .expect("singleton row exists");
    let mut m: status_page::ActiveModel = existing.into();
    m.enabled = Set(enabled);
    m.server_ids_json = Set(serde_json::to_string(&server_ids).unwrap());
    m.show_server_detail = Set(show_server_detail);
    m.show_network = Set(show_network);
    m.show_ip_quality = Set(show_ip_quality);
    m.show_incidents = Set(show_incidents);
    m.show_maintenance = Set(show_maintenance);
    m.updated_at = Set(Utc::now());
    m.update(db).await.expect("update singleton");
}

async fn insert_public_incident(db: &sea_orm::DatabaseConnection, id: &str, title: &str) {
    let now = Utc::now();
    let m = incident::ActiveModel {
        id: Set(id.to_string()),
        title: Set(title.to_string()),
        status: Set("investigating".to_string()),
        severity: Set("minor".to_string()),
        server_ids_json: Set(None),
        is_public: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        resolved_at: Set(None),
    };
    m.insert(db).await.expect("insert incident");
}

async fn insert_public_maintenance(db: &sea_orm::DatabaseConnection, id: &str, title: &str) {
    let now = Utc::now();
    let m = maintenance::ActiveModel {
        id: Set(id.to_string()),
        title: Set(title.to_string()),
        description: Set(Some("scheduled".to_string())),
        start_at: Set(now),
        end_at: Set(now + chrono::Duration::hours(2)),
        server_ids_json: Set(None),
        is_public: Set(true),
        active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    };
    m.insert(db).await.expect("insert maintenance");
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn public_status_endpoints_return_200_when_fully_enabled() {
    let (base_url, state, _tmp) = start_test_server().await;
    let server_id = "srv-anon-1";

    insert_server(&state.db, server_id, "anon-server", None, None, false).await;
    insert_public_incident(&state.db, "inc-1", "Database hiccup").await;
    insert_public_maintenance(&state.db, "mnt-1", "Quarterly maintenance").await;

    configure_status_page(
        &state.db,
        true,
        vec![server_id.to_string()],
        true,
        true,
        true,
        true,
        true,
    )
    .await;

    // Seed a fresh metrics report into the agent_manager cache so the
    // `metrics` field on the summary is populated. is_online() requires an
    // entry in `connections` which only exists for live WS sessions, so we
    // skip that path — endpoints must still return 200 for offline servers.
    state.agent_manager.update_report(
        server_id,
        SystemReport {
            cpu: 12.5,
            mem_used: 8_000_000_000,
            disk_used: 30_000_000_000,
            ..Default::default()
        },
    );

    let client = anon_client();

    let now = Utc::now();
    let from = (now - chrono::Duration::hours(1)).to_rfc3339();
    let to = now.to_rfc3339();

    let endpoints: Vec<(&str, String)> = vec![
        ("/api/status/config", format!("{}/api/status/config", base_url)),
        ("/api/status", format!("{}/api/status", base_url)),
        (
            "/api/status/servers/{id}",
            format!("{}/api/status/servers/{}", base_url, server_id),
        ),
        (
            "/api/status/servers/{id}/metrics",
            {
                // reqwest will URL-encode the `:` etc. in the ISO-8601
                // strings via the `query()` builder; constructing a raw URL
                // string would break with `400 Bad Request` on the chrono
                // serde format. We build the encoded URL via `Url::parse_with_params`.
                let mut url = reqwest::Url::parse(&format!(
                    "{}/api/status/servers/{}/metrics",
                    base_url, server_id
                ))
                .unwrap();
                url.query_pairs_mut()
                    .append_pair("from", &from)
                    .append_pair("to", &to)
                    .append_pair("interval", "raw");
                url.to_string()
            },
        ),
        (
            "/api/status/servers/{id}/uptime-daily",
            format!("{}/api/status/servers/{}/uptime-daily", base_url, server_id),
        ),
        ("/api/status/network", format!("{}/api/status/network", base_url)),
        (
            "/api/status/network/{id}",
            format!("{}/api/status/network/{}", base_url, server_id),
        ),
        (
            "/api/status/ip-quality",
            format!("{}/api/status/ip-quality", base_url),
        ),
        (
            "/api/status/incidents",
            format!("{}/api/status/incidents", base_url),
        ),
        (
            "/api/status/maintenances",
            format!("{}/api/status/maintenances", base_url),
        ),
    ];

    for (label, url) in endpoints {
        let resp = client
            .get(&url)
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {label} failed transport: {e}"));
        assert_eq!(
            resp.status(),
            200,
            "expected 200 for {label} when fully enabled (got {})",
            resp.status()
        );
        // Body should be valid JSON-with-data envelope.
        let body: serde_json::Value = resp.json().await.expect("json body");
        assert!(
            body.get("data").is_some(),
            "{label} response missing top-level `data` field: {body:?}"
        );
    }

    // Sanity: incidents response is the documented {active, recent} shape.
    let resp = client
        .get(format!("{}/api/status/incidents", base_url))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"]["active"].is_array());
    assert!(body["data"]["recent"].is_array());

    // Suppress unused: json! import path is used elsewhere in the suite.
    let _ = json!({});
}
