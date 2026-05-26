//! Integration test: scope guard for `/api/status/*`.
//!
//! Spec rule (`PublicScope`): `id` ∈ status_page.server_ids ∧ servers row
//! exists ∧ hidden = false. Anything outside that intersection must surface
//! as 404 from per-id endpoints, and be filtered out of list endpoints.
//! Crucially, hidden-but-listed and nonexistent IDs both yield 404 — the
//! response shape must NOT distinguish them.

use std::time::Duration;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectOptions, ConnectionTrait, Database, EntityTrait, Set};
use sea_orm_migration::MigratorTrait;
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::entity::{ip_quality_snapshot, server, status_page};
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

async fn insert_server_with_hidden(
    db: &sea_orm::DatabaseConnection,
    id: &str,
    name: &str,
    hidden: bool,
) {
    let now = Utc::now();
    server::ActiveModel {
        id: Set(id.to_string()),
        token_hash: Set(None),
        token_prefix: Set(None),
        name: Set(name.to_string()),
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
        virtualization: Set(None),
        agent_version: Set(None),
        group_id: Set(None),
        weight: Set(0),
        hidden: Set(hidden),
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

async fn insert_minimal_snapshot(
    db: &sea_orm::DatabaseConnection,
    id: &str,
    server_id: &str,
) {
    ip_quality_snapshot::ActiveModel {
        id: Set(id.to_string()),
        server_id: Set(server_id.to_string()),
        ip: Set("0.0.0.0".to_string()),
        asn: Set(None),
        as_org: Set(None),
        country: Set(None),
        region: Set(None),
        city: Set(None),
        ip_type: Set("unknown".to_string()),
        is_proxy: Set(false),
        is_vpn: Set(false),
        is_hosting: Set(false),
        risk_score: Set(None),
        risk_level: Set("low".to_string()),
        is_tor: Set(false),
        is_abuser: Set(false),
        is_mobile: Set(false),
        asn_abuser_score: Set(None),
        abuse_email: Set(None),
        checked_at: Set(Utc::now()),
    }
    .insert(db)
    .await
    .expect("insert snapshot");
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

#[tokio::test]
async fn public_status_scope_guard_enforces_visibility_rules() {
    let (base_url, state, _tmp) = start_test_server().await;

    let id_a = "srv-A";
    let id_b = "srv-B";
    let id_h = "srv-H-hidden"; // listed but hidden=true → out-of-scope
    let id_z = "srv-Z-missing"; // listed but row absent → out-of-scope

    insert_server_with_hidden(&state.db, id_a, "alpha", false).await;
    insert_server_with_hidden(&state.db, id_b, "bravo", false).await;
    insert_server_with_hidden(&state.db, id_h, "hidden", true).await;
    // Do NOT insert id_z — exercise nonexistent path.

    // Snapshots for every "server-shaped" entity so the IP-quality endpoint
    // would expose them if scope were broken.
    insert_minimal_snapshot(&state.db, "snap-a", id_a).await;
    insert_minimal_snapshot(&state.db, "snap-b", id_b).await;
    insert_minimal_snapshot(&state.db, "snap-h", id_h).await;

    enable_status_page(
        &state.db,
        vec![
            id_a.to_string(),
            id_b.to_string(),
            id_h.to_string(),
            id_z.to_string(),
        ],
    )
    .await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // 1. /api/status — list should contain A and B but NOT H or Z.
    let list_resp = client
        .get(format!("{}/api/status", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let ids: Vec<String> = list_body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&id_a.to_string()), "A must be listed");
    assert!(ids.contains(&id_b.to_string()), "B must be listed");
    assert!(!ids.contains(&id_h.to_string()), "hidden H must NOT be listed");
    assert!(!ids.contains(&id_z.to_string()), "missing Z must NOT be listed");

    // 2. /api/status/servers/{H} → 404 (hidden out-of-scope).
    let resp = client
        .get(format!("{}/api/status/servers/{}", base_url, id_h))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "hidden server detail must 404");

    // 3. /api/status/servers/{Z} → 404 (nonexistent).
    let resp = client
        .get(format!("{}/api/status/servers/{}", base_url, id_z))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "missing server detail must 404");

    // 4. /api/status/network/{H} → 404 (hidden).
    let resp = client
        .get(format!("{}/api/status/network/{}", base_url, id_h))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "hidden server network must 404");

    // 5. /api/status/ip-quality → no entry with server_id = H or Z.
    let resp = client
        .get(format!("{}/api/status/ip-quality", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let entries = body["data"]["entries"]
        .as_array()
        .expect("entries array");
    for e in entries {
        let sid = e["server_id"].as_str().unwrap_or("");
        assert_ne!(sid, id_h, "hidden server must not appear in ip-quality");
        assert_ne!(sid, id_z, "missing server must not appear in ip-quality");
    }
}
