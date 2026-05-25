use std::time::Duration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};

use serverbee_server::config::{AppConfig, AuthConfig, DatabaseConfig, ServerConfig};
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::state::AppState;

/// Build the absolute path to the test SQLite file for direct DB access in
/// tests that need to mutate state the public API does not expose (e.g.
/// expiring an enrollment).
fn db_path_for(tmp: &tempfile::TempDir) -> String {
    format!("{}/test.db", tmp.path().to_str().unwrap())
}

async fn start_test_server_with_cap(max_servers: u32) -> (String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
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
            max_servers,
        },
        ..AppConfig::default()
    };

    let db_path = format!("{}/test.db", data_dir);
    let db_url = format!("sqlite://{}?mode=rwc", db_path);
    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(5);
    opt.sqlx_logging(false);

    let db = Database::connect(opt)
        .await
        .expect("Failed to connect to test database");

    db.execute_unprepared("PRAGMA journal_mode=WAL")
        .await
        .expect("Failed to set journal mode");
    db.execute_unprepared("PRAGMA foreign_keys=ON")
        .await
        .expect("Failed to enable foreign keys");

    Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");

    AuthService::create_user(&db, "admin", "testpass", "admin")
        .await
        .expect("Failed to seed admin");

    let state = AppState::new(db, config)
        .await
        .expect("Failed to create AppState");
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().expect("Failed to read listener addr");
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .expect("Test server failed");
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    (base_url, tmp)
}

async fn start_test_server() -> (String, tempfile::TempDir) {
    start_test_server_with_cap(0).await
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
}

async fn login_admin(client: &reqwest::Client, base_url: &str) {
    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({
            "username": "admin",
            "password": "testpass"
        }))
        .send()
        .await
        .expect("Login request failed");

    assert_eq!(resp.status(), 200, "Login should succeed");
}

#[tokio::test]
async fn post_servers_creates_pending_with_bound_enrollment() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": "vps-1"}))
        .send()
        .await
        .expect("create server request failed");
    assert_eq!(resp.status(), 200, "POST /api/servers should succeed");

    let body: Value = resp.json().await.expect("Failed to parse response");
    let data = &body["data"];
    assert!(data["server_id"].is_string(), "server_id must be a string");

    let enrollment = &data["enrollment"];
    assert!(enrollment["id"].is_string(), "enrollment.id must be a string");

    let code = enrollment["code"]
        .as_str()
        .expect("enrollment.code must be a string");
    assert!(
        code.len() >= 16,
        "enrollment code length should be >= 16, got {}",
        code.len()
    );

    let code_prefix = enrollment["code_prefix"]
        .as_str()
        .expect("enrollment.code_prefix must be a string");
    assert_eq!(&code[..8], code_prefix, "code prefix mismatch");

    let expires_at = enrollment["expires_at"]
        .as_str()
        .expect("expires_at must be a string");
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .expect("expires_at must parse as RFC3339");
}

#[tokio::test]
async fn post_servers_with_full_metadata_persists_tags_and_billing() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({
            "name": "vps-prod",
            "tags": ["db", "prod"],
            "remark": "primary",
            "public_remark": "edge",
            "price": 5.0,
            "currency": "USD",
            "billing_cycle": "monthly",
            "billing_start_day": 1,
            "traffic_limit": 1099511627776_i64,
            "traffic_limit_type": "sum"
        }))
        .send()
        .await
        .expect("create server request failed");
    assert_eq!(resp.status(), 200, "POST /api/servers should succeed");
    let body: Value = resp.json().await.expect("Failed to parse response");
    let server_id = body["data"]["server_id"]
        .as_str()
        .expect("server_id")
        .to_string();

    let get_resp = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("get server request failed");
    assert_eq!(get_resp.status(), 200);
    let get_body: Value = get_resp.json().await.expect("Failed to parse get response");
    let data = &get_body["data"];
    assert_eq!(data["price"], 5.0);
    assert_eq!(data["currency"], "USD");
    assert_eq!(data["billing_cycle"], "monthly");
    assert_eq!(data["billing_start_day"], 1);
    assert_eq!(data["traffic_limit"], 1099511627776_i64);
    assert_eq!(data["traffic_limit_type"], "sum");
    assert_eq!(data["remark"], "primary");
    assert_eq!(data["public_remark"], "edge");

    let tags_resp = client
        .get(format!("{}/api/servers/{}/tags", base_url, server_id))
        .send()
        .await
        .expect("get tags request failed");
    assert_eq!(tags_resp.status(), 200);
    let tags_body: Value = tags_resp.json().await.expect("Failed to parse tags");
    let mut tags: Vec<String> = tags_body["data"]
        .as_array()
        .expect("tags should be array")
        .iter()
        .map(|t| t.as_str().expect("tag should be string").to_string())
        .collect();
    tags.sort();
    assert_eq!(tags, vec!["db".to_string(), "prod".to_string()]);
}

