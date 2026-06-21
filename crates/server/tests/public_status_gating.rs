//! Integration test: per-sub-page `show_*` toggles gate their respective
//! endpoints; `enabled = false` gates the entire surface except
//! `/api/status/config`.

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

async fn insert_minimal_server(db: &sea_orm::DatabaseConnection, id: &str) {
    let now = Utc::now();
    server::ActiveModel {
        id: Set(id.to_string()),
        token_hash: Set(None),
        token_prefix: Set(None),
        name: Set("gating-target".to_string()),
        cpu_name: Set(None),
        cpu_cores: Set(None),
        cpu_arch: Set(None),
        os: Set(None),
        kernel_version: Set(None),
        mem_total: Set(None),
        swap_total: Set(None),
        disk_total: Set(None),
        ipv4: Set(None),
        ipv6: Set(None),
        region: Set(None),
        country_code: Set(None),
        geo_manual: Set(false),
        virtualization: Set(None),
        agent_version: Set(None),
        group_id: Set(None),
        weight: Set(0),
        hidden: Set(false),
        remark: Set(None),
        public_remark: Set(None),
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

/// Hydrate the singleton with `enabled=true`, all `show_*=true`, and the
/// given scope. Individual sub-tests then flip one toggle at a time.
async fn set_all_on(db: &sea_orm::DatabaseConnection, server_ids: Vec<String>) {
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

/// Patch a single column on the singleton.
async fn patch_toggle(db: &sea_orm::DatabaseConnection, field: &str, value: bool) {
    let existing = status_page::Entity::find()
        .one(db)
        .await
        .unwrap()
        .expect("singleton row");
    let mut m: status_page::ActiveModel = existing.into();
    match field {
        "enabled" => m.enabled = Set(value),
        "show_server_detail" => m.show_server_detail = Set(value),
        "show_network" => m.show_network = Set(value),
        "show_ip_quality" => m.show_ip_quality = Set(value),
        "show_incidents" => m.show_incidents = Set(value),
        "show_maintenance" => m.show_maintenance = Set(value),
        other => panic!("unknown toggle: {other}"),
    }
    m.updated_at = Set(Utc::now());
    m.update(db).await.expect("update toggle");
}

async fn status(client: &reqwest::Client, url: &str) -> u16 {
    client
        .get(url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url} transport: {e}"))
        .status()
        .as_u16()
}

/// Endpoints that should toggle 200/403 together with a given `show_*` flag.
fn endpoints_for_toggle(base: &str, server_id: &str, toggle: &str) -> Vec<String> {
    match toggle {
        "show_server_detail" => vec![
            format!("{}/api/status/servers/{}", base, server_id),
            // The metrics endpoint needs from/to query params, otherwise we'd
            // hit a 400 before the gate. Use a wide range that's always valid.
            {
                let mut u = reqwest::Url::parse(&format!(
                    "{}/api/status/servers/{}/metrics",
                    base, server_id
                ))
                .unwrap();
                u.query_pairs_mut()
                    .append_pair("from", "2020-01-01T00:00:00Z")
                    .append_pair("to", "2030-01-01T00:00:00Z")
                    .append_pair("interval", "raw");
                u.to_string()
            },
        ],
        "show_network" => vec![
            format!("{}/api/status/network", base),
            format!("{}/api/status/network/{}", base, server_id),
        ],
        "show_ip_quality" => vec![format!("{}/api/status/ip-quality", base)],
        "show_incidents" => vec![format!("{}/api/status/incidents", base)],
        "show_maintenance" => vec![format!("{}/api/status/maintenances", base)],
        other => panic!("unknown toggle: {other}"),
    }
}

#[tokio::test]
async fn public_status_subpage_toggles_gate_endpoints() {
    let (base_url, state, _tmp) = start_test_server().await;
    let server_id = "srv-gating-1";

    insert_minimal_server(&state.db, server_id).await;
    set_all_on(&state.db, vec![server_id.to_string()]).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // Iterate (toggle, endpoints) pairs.
    for toggle in [
        "show_server_detail",
        "show_network",
        "show_ip_quality",
        "show_incidents",
        "show_maintenance",
    ] {
        // Toggle off → endpoints 403.
        patch_toggle(&state.db, toggle, false).await;
        for url in endpoints_for_toggle(&base_url, server_id, toggle) {
            let s = status(&client, &url).await;
            assert_eq!(
                s, 403,
                "with {toggle}=false, expected 403 from {url}, got {s}"
            );
        }
        // Toggle back on → endpoints 200.
        patch_toggle(&state.db, toggle, true).await;
        for url in endpoints_for_toggle(&base_url, server_id, toggle) {
            let s = status(&client, &url).await;
            assert_eq!(
                s, 200,
                "with {toggle}=true, expected 200 from {url}, got {s}"
            );
        }
    }
}

#[tokio::test]
async fn public_status_disabled_returns_403_everywhere_except_config() {
    let (base_url, state, _tmp) = start_test_server().await;
    let server_id = "srv-disabled-1";

    insert_minimal_server(&state.db, server_id).await;
    // Set all show_* on but enabled=false.
    set_all_on(&state.db, vec![server_id.to_string()]).await;
    patch_toggle(&state.db, "enabled", false).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // /api/status/config must remain reachable so the SPA can render a
    // "site disabled" notice.
    let config_resp = client
        .get(format!("{}/api/status/config", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(
        config_resp.status(),
        200,
        "/api/status/config must remain 200 when enabled=false"
    );
    let body: serde_json::Value = config_resp.json().await.unwrap();
    assert_eq!(
        body["data"]["enabled"], false,
        "config endpoint should reflect enabled=false"
    );

    // Every other public endpoint must 403.
    let now = Utc::now();
    let from = (now - chrono::Duration::hours(1)).to_rfc3339();
    let to = now.to_rfc3339();
    let metrics_url = {
        let mut u =
            reqwest::Url::parse(&format!("{}/api/status/servers/{}/metrics", base_url, server_id))
                .unwrap();
        u.query_pairs_mut()
            .append_pair("from", &from)
            .append_pair("to", &to)
            .append_pair("interval", "raw");
        u.to_string()
    };

    let other_endpoints = [
        format!("{}/api/status", base_url),
        format!("{}/api/status/servers/{}", base_url, server_id),
        metrics_url,
        format!("{}/api/status/servers/{}/uptime-daily", base_url, server_id),
        format!("{}/api/status/network", base_url),
        format!("{}/api/status/network/{}", base_url, server_id),
        format!("{}/api/status/ip-quality", base_url),
        format!("{}/api/status/incidents", base_url),
        format!("{}/api/status/maintenances", base_url),
    ];

    for url in other_endpoints {
        let s = status(&client, &url).await;
        assert_eq!(
            s, 403,
            "with enabled=false, expected 403 from {url}, got {s}"
        );
    }
}
