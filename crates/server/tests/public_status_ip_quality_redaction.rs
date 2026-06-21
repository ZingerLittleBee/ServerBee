//! Integration test: IP-quality snapshot + unlock-result redaction on the
//! public surface.
//!
//! The auth'd IP-quality endpoint exposes every snapshot column (ip, asn,
//! abuse_email, etc.) and the full `detail` blob of an unlock result. The
//! public DTO defined in `service::public_status::PublicIpQualitySnapshot`
//! and `PublicUnlockResult` intentionally drops those fields. This test
//! locks that redaction in.

use std::time::Duration;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectOptions, ConnectionTrait, Database, EntityTrait, Set};
use sea_orm_migration::MigratorTrait;
use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::entity::{
    ip_quality_snapshot, server, status_page, unlock_result,
};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

// Sensitive substrings the public response body must NOT contain.
const SENSITIVE_IP: &str = "9.9.9.9";
const SENSITIVE_ASN: &str = "AS12345";
const SENSITIVE_AS_ORG: &str = "EvilCo";
const SENSITIVE_ABUSE_EMAIL: &str = "abuse@evilco";
const SENSITIVE_UNLOCK_DETAIL: &str = "secret error blob";

/// Keys that must NOT appear inside any `entries[*].ip_quality` sub-object.
/// Scoped to the snapshot — `unlock_results[*].region` legitimately retains
/// the service-specific unlock region (e.g. Netflix "US-NY").
const FORBIDDEN_SNAPSHOT_KEYS: &[&str] = &[
    "ip",
    "asn",
    "as_org",
    "region",
    "city",
    "is_proxy",
    "is_vpn",
    "is_hosting",
    "is_tor",
    "is_abuser",
    "is_mobile",
    "asn_abuser_score",
    "abuse_email",
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

async fn insert_minimal_server(db: &sea_orm::DatabaseConnection, id: &str) {
    let now = Utc::now();
    server::ActiveModel {
        id: Set(id.to_string()),
        token_hash: Set(None),
        token_prefix: Set(None),
        name: Set("ipq-target".to_string()),
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

async fn insert_snapshot(db: &sea_orm::DatabaseConnection, id: &str, server_id: &str) {
    let now = Utc::now();
    ip_quality_snapshot::ActiveModel {
        id: Set(id.to_string()),
        server_id: Set(server_id.to_string()),
        ip: Set(SENSITIVE_IP.to_string()),
        asn: Set(Some(SENSITIVE_ASN.to_string())),
        as_org: Set(Some(SENSITIVE_AS_ORG.to_string())),
        country: Set(Some("United States".to_string())),
        region: Set(Some("US-WA".to_string())),
        city: Set(Some("Seattle".to_string())),
        ip_type: Set("residential".to_string()),
        is_proxy: Set(true),
        is_vpn: Set(true),
        is_hosting: Set(true),
        risk_score: Set(Some(87)),
        risk_level: Set("high".to_string()),
        is_tor: Set(true),
        is_abuser: Set(true),
        is_mobile: Set(true),
        asn_abuser_score: Set(Some(50)),
        abuse_email: Set(Some(SENSITIVE_ABUSE_EMAIL.to_string())),
        checked_at: Set(now),
    }
    .insert(db)
    .await
    .expect("insert snapshot");
}

async fn insert_unlock_result(
    db: &sea_orm::DatabaseConnection,
    id: &str,
    server_id: &str,
    service_id: &str,
    region: Option<&str>,
    detail: Option<&str>,
) {
    let now = Utc::now();
    unlock_result::ActiveModel {
        id: Set(id.to_string()),
        server_id: Set(server_id.to_string()),
        service_id: Set(service_id.to_string()),
        status: Set("yes".to_string()),
        region: Set(region.map(String::from)),
        latency_ms: Set(Some(42)),
        detail: Set(detail.map(String::from)),
        checked_at: Set(now),
    }
    .insert(db)
    .await
    .expect("insert unlock result");
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
    m.show_ip_quality = Set(true);
    m.updated_at = Set(Utc::now());
    m.update(db).await.expect("update status page");
}

#[tokio::test]
async fn public_ip_quality_redacts_snapshot_and_unlock_detail() {
    let (base_url, state, _tmp) = start_test_server().await;
    let server_id = "srv-ipq-1";

    insert_minimal_server(&state.db, server_id).await;
    insert_snapshot(&state.db, "snap-1", server_id).await;
    // Netflix unlock result — public-facing `region` should be retained.
    insert_unlock_result(
        &state.db,
        "ur-1",
        server_id,
        "01960000-0000-7000-8000-000000000001", // netflix builtin id
        Some("US-NY"),
        Some(SENSITIVE_UNLOCK_DETAIL),
    )
    .await;

    enable_status_page(&state.db, vec![server_id.to_string()]).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let resp = client
        .get(format!("{}/api/status/ip-quality", base_url))
        .send()
        .await
        .expect("transport");
    let status = resp.status();
    let text = resp.text().await.expect("body");
    assert_eq!(status, 200, "expected 200, got {status}: {text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("json");

    let entries = body["data"]["entries"]
        .as_array()
        .expect("entries array");
    assert!(!entries.is_empty(), "expected at least one entry");

    // Find the entry for our server.
    let entry = entries
        .iter()
        .find(|e| e["server_id"] == server_id)
        .expect("entry for seeded server");

    // 1. Snapshot redaction — keys absent inside the `ip_quality` sub-object
    //    (NOT the full tree, because `unlock_results[*].region` is allowed).
    let snapshot = &entry["ip_quality"];
    assert!(
        !snapshot.is_null(),
        "snapshot should be present, got: {snapshot:?}"
    );
    let snap_obj = snapshot.as_object().expect("snapshot is object");
    for key in FORBIDDEN_SNAPSHOT_KEYS {
        assert!(
            !snap_obj.contains_key(*key),
            "snapshot must not contain forbidden key `{key}`; snap={snap_obj:?}"
        );
    }

    // 2. Unlock results must not include `detail`.
    let unlock_results = entry["unlock_results"]
        .as_array()
        .expect("unlock_results array");
    assert!(!unlock_results.is_empty(), "expected at least one unlock result");
    for ur in unlock_results {
        let ur_obj = ur.as_object().expect("unlock result is object");
        assert!(
            !ur_obj.contains_key("detail"),
            "unlock_result must not contain `detail` key; ur={ur:?}"
        );
    }

    // 3. Defense-in-depth: literal substrings must not appear anywhere.
    for needle in [
        SENSITIVE_IP,
        SENSITIVE_ASN,
        SENSITIVE_AS_ORG,
        SENSITIVE_ABUSE_EMAIL,
        SENSITIVE_UNLOCK_DETAIL,
    ] {
        assert!(
            !text.contains(needle),
            "response body must not contain sensitive substring `{needle}`: {text}"
        );
    }

    // 4. Service-specific unlock region IS retained for the netflix entry.
    let netflix = unlock_results
        .iter()
        .find(|ur| ur["service_id"] == "01960000-0000-7000-8000-000000000001")
        .expect("netflix unlock result");
    assert_eq!(
        netflix["region"], "US-NY",
        "service unlock region should be preserved"
    );
}