#[tokio::test]
async fn post_servers_respects_max_servers_cap() {
    let (base_url, _tmp) = start_test_server_with_cap(1).await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let first = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": "first"}))
        .send()
        .await
        .expect("first create failed");
    assert_eq!(first.status(), 200, "first create should succeed");

    let second = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": "second"}))
        .send()
        .await
        .expect("second create failed");
    assert_eq!(
        second.status(),
        400,
        "second create should hit max_servers cap"
    );
}

#[tokio::test]
async fn get_server_returns_has_token_and_outstanding_enrollment() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": "vps-pending"}))
        .send()
        .await
        .expect("create server request failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("Failed to parse response");
    let server_id = body["data"]["server_id"]
        .as_str()
        .expect("server_id")
        .to_string();
    let returned_code_prefix = body["data"]["enrollment"]["code_prefix"]
        .as_str()
        .expect("code_prefix")
        .to_string();
    let returned_enrollment_id = body["data"]["enrollment"]["id"]
        .as_str()
        .expect("enrollment id")
        .to_string();

    let get_resp = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("get server request failed");
    assert_eq!(get_resp.status(), 200);
    let get_body: Value = get_resp.json().await.expect("Failed to parse get response");
    let data = &get_body["data"];

    assert_eq!(data["has_token"], false, "pending server must have has_token=false");

    let outstanding = &data["outstanding_enrollment"];
    assert!(
        outstanding.is_object(),
        "outstanding_enrollment must be present for pending server"
    );
    assert_eq!(outstanding["id"], returned_enrollment_id);
    assert_eq!(outstanding["code_prefix"], returned_code_prefix);
    assert!(outstanding["expires_at"].is_string());
    assert!(outstanding["created_at"].is_string());

    // Plaintext code must NOT leak through the GET endpoint.
    assert!(
        outstanding.get("code").is_none(),
        "GET response must not include plaintext code"
    );
}

/// Helper: POST /api/servers and return (server_id, enrollment_id, code).
async fn create_pending_server(
    client: &reqwest::Client,
    base_url: &str,
    name: &str,
) -> (String, String, String) {
    let resp = client
        .post(format!("{}/api/servers", base_url))
        .json(&json!({"name": name}))
        .send()
        .await
        .expect("create server request failed");
    assert_eq!(resp.status(), 200, "POST /api/servers should succeed");
    let body: Value = resp.json().await.expect("Failed to parse response");
    let server_id = body["data"]["server_id"]
        .as_str()
        .expect("server_id")
        .to_string();
    let enrollment_id = body["data"]["enrollment"]["id"]
        .as_str()
        .expect("enrollment.id")
        .to_string();
    let code = body["data"]["enrollment"]["code"]
        .as_str()
        .expect("enrollment.code")
        .to_string();
    (server_id, enrollment_id, code)
}

