//! Recovery is deletion-only and operates on the original persisted identity.
use sea_orm::{ConnectionTrait, EntityTrait};
use serde_json::{Value, json};
use serverbee_server::{
    entity::{mobile_session, user},
    service::auth::AuthService,
};

use super::{
    INST_ID, http_client, legacy_mobile_server, mobile_admin_token, mobile_login, register_mobile,
    registration_db, start_test_server,
};

fn recovery_request(tokens: &Value) -> Value {
    json!({
        "username": "admin", "password": "testpass",
        "expected_user_id": tokens["data"]["user"]["id"],
        "installation_id": INST_ID,
        "expected_session_id": tokens["data"].get("mobile_session_id"),
        "access_token": tokens["data"]["access_token"],
        "refresh_token": tokens["data"]["refresh_token"],
        "revocation_token": tokens["data"].get("revocation_token"),
    })
}

async fn recover(client: &reqwest::Client, base: &str, body: &Value) -> reqwest::Response {
    client
        .post(format!("{base}/api/mobile/auth/recover"))
        .json(body)
        .send()
        .await
        .unwrap()
}

async fn expire_original(db: &sea_orm::DatabaseConnection) {
    db.execute_unprepared("UPDATE sessions SET expires_at='2000-01-01 00:00:00'; UPDATE mobile_sessions SET expires_at='2000-01-01 00:00:00'").await.unwrap();
}

