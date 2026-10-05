//! Billing-calendar edits through production HTTP with a real SQLite database.
mod common;
use common::{create_server, http_client, login_admin, start_test_server};
use serde_json::{Value, json};

#[tokio::test]
async fn manual_date_uses_complete_day_in_stored_billing_timezone() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "calendar-host").await;
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(
            &json!({"renewal":{"expiry_date":"2026-03-08","billing_timezone":"America/New_York"}}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["expired_at"], "2026-03-09T03:59:59.999999999Z");
    assert_eq!(body["data"]["renewal"]["expiry_date"], "2026-03-08");
    assert_eq!(
        body["data"]["renewal"]["billing_timezone"],
        "America/New_York"
    );
    assert_eq!(body["data"]["renewal"]["enabled"], false);
    assert_eq!(body["data"]["renewal"]["deadline_origin"], "confirmed");
}

#[tokio::test]
async fn onboarding_calendar_is_atomic_and_idempotent() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let request = json!({"onboarding_request_id":uuid::Uuid::new_v4().to_string(),"name":"date-create","renewal":{"expiry_date":"2026-01-31","billing_timezone":"Asia/Tokyo"}});
    let body: Value = admin
        .post(format!("{base}/api/servers"))
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = body["data"]["server_id"].as_str().unwrap();
    let detail: Value = admin
        .get(format!("{base}/api/servers/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        detail["data"]["expired_at"],
        "2026-01-31T14:59:59.999999999Z"
    );
    assert_eq!(detail["data"]["renewal"]["expiry_date"], "2026-01-31");
    let replay: Value = admin
        .post(format!("{base}/api/servers"))
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(replay["data"]["server_id"], id);
}

/// Reopen the on-disk database and serve production routes with fresh AppState.
async fn serve_database(path: &std::path::Path) -> String {
    use serverbee_server::{config::AppConfig, router::create_router, state::AppState};
    let db = sea_orm::Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .unwrap();
    let mut config = AppConfig::default();
    config.auth.secure_cookie = false;
    config.server.data_dir = path.parent().unwrap().to_str().unwrap().into();
    let state = AppState::new(db, config).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(
            listener,
            create_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    base
}

async fn predecessor_database() -> (tempfile::TempDir, std::path::PathBuf) {
    use sea_orm::ConnectionTrait;
    use sea_orm_migration::MigratorTrait;
    use serverbee_server::{migration::Migrator, service::auth::AuthService};
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("legacy.db");
    let db = sea_orm::Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .unwrap();
    let steps = Migrator::migrations()
        .iter()
        .position(|migration| migration.name() == "m20261005_000086_renewal_dates")
        .expect("renewal migration remains registered") as u32;
    Migrator::up(&db, Some(steps)).await.unwrap();
    let columns = db
        .query_all(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "PRAGMA table_info(servers)",
        ))
        .await
        .unwrap();
    assert!(
        !columns
            .iter()
            .any(|column| column.try_get::<String>("", "name").unwrap() == "renewal_state"),
        "fixture must predate renewal schema"
    );
    let user = AuthService::create_user(&db, "admin", "testpass", "admin")
        .await
        .unwrap();
    db.execute_unprepared("INSERT INTO servers (id, name, capabilities, protocol_version, expired_at, created_at, updated_at) VALUES ('legacy-host','legacy-host',56,1,'2026-04-18 17:42:13+00:00',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").await.unwrap();
    db.execute(sea_orm::Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite,
        "INSERT INTO server_onboarding_requests (id, actor_id, request_id, normalized_input_hash, server_id, created_at) VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)",
        ["legacy-request".into(), user.id.into(), "00000000-0000-4000-8000-000000000001".into(), "n9aQOl2YNKimgYTxqUR6lkxuN9Wj04TjkISmtiifE_M".into(), "legacy-host".into()])).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    db.close().await.unwrap();
    (tmp, path)
}