#[tokio::test]
async fn agent_register_updates_bound_server_does_not_create() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, _enrollment_id, code) =
        create_pending_server(&client, &base_url, "vps-bound").await;

    // Count servers before register.
    let list_before: Value = client
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .expect("list before failed")
        .json()
        .await
        .expect("parse list before");
    let count_before = list_before["data"]
        .as_array()
        .expect("list data is array")
        .len();

    // Anonymous client (no admin cookies) using the enrollment as Bearer.
    let anon = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("anon client");
    let resp = anon
        .post(format!("{}/api/agent/register", base_url))
        .bearer_auth(&code)
        .json(&json!({"fingerprint": ""}))
        .send()
        .await
        .expect("register request failed");
    assert_eq!(resp.status(), 200, "register should succeed");
    let body: Value = resp.json().await.expect("parse register response");
    assert_eq!(
        body["data"]["server_id"].as_str().expect("server_id"),
        server_id,
        "register must return the bound server_id, not create a new one",
    );
    assert!(
        body["data"]["token"]
            .as_str()
            .map(|t| !t.is_empty())
            .unwrap_or(false),
        "register must return a non-empty token",
    );

    // No new server row was created.
    let list_after: Value = client
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .expect("list after failed")
        .json()
        .await
        .expect("parse list after");
    let count_after = list_after["data"]
        .as_array()
        .expect("list data is array")
        .len();
    assert_eq!(
        count_after, count_before,
        "register must NOT create a new server row",
    );

    // The bound server now has a token; outstanding_enrollment is gone.
    let get_resp: Value = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("get server failed")
        .json()
        .await
        .expect("parse get server");
    assert_eq!(
        get_resp["data"]["has_token"], true,
        "registered server must have_token=true",
    );
    assert!(
        get_resp["data"]["outstanding_enrollment"].is_null(),
        "consumed enrollment should not show as outstanding: {:?}",
        get_resp["data"]["outstanding_enrollment"],
    );
}

#[tokio::test]
async fn agent_register_with_revoked_code_returns_401_and_does_not_set_token() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, enrollment_id, code) =
        create_pending_server(&client, &base_url, "vps-revoked").await;

    // Revoke via the existing admin DELETE endpoint (T6 maps DELETE to revoke).
    let revoke_resp = client
        .delete(format!(
            "{}/api/agent/enrollments/{}",
            base_url, enrollment_id
        ))
        .send()
        .await
        .expect("revoke request failed");
    assert_eq!(revoke_resp.status(), 200, "revoke should succeed");

    let anon = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("anon client");
    let resp = anon
        .post(format!("{}/api/agent/register", base_url))
        .bearer_auth(&code)
        .json(&json!({"fingerprint": ""}))
        .send()
        .await
        .expect("register request failed");
    assert_eq!(
        resp.status(),
        401,
        "register with revoked code must return 401",
    );

    // Server stays pending: no token, outstanding stays missing (we revoked it).
    let get_resp: Value = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("get server failed")
        .json()
        .await
        .expect("parse get server");
    assert_eq!(
        get_resp["data"]["has_token"], false,
        "revoked-code register must not stamp a token onto the bound server",
    );
}

#[tokio::test]
async fn agent_register_with_expired_code_returns_401() {
    let (base_url, tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (_server_id, enrollment_id, code) =
        create_pending_server(&client, &base_url, "vps-expired").await;

    // Flip the enrollment's expires_at to the epoch directly via SQLite, since
    // no public endpoint can force expiry.
    let db_url = format!("sqlite://{}?mode=rw", db_path_for(&tmp));
    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(2);
    opt.sqlx_logging(false);
    let db = Database::connect(opt).await.expect("connect test db");
    let stmt = sea_orm::Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Sqlite,
        "UPDATE agent_enrollments SET expires_at = ? WHERE id = ?",
        [
            "1970-01-01T00:00:00+00:00".into(),
            enrollment_id.clone().into(),
        ],
    );
    db.execute(stmt).await.expect("force-expire enrollment");
    drop(db);

    let anon = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("anon client");
    let resp = anon
        .post(format!("{}/api/agent/register", base_url))
        .bearer_auth(&code)
        .json(&json!({"fingerprint": ""}))
        .send()
        .await
        .expect("register request failed");
    assert_eq!(
        resp.status(),
        401,
        "register with expired code must return 401",
    );
}