async fn assert_exists(db: &sea_orm::DatabaseConnection, id: &str) {
    assert!(
        mobile_session::Entity::find_by_id(id)
            .one(db)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn all_401_legacy_cleanup_requires_explicit_selection_then_deletes_exact_original() {
    let (base, tmp, original) = legacy_mobile_server().await;
    let client = http_client();
    let db = registration_db(&tmp).await;
    assert_eq!(
        register_mobile(&client, &base, &original).await.status(),
        200
    );
    let target = mobile_session::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    db.execute_unprepared("INSERT INTO mobile_push_registrations (installation_id,user_id,mobile_session_id,updated_at) SELECT installation_id,user_id,id,CURRENT_TIMESTAMP FROM mobile_sessions").await.unwrap();
    expire_original(&db).await;
    // One captured pre-recovery identity drives every failing cleanup path and
    // recovery itself. Logout's rejected authentication removes its expired
    // access mapping; its retained refresh has no current match or proof.
    let mut captured = original.clone();
    captured["data"]["refresh_token"] = json!("discarded-refresh-secret");
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/auth/logout"))
            .bearer_auth(captured["data"]["access_token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        super::refresh_mobile(&client, &base, &captured)
            .await
            .status(),
        401
    );
    assert_eq!(
        client
            .post(format!("{base}/api/mobile/auth/revoke"))
            .json(&json!({"installation_id": INST_ID,"revocation_token":captured["data"]["refresh_token"]}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let mut body = recovery_request(&captured);
    let offered = recover(&client, &base, &body).await;
    assert_eq!(offered.status(), 200);
    let offered = offered.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(offered["outcome"], "selection_required");
    assert_eq!(offered["mobile_session_id"], Value::Null);
    assert_eq!(
        offered["candidates"],
        json!([{
            "mobile_session_id": target.id, "device_name": target.device_name,
            "created_at": target.created_at.to_rfc3339(), "last_used_at": target.last_used_at.to_rfc3339(),
        }])
    );
    assert_exists(&db, &target.id).await;
    for table in ["device_tokens", "mobile_push_registrations"] {
        let count = db
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!("SELECT COUNT(*) AS n FROM {table}"),
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "n")
            .unwrap();
        assert_eq!(count, 1, "Offering selection cannot revoke {table}");
    }
    body["expected_session_id"] = json!(target.id);
    let result = recover(&client, &base, &body).await;
    assert_eq!(result.status(), 200);
    let data = result.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(data["outcome"], "ok");
    assert_eq!(data["candidates"], json!([]));
    assert_eq!(data["mobile_session_id"], target.id);
    assert_eq!(data["user_id"], original["data"]["user"]["id"]);
    assert!(data.get("access_token").is_none());
    assert!(data.get("refresh_token").is_none());
    assert!(
        mobile_session::Entity::find()
            .one(&db)
            .await
            .unwrap()
            .is_none()
    );
    for table in [
        "sessions",
        "device_tokens",
        "mobile_push_registrations",
        "mobile_session_revocation_proofs",
    ] {
        let count = db
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!("SELECT COUNT(*) AS n FROM {table}"),
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "n")
            .unwrap();
        assert_eq!(count, 0, "{table} must lose only original authority");
    }
}

#[tokio::test]
async fn expired_legacy_current_refresh_identifies_original_without_access_mapping() {
    let (base, tmp, original) = legacy_mobile_server().await;
    let db = registration_db(&tmp).await;
    expire_original(&db).await;
    db.execute_unprepared("DELETE FROM sessions").await.unwrap();
    let response = recover(&http_client(), &base, &recovery_request(&original)).await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.unwrap()["data"]["outcome"],
        "ok"
    );
    assert!(
        mobile_session::Entity::find()
            .one(&db)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn stable_and_consumed_revocation_material_identifies_lost_rotation() {
    for use_history in [false, true] {
        let (base, tmp) = start_test_server().await;
        let client = http_client();
        let original = mobile_admin_token(&client, &base, INST_ID).await;
        assert_eq!(
            super::refresh_mobile(&client, &base, &original)
                .await
                .status(),
            200
        );
        let db = registration_db(&tmp).await;
        expire_original(&db).await;
        let mut body = recovery_request(&original);
        body["expected_session_id"] = Value::Null;
        if use_history {
            body["revocation_token"] = Value::Null;
        }
        let result = recover(&client, &base, &body).await;
        assert_eq!(result.status(), 200);
        assert_eq!(
            result.json::<Value>().await.unwrap()["data"]["mobile_session_id"],
            original["data"]["mobile_session_id"]
        );
    }
}

#[tokio::test]
async fn exact_id_retry_and_concurrent_recovery_preserve_replacement_and_other_device() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let replacement = mobile_admin_token(&client, &base, INST_ID).await;
    let other = mobile_admin_token(&client, &base, "other-installation").await;
    let body = recovery_request(&original);
    let (a, b) = tokio::join!(
        recover(&client, &base, &body),
        recover(&client, &base, &body)
    );
    assert_eq!(a.status(), 200);
    assert_eq!(b.status(), 200);
    let mut outcomes = vec![
        a.json::<Value>().await.unwrap()["data"]["outcome"]
            .as_str()
            .unwrap()
            .to_string(),
        b.json::<Value>().await.unwrap()["data"]["outcome"]
            .as_str()
            .unwrap()
            .to_string(),
    ];
    outcomes.sort();
    assert_eq!(outcomes, ["already_absent", "ok"]);
    let db = registration_db(&tmp).await;
    assert_exists(
        &db,
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
    assert_exists(&db, other["data"]["mobile_session_id"].as_str().unwrap()).await;
    assert_eq!(
        super::refresh_mobile(&client, &base, &replacement)
            .await
            .status(),
        200
    );
}

#[tokio::test]
async fn idless_confirmed_absence_then_sole_replacement_requires_explicit_selection() {
    let (base, tmp, original) = legacy_mobile_server().await;
    let client = http_client();
    let body = recovery_request(&original);
    assert_eq!(recover(&client, &base, &body).await.status(), 200);
    let absent = recover(&client, &base, &body).await;
    assert_eq!(absent.status(), 200);
    let data = absent.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(data["outcome"], "already_absent");
    assert_eq!(data["mobile_session_id"], Value::Null);
    let replacement = mobile_admin_token(&client, &base, INST_ID).await;
    let response = recover(&client, &base, &body).await;
    assert_eq!(response.status(), 200);
    let data = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(data["outcome"], "selection_required");
    assert_eq!(data["mobile_session_id"], Value::Null);
    assert_eq!(data["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(
        data["candidates"][0]["mobile_session_id"],
        replacement["data"]["mobile_session_id"]
    );
    let db = registration_db(&tmp).await;
    assert_exists(
        &db,
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn conflicting_captured_identities_never_choose_one_session() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let replacement = mobile_admin_token(&client, &base, INST_ID).await;
    let mut body = recovery_request(&original);
    body["expected_session_id"] = Value::Null;
    body["revocation_token"] = replacement["data"]["revocation_token"].clone();
    assert_eq!(recover(&client, &base, &body).await.status(), 409);
    let db = registration_db(&tmp).await;
    assert_exists(&db, original["data"]["mobile_session_id"].as_str().unwrap()).await;
    assert_exists(
        &db,
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn bad_credentials_or_claimed_identity_preserve_original() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    for (field, value) in [
        ("password", json!("wrong-password")),
        ("expected_user_id", json!(uuid::Uuid::new_v4().to_string())),
        ("installation_id", json!("wrong-installation")),
    ] {
        let mut body = recovery_request(&original);
        body[field] = value;
        assert_eq!(recover(&client, &base, &body).await.status(), 401);
    }
    let mut body = recovery_request(&original);
    body["expected_session_id"] = json!("malformed");
    assert_eq!(recover(&client, &base, &body).await.status(), 422);
    assert_exists(
        &registration_db(&tmp).await,
        original["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn another_accounts_exact_session_and_global_access_proof_are_rejected() {
    use serverbee_server::service::mobile_auth::{MobileAuthService, MobileLoginParams};
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let db = registration_db(&tmp).await;
    let member = AuthService::create_user(&db, "member", "memberpass", "member")
        .await
        .unwrap();
    let foreign = MobileAuthService::login(
        &db,
        &serverbee_server::config::MobileConfig::default(),
        MobileLoginParams {
            username: "member",
            password: "memberpass",
            totp_code: None,
            installation_id: INST_ID,
            device_name: "foreign",
            ip: "",
            user_agent: "",
        },
    )
    .await
    .unwrap();
    let mut body = recovery_request(&original);
    body["expected_session_id"] = json!(foreign.mobile_session_id);
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    body["expected_session_id"] = Value::Null;
    body["access_token"] = json!(foreign.access_token);
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    body["access_token"] = json!("unknown-access");
    body["revocation_token"] = json!(foreign.revocation_token);
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    body["revocation_token"] = Value::Null;
    body["refresh_token"] = json!(foreign.refresh_token);
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    assert_exists(&db, &foreign.mobile_session_id).await;
    assert_eq!(
        mobile_session::Entity::find_by_id(&foreign.mobile_session_id)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .user_id,
        member.id
    );
    assert_exists(&db, original["data"]["mobile_session_id"].as_str().unwrap()).await;
}

#[tokio::test]
async fn totp_reauthentication_and_onboarding_policy_match_mobile_login() {
    use sea_orm::{ActiveModelTrait, Set};
    use totp_rs::{Algorithm, Secret, TOTP};
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let db = registration_db(&tmp).await;
    let model = user::Entity::find_by_id(original["data"]["user"]["id"].as_str().unwrap())
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let (secret, _, _) = AuthService::generate_totp_secret("admin").unwrap();
    let code = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        Secret::Encoded(secret.clone()).to_bytes().unwrap(),
        None,
        "admin".to_string(),
    )
    .unwrap()
    .generate_current()
    .unwrap();
    let mut active: user::ActiveModel = model.into();
    active.totp_secret = Set(Some(secret));
    let model = active.update(&db).await.unwrap();
    let mut body = recovery_request(&original);
    assert_eq!(recover(&client, &base, &body).await.status(), 422);
    body["totp_code"] = json!("invalid");
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    let mut active: user::ActiveModel = model.into();
    active.must_change_password = Set(true);
    let model = active.update(&db).await.unwrap();
    body["totp_code"] = json!(code);
    assert_eq!(recover(&client, &base, &body).await.status(), 403);
    let mut active: user::ActiveModel = model.into();
    active.must_change_password = Set(false);
    active.update(&db).await.unwrap();
    assert_eq!(recover(&client, &base, &body).await.status(), 200);
}

#[tokio::test]
async fn unknown_scoped_session_is_never_selected_from_a_single_candidate() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let mut body = recovery_request(&original);
    body["expected_session_id"] = Value::Null;
    body["access_token"] = json!("unknown-access");
    body["refresh_token"] = json!("unknown-refresh");
    body["revocation_token"] = Value::Null;
    let response = recover(&client, &base, &body).await;
    assert_eq!(response.status(), 200);
    let data = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(data["outcome"], "selection_required");
    assert_eq!(data["mobile_session_id"], Value::Null);
    assert_eq!(
        data["candidates"][0]["mobile_session_id"],
        original["data"]["mobile_session_id"]
    );
    assert_exists(
        &registration_db(&tmp).await,
        original["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn missing_exact_identity_with_dangling_operational_state_fails_closed() {
    use sqlx::Connection;
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let absent = uuid::Uuid::new_v4().to_string();
    let mut fixture = sqlx::SqliteConnection::connect(&format!(
        "sqlite://{}?mode=rw",
        tmp.path().join("test.db").display()
    ))
    .await
    .unwrap();
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut fixture)
        .await
        .unwrap();
    let mut body = recovery_request(&original);
    body["expected_session_id"] = json!(absent);
    body["access_token"] = json!("unknown-access");
    body["refresh_token"] = json!("unknown-refresh");
    body["revocation_token"] = Value::Null;
    for (table, insert) in [
        (
            "sessions",
            "INSERT INTO sessions (id,user_id,token,ip,user_agent,expires_at,created_at,source,mobile_session_id) SELECT 'orphan',user_id,'orphan-hash','','','2999-01-01 00:00:00',CURRENT_TIMESTAMP,'mobile',? FROM mobile_sessions WHERE id=?",
        ),
        (
            "device_tokens",
            "INSERT INTO device_tokens (id,user_id,mobile_session_id,installation_id,token,created_at,updated_at) SELECT 'orphan',user_id,?,'orphan-install','orphan-token',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP FROM mobile_sessions WHERE id=?",
        ),
        (
            "mobile_push_registrations",
            "INSERT INTO mobile_push_registrations (installation_id,user_id,mobile_session_id,updated_at) SELECT 'orphan-install',user_id,?,CURRENT_TIMESTAMP FROM mobile_sessions WHERE id=?",
        ),
        (
            "mobile_session_revocation_proofs",
            "INSERT INTO mobile_session_revocation_proofs (id,mobile_session_id,token_hash) SELECT 'orphan',?,'orphan-hash' FROM mobile_sessions WHERE id=?",
        ),
    ] {
        sqlx::query(insert)
            .bind(&absent)
            .bind(original["data"]["mobile_session_id"].as_str().unwrap())
            .execute(&mut fixture)
            .await
            .unwrap();
        assert_eq!(recover(&client, &base, &body).await.status(), 409);
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE mobile_session_id=?"
        ))
        .bind(&absent)
        .fetch_one(&mut fixture)
        .await
        .unwrap();
        assert_eq!(count, 1);
        sqlx::query(&format!("DELETE FROM {table} WHERE mobile_session_id=?"))
            .bind(&absent)
            .execute(&mut fixture)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn recovery_shares_login_rate_limit_and_failed_login_audit() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let mut body = recovery_request(&original);
    body["password"] = json!("never-log-this-password");
    for _ in 0..4 {
        assert_eq!(recover(&client, &base, &body).await.status(), 401);
    }
    assert_eq!(
        mobile_login(&client, &base, "admin", "testpass", INST_ID)
            .await
            .status(),
        429
    );
    assert_eq!(recover(&client, &base, &body).await.status(), 429);
    let db = registration_db(&tmp).await;
    let entries = db.query_all(sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite, "SELECT action,detail FROM audit_logs WHERE action IN ('login_failed','login_rate_limited')")).await.unwrap();
    assert_eq!(entries.len(), 6);
    for entry in entries {
        let detail: Option<String> = entry.try_get("", "detail").unwrap();
        let detail = detail.unwrap_or_default();
        assert!(!detail.contains("never-log-this-password"));
        assert!(!detail.contains(original["data"]["access_token"].as_str().unwrap()));
        assert!(!detail.contains(original["data"]["refresh_token"].as_str().unwrap()));
    }
}

#[tokio::test]
async fn nonexistent_expected_id_cannot_acknowledge_other_live_captured_material() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let mut body = recovery_request(&original);
    body["expected_session_id"] = json!(uuid::Uuid::new_v4().to_string());
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    // Isolate the current refresh Argon2 identity from access/proof mappings.
    body["access_token"] = json!("discarded-access");
    body["revocation_token"] = Value::Null;
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    assert_exists(
        &registration_db(&tmp).await,
        original["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn global_access_and_proof_with_wrong_installation_do_not_ack_empty_scope() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let mut body = recovery_request(&original);
    body["expected_session_id"] = Value::Null;
    body["installation_id"] = json!("empty-wrong-installation");
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    body["access_token"] = json!("discarded-access");
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    assert_exists(
        &registration_db(&tmp).await,
        original["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn idless_absence_refuses_orphan_device_or_encrypted_registration() {
    use sqlx::Connection;
    for table in ["device_tokens", "mobile_push_registrations"] {
        let (base, tmp) = start_test_server().await;
        let client = http_client();
        let original = mobile_admin_token(&client, &base, INST_ID).await;
        let mut fixture = sqlx::SqliteConnection::connect(&format!(
            "sqlite://{}?mode=rw",
            tmp.path().join("test.db").display()
        ))
        .await
        .unwrap();
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&mut fixture)
            .await
            .unwrap();
        let insert = if table == "device_tokens" {
            "INSERT INTO device_tokens (id,user_id,mobile_session_id,installation_id,token,created_at,updated_at) SELECT 'orphan',user_id,id,installation_id,'token',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP FROM mobile_sessions"
        } else {
            "INSERT INTO mobile_push_registrations (installation_id,user_id,mobile_session_id,updated_at) SELECT installation_id,user_id,id,CURRENT_TIMESTAMP FROM mobile_sessions"
        };
        sqlx::query(insert).execute(&mut fixture).await.unwrap();
        for statement in ["DELETE FROM sessions", "DELETE FROM mobile_sessions"] {
            sqlx::query(statement).execute(&mut fixture).await.unwrap();
        }
        let mut body = recovery_request(&original);
        body["expected_session_id"] = Value::Null;
        assert_eq!(recover(&client, &base, &body).await.status(), 409);
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut fixture)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
}

#[tokio::test]
async fn idless_absence_refuses_unattributed_mobile_access_but_allows_other_installations() {
    use sqlx::Connection;
    for dangling_id in [false, true] {
        let (base, tmp) = start_test_server().await;
        let client = http_client();
        let original = mobile_admin_token(&client, &base, INST_ID).await;
        let other = mobile_admin_token(&client, &base, "other-installation").await;
        let mut fixture = sqlx::SqliteConnection::connect(&format!(
            "sqlite://{}?mode=rw",
            tmp.path().join("test.db").display()
        ))
        .await
        .unwrap();
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&mut fixture)
            .await
            .unwrap();
        let original_id = original["data"]["mobile_session_id"].as_str().unwrap();
        sqlx::query("DELETE FROM mobile_sessions WHERE id=?")
            .bind(original_id)
            .execute(&mut fixture)
            .await
            .unwrap();
        // No captured credential maps to the orphan. Installation ownership can
        // no longer be reconstructed even though the account still has access.
        sqlx::query(
            "UPDATE sessions SET token='unmatched-orphan-access' WHERE mobile_session_id=?",
        )
        .bind(original_id)
        .execute(&mut fixture)
        .await
        .unwrap();
        if !dangling_id {
            sqlx::query("UPDATE sessions SET mobile_session_id=NULL WHERE mobile_session_id=?")
                .bind(original_id)
                .execute(&mut fixture)
                .await
                .unwrap();
        }
        let mut body = recovery_request(&original);
        body["expected_session_id"] = Value::Null;
        assert_eq!(recover(&client, &base, &body).await.status(), 409);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE token='unmatched-orphan-access'",
        )
        .fetch_one(&mut fixture)
        .await
        .unwrap();
        assert_eq!(count, 1, "recovery must preserve unattributed access");
        sqlx::query("DELETE FROM sessions WHERE token='unmatched-orphan-access'")
            .execute(&mut fixture)
            .await
            .unwrap();
        let response = recover(&client, &base, &body).await;
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.json::<Value>().await.unwrap()["data"]["outcome"],
            "already_absent"
        );
        assert_exists(
            &registration_db(&tmp).await,
            other["data"]["mobile_session_id"].as_str().unwrap(),
        )
        .await;
    }
}

#[tokio::test]
async fn selection_lists_only_scoped_expired_and_live_rows_and_selected_retry_preserves_replacement()
 {
    use serverbee_server::service::mobile_auth::{MobileAuthService, MobileLoginParams};
    let (base, tmp, original) = legacy_mobile_server().await;
    let client = http_client();
    let db = registration_db(&tmp).await;
    let original_id = mobile_session::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .id;
    expire_original(&db).await;
    let replacement = mobile_admin_token(&client, &base, INST_ID).await;
    let other = mobile_admin_token(&client, &base, "other-installation").await;
    AuthService::create_user(&db, "member", "memberpass", "member")
        .await
        .unwrap();
    let foreign = MobileAuthService::login(
        &db,
        &serverbee_server::config::MobileConfig::default(),
        MobileLoginParams {
            username: "member",
            password: "memberpass",
            totp_code: None,
            installation_id: INST_ID,
            device_name: "foreign-account",
            ip: "",
            user_agent: "",
        },
    )
    .await
    .unwrap();
    let mut body = recovery_request(&original);
    body["access_token"] = json!("discarded-access");
    body["refresh_token"] = json!("discarded-refresh");
    let result = recover(&client, &base, &body).await;
    assert_eq!(result.status(), 200);
    let data = result.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(data["outcome"], "selection_required");
    assert_eq!(data["mobile_session_id"], Value::Null);
    let ids: std::collections::BTreeSet<_> = data["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|candidate| candidate["mobile_session_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        std::collections::BTreeSet::from([
            original_id.as_str(),
            replacement["data"]["mobile_session_id"].as_str().unwrap()
        ])
    );
    for id in [
        original_id.as_str(),
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
        other["data"]["mobile_session_id"].as_str().unwrap(),
        foreign.mobile_session_id.as_str(),
    ] {
        assert_exists(&db, id).await;
    }
    // Explicit selection is persisted by the caller before dispatch. Losing
    // the first deletion response cannot cause a later retry to pick a new row.
    body["expected_session_id"] = json!(original_id);
    let committed = recover(&client, &base, &body).await;
    assert_eq!(committed.status(), 200);
    drop(committed);
    let retry = recover(&client, &base, &body).await;
    assert_eq!(retry.status(), 200);
    let data = retry.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(data["outcome"], "already_absent");
    assert_eq!(data["mobile_session_id"], original_id);
    assert_eq!(data["candidates"], json!([]));
    assert_exists(
        &db,
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
    assert_exists(&db, other["data"]["mobile_session_id"].as_str().unwrap()).await;
    assert_exists(&db, &foreign.mobile_session_id).await;
}

#[tokio::test]
async fn selecting_a_session_outside_original_installation_is_rejected() {
    let (base, tmp, original) = legacy_mobile_server().await;
    let client = http_client();
    let other = mobile_admin_token(&client, &base, "other-installation").await;
    let mut body = recovery_request(&original);
    body["access_token"] = json!("discarded-access");
    body["refresh_token"] = json!("discarded-refresh");
    let offered = recover(&client, &base, &body).await;
    assert_eq!(offered.status(), 200);
    assert_eq!(
        offered.json::<Value>().await.unwrap()["data"]["outcome"],
        "selection_required"
    );
    body["expected_session_id"] = other["data"]["mobile_session_id"].clone();
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    assert_exists(
        &registration_db(&tmp).await,
        other["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn recovery_candidate_capacity_never_truncates_or_selects_a_subset() {
    let (base, tmp, original) = legacy_mobile_server().await;
    let client = http_client();
    let db = registration_db(&tmp).await;
    let id = mobile_session::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .id;
    for _ in 0..64 {
        db.execute(sea_orm::Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite,
            "INSERT INTO mobile_sessions (id,user_id,refresh_token_hash,installation_id,device_name,created_at,expires_at,last_used_at) SELECT ?,user_id,refresh_token_hash,installation_id,device_name,created_at,expires_at,last_used_at FROM mobile_sessions WHERE id=?",
            [uuid::Uuid::new_v4().to_string().into(), id.clone().into()],
        )).await.unwrap();
    }
    let mut body = recovery_request(&original);
    body["access_token"] = json!("discarded-access");
    body["refresh_token"] = json!("discarded-refresh");
    assert_eq!(recover(&client, &base, &body).await.status(), 409);
    let count = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT COUNT(*) AS n FROM mobile_sessions",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(count, 65);
    assert_exists(&db, &id).await;
}