#[tokio::test]
async fn predecessor_migration_and_real_legacy_full_forms_preserve_instant_after_reopen() {
    let (_tmp, path) = predecessor_database().await;
    let base = serve_database(&path).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let original: Value = admin
        .get(format!("{base}/api/servers/legacy-host"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(original["data"]["expired_at"], "2026-04-18T17:42:13Z");
    assert_eq!(original["data"]["renewal"]["enabled"], false);
    assert_eq!(original["data"]["renewal"]["billing_timezone"], "UTC");
    for payload in [
        json!({"name":"ios-save","price":12.0,"expired_at":"2026-04-18T17:42:13Z","billing_cycle":"monthly"}),
        json!({"name":"web-save","price":13.0,"expired_at":"2026-04-18T00:00:00.000Z","billing_cycle":"monthly"}),
        json!({"name":"new-save","renewal":{"expiry_date":"2026-04-18","billing_timezone":"UTC"}}),
    ] {
        let response = admin
            .put(format!("{base}/api/servers/legacy-host"))
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["expired_at"], "2026-04-18T17:42:13Z");
        assert_eq!(
            body["data"]["renewal"]["confirmed_expired_at"],
            "2026-04-18T17:42:13Z"
        );
    }
    let reopened = serve_database(&path).await;
    let admin = http_client();
    login_admin(&admin, &reopened).await;
    let persisted: Value = admin
        .get(format!("{reopened}/api/servers/legacy-host"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(persisted["data"]["expired_at"], "2026-04-18T17:42:13Z");
    assert_eq!(persisted["data"]["renewal"]["expiry_date"], "2026-04-18");
}

#[tokio::test]
async fn nonexistent_iana_calendar_date_is_rejected_atomically() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "unchanged-name").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"name":"must-not-persist","renewal":{"expiry_date":"2011-12-30","billing_timezone":"Pacific/Apia"}}))
        .send().await.unwrap();
    assert_eq!(
        response.status(),
        422,
        "Apia skipped this complete calendar date"
    );
    let detail: Value = admin
        .get(format!("{base}/api/servers/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["data"]["name"], "unchanged-name");
    assert_eq!(detail["data"]["expired_at"], Value::Null);
}

#[tokio::test]
async fn invalid_calendar_inputs_cannot_partially_change_the_server() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "original").await;
    for renewal in [
        json!({"expiry_date":"2026-02-30","billing_timezone":"UTC"}),
        json!({"expiry_date":"2026-01-31","billing_timezone":"Mars/Olympus"}),
        json!({"expiry_date":"2026-1-31"}),
    ] {
        let response = admin
            .put(format!("{base}/api/servers/{id}"))
            .json(&json!({"name":"partial","renewal":renewal}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
    }
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"expiry_date":"2026-01-31"},"expired_at":"2026-01-31T00:00:00Z"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        422,
        "simultaneous date contracts are ambiguous"
    );
    let body: Value = admin
        .get(format!("{base}/api/servers/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["name"], "original");
    assert_eq!(body["data"]["expired_at"], Value::Null);
}

#[tokio::test]
async fn timezone_only_edit_keeps_the_calendar_date_and_operator_history() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "timezone-host").await;
    let original: Value = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(
            &json!({"renewal":{"expiry_date":"2026-11-01","billing_timezone":"America/New_York"}}),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        original["data"]["expired_at"],
        "2026-11-02T04:59:59.999999999Z"
    );
    let changed: Value = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"billing_timezone":"Asia/Tokyo"}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        changed["data"]["expired_at"],
        "2026-11-01T14:59:59.999999999Z"
    );
    assert_eq!(changed["data"]["renewal"]["expiry_date"], "2026-11-01");
    assert_eq!(
        changed["data"]["renewal"]["confirmed_expired_at"],
        original["data"]["expired_at"]
    );
    let cleared: Value = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"renewal":{"expiry_date":null}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cleared["data"]["expired_at"], Value::Null);
    assert_eq!(
        cleared["data"]["renewal"]["confirmed_expired_at"],
        Value::Null
    );
}