#[tokio::test]
async fn agent_register_records_fingerprint_does_not_dedup() {
    let (base_url, tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Two pending servers, each with its own bound enrollment.
    let (server_a, _, code_a) = create_pending_server(&client, &base_url, "vps-a").await;
    let (server_b, _, code_b) = create_pending_server(&client, &base_url, "vps-b").await;

    let fp: String = "a".repeat(64);
    let anon = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("anon client");

    let resp_a = anon
        .post(format!("{}/api/agent/register", base_url))
        .bearer_auth(&code_a)
        .json(&json!({"fingerprint": fp}))
        .send()
        .await
        .expect("register A failed");
    assert_eq!(resp_a.status(), 200, "register A must succeed");
    let body_a: Value = resp_a.json().await.expect("parse A");
    assert_eq!(body_a["data"]["server_id"].as_str().unwrap(), server_a);

    // Register B with the SAME fingerprint — must NOT be silently mapped to A.
    let resp_b = anon
        .post(format!("{}/api/agent/register", base_url))
        .bearer_auth(&code_b)
        .json(&json!({"fingerprint": fp}))
        .send()
        .await
        .expect("register B failed");
    assert_eq!(resp_b.status(), 200, "register B must succeed");
    let body_b: Value = resp_b.json().await.expect("parse B");
    assert_eq!(
        body_b["data"]["server_id"].as_str().unwrap(),
        server_b,
        "B must return B's id, not be deduped onto A",
    );

    // Both rows present and both have tokens via the REST list.
    let list: Value = client
        .get(format!("{}/api/servers", base_url))
        .send()
        .await
        .expect("list failed")
        .json()
        .await
        .expect("parse list");
    let arr = list["data"].as_array().expect("list array");
    assert_eq!(arr.len(), 2, "both servers must remain in the list");
    for s in arr {
        assert_eq!(s["has_token"], true, "both rows must have_token=true");
    }

    // Verify both rows persist the supplied fingerprint at the DB level —
    // the public ServerResponse intentionally does not expose `fingerprint`,
    // so we read it directly. The key invariant: no fingerprint dedup, so
    // BOTH rows carry the same fingerprint value.
    let db_url = format!("sqlite://{}?mode=ro", db_path_for(&tmp));
    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(2);
    opt.sqlx_logging(false);
    let db = Database::connect(opt).await.expect("connect test db");
    for sid in [&server_a, &server_b] {
        let stmt = sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT fingerprint FROM servers WHERE id = ?",
            [sid.clone().into()],
        );
        let row = db
            .query_one(stmt)
            .await
            .expect("query")
            .expect("row must exist");
        let stored: Option<String> = row.try_get("", "fingerprint").expect("fingerprint col");
        assert_eq!(
            stored.as_deref(),
            Some(fp.as_str()),
            "server {sid} must record the supplied fingerprint (no dedup)",
        );
    }
}

#[tokio::test]
async fn agent_register_with_invalid_fingerprint_format_returns_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let (server_id, enrollment_id, code) =
        create_pending_server(&client, &base_url, "vps-badfp").await;

    let anon = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("anon client");
    let resp = anon
        .post(format!("{}/api/agent/register", base_url))
        .bearer_auth(&code)
        .json(&json!({"fingerprint": "short"}))
        .send()
        .await
        .expect("register request failed");
    assert_eq!(
        resp.status(),
        400,
        "invalid fingerprint format must be rejected",
    );

    // Server stays pending: no token.
    let get_resp: Value = client
        .get(format!("{}/api/servers/{}", base_url, server_id))
        .send()
        .await
        .expect("get server failed")
        .json()
        .await
        .expect("parse get server");
    assert_eq!(
        get_resp["data"]["has_token"], false,
        "invalid fingerprint must not stamp a token onto the bound server",
    );
    let outstanding = &get_resp["data"]["outstanding_enrollment"];
    assert!(
        outstanding.is_object(),
        "invalid fingerprint must NOT burn the code: outstanding enrollment must remain",
    );
    assert_eq!(
        outstanding["id"].as_str().unwrap(),
        enrollment_id,
        "outstanding enrollment should still be the original one",
    );
}