#[tokio::test]
async fn renewal_read_permissions_and_public_redaction_remain_unchanged() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "private-renewal").await;
    let response = admin.put(format!("{base}/api/servers/{id}"))
        .json(&json!({"price":99.0,"renewal":{"expiry_date":"2028-02-29","billing_timezone":"Asia/Tokyo"}})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let member = common::login_as_new_user(&admin, &base, "calendar-member", "member").await;
    let detail: Value = member
        .get(format!("{base}/api/servers/{id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["data"]["renewal"]["expiry_date"], "2028-02-29");
    assert_eq!(
        member
            .put(format!("{base}/api/servers/{id}"))
            .json(&json!({"renewal":{"expiry_date":null}}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        admin
            .put(format!("{base}/api/status-page"))
            .json(&json!({"enabled":true,"server_ids":[id],"show_server_detail":true}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let anonymous = http_client();
    for route in ["/api/status".into(), format!("/api/status/servers/{id}")] {
        let response = anonymous
            .get(format!("{base}{route}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body: Value = response.json().await.unwrap();
        let text = body.to_string();
        for private in [
            "renewal",
            "expired_at",
            "confirmed_expired_at",
            "billing_timezone",
            "price",
        ] {
            assert!(
                !text.contains(&format!("\"{private}\"")),
                "public response contains {private}: {body}"
            );
        }
    }
}

#[tokio::test]
async fn legacy_web_changed_date_is_a_calendar_date_even_west_of_utc() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let id = create_server(&admin, &base, "legacy-change").await;
    assert_eq!(admin.put(format!("{base}/api/servers/{id}")).json(&json!({"renewal":{"expiry_date":"2026-01-31","billing_timezone":"America/Los_Angeles"}})).send().await.unwrap().status(), 200);
    let response = admin
        .put(format!("{base}/api/servers/{id}"))
        .json(&json!({"expired_at":"2026-02-28T00:00:00.000Z"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["renewal"]["expiry_date"], "2026-02-28");
    assert_eq!(body["data"]["expired_at"], "2026-03-01T07:59:59.999999999Z");
}

#[tokio::test]
async fn predecessor_onboarding_retry_keeps_its_original_request_identity() {
    let (_tmp, path) = predecessor_database().await;
    let base = serve_database(&path).await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    let response = admin.post(format!("{base}/api/servers"))
        .json(&json!({"onboarding_request_id":"00000000-0000-4000-8000-000000000001","name":"legacy-host","expired_at":"2026-04-18T17:42:13Z"}))
        .send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["server_id"], "legacy-host");
    assert!(body["data"]["enrollment"].is_null());
}

#[tokio::test]
async fn invalid_or_ambiguous_onboarding_calendar_has_no_durable_side_effects() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    for fields in [
        json!({"renewal":{"expiry_date":"2026-02-30"}}),
        json!({"renewal":{"expiry_date":"2026-01-31","billing_timezone":"not-a-timezone"}}),
        json!({"renewal":{"expiry_date":"2026-01-31"},"expired_at":"2026-01-31T00:00:00Z"}),
    ] {
        let mut payload = fields;
        payload["name"] = json!("invalid-create");
        payload["onboarding_request_id"] = json!(uuid::Uuid::new_v4().to_string());
        let response = admin
            .post(format!("{base}/api/servers"))
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
    }
    let body: Value = admin
        .get(format!("{base}/api/servers"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"], json!([]));
}

#[tokio::test]
async fn combined_timezone_and_date_edit_validates_only_the_final_calendar() {
    let (base, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base).await;
    for date in [json!("2011-12-31"), Value::Null] {
        let id = create_server(&admin, &base, "combined-calendar").await;
        assert_eq!(
            admin
                .put(format!("{base}/api/servers/{id}"))
                .json(&json!({"renewal":{"expiry_date":"2011-12-30","billing_timezone":"UTC"}}))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        let response = admin
            .put(format!("{base}/api/servers/{id}"))
            .json(&json!({"renewal":{"expiry_date":date,"billing_timezone":"Pacific/Apia"}}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            200,
            "the replaced UTC date need not exist in the new zone"
        );
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["data"]["renewal"]["expiry_date"], date);
        assert_eq!(body["data"]["renewal"]["billing_timezone"], "Pacific/Apia");
        if date.is_null() {
            assert_eq!(body["data"]["expired_at"], Value::Null);
        } else {
            assert_eq!(body["data"]["expired_at"], "2011-12-31T09:59:59.999999999Z");
        }
    }
}
