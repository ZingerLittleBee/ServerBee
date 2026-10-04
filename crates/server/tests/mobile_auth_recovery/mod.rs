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

async fn pairing_code(client: &reqwest::Client, base: &str) -> String {
    super::login_admin(client, base).await;
    let response = client
        .post(format!("{base}/api/mobile/pair"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.json::<Value>().await.unwrap()["data"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn qr_recovery_request(tokens: &Value, code: &str) -> Value {
    let mut body = recovery_request(tokens);
    body.as_object_mut().unwrap().remove("username");
    body.as_object_mut().unwrap().remove("password");
    body["pairing_code"] = json!(code);
    body
}

#[tokio::test]
async fn qr_recovery_consumes_pairing_code_without_minting_login_tokens() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let code = pairing_code(&client, &base).await;
    let response = recover(&client, &base, &qr_recovery_request(&original, &code)).await;
    assert_eq!(
        response.status(),
        200,
        "QR-only accounts must be able to recover without a password"
    );
    let result = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(result["outcome"], "ok");
    assert_eq!(
        result["mobile_session_id"],
        original["data"]["mobile_session_id"]
    );
    assert!(
        result["recovery_token"]
            .as_str()
            .is_some_and(|token| token.starts_with("sb_recover_"))
    );
    assert!(result.get("access_token").is_none());
    assert!(result.get("refresh_token").is_none());
    let db = registration_db(&tmp).await;
    assert!(
        mobile_session::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .is_empty()
    );
    let replay = client
        .post(format!("{base}/api/mobile/auth/pair"))
        .json(&json!({"code":code,"installation_id":INST_ID,"device_name":"Must not log in"}))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), 400);
}

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

fn grant_request(mut body: Value, response: &Value) -> Value {
    body.as_object_mut().unwrap().remove("pairing_code");
    body["recovery_token"] = response["recovery_token"].clone();
    body
}

fn discard_captured_mapping(body: &mut Value) {
    body["expected_session_id"] = Value::Null;
    body["access_token"] = json!("lost-access");
    body["refresh_token"] = json!("lost-refresh");
    body["revocation_token"] = json!("lost-revocation");
}

#[tokio::test]
async fn qr_candidates_are_frozen_and_selected_lost_response_retry_preserves_new_logins() {
    let (base, tmp, _) = super::common::start_test_server_with_login_limit(30).await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    expire_original(&registration_db(&tmp).await).await;
    let code = pairing_code(&client, &base).await;
    let mut body = qr_recovery_request(&original, &code);
    discard_captured_mapping(&mut body);
    let offered = recover(&client, &base, &body).await;
    assert_eq!(offered.status(), 200);
    let offered = offered.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(offered["outcome"], "selection_required");
    assert_eq!(offered["candidates"].as_array().unwrap().len(), 1);
    let replacement = mobile_admin_token(&client, &base, INST_ID).await;
    let other = mobile_admin_token(&client, &base, "other-installation").await;
    let db = registration_db(&tmp).await;
    body = grant_request(body, &offered);
    let repeated = recover(&client, &base, &body).await;
    assert_eq!(repeated.status(), 200);
    assert_eq!(
        repeated.json::<Value>().await.unwrap()["data"]["candidates"],
        offered["candidates"]
    );
    let mut wrong = body.clone();
    wrong["expected_session_id"] = replacement["data"]["mobile_session_id"].clone();
    assert_eq!(recover(&client, &base, &wrong).await.status(), 401);
    assert_exists(&db, original["data"]["mobile_session_id"].as_str().unwrap()).await;
    body["expected_session_id"] = original["data"]["mobile_session_id"].clone();
    let committed = recover(&client, &base, &body).await;
    assert_eq!(committed.status(), 200);
    drop(committed); // Simulate losing the acknowledgement after committed deletion.
    let retry = recover(&client, &base, &body).await;
    assert_eq!(retry.status(), 200);
    let result = retry.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(result["outcome"], "already_absent");
    assert_eq!(
        result["mobile_session_id"],
        original["data"]["mobile_session_id"]
    );
    assert_eq!(result["candidates"], json!([]));
    assert_eq!(recover(&client, &base, &wrong).await.status(), 401);
    assert_exists(
        &db,
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
    assert_exists(&db, other["data"]["mobile_session_id"].as_str().unwrap()).await;
}

#[tokio::test]
async fn qr_terminal_grant_pins_implicitly_resolved_original_for_idless_concurrent_retries() {
    let (base, tmp, _) = super::common::start_test_server_with_login_limit(30).await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let code = pairing_code(&client, &base).await;
    let mut body = qr_recovery_request(&original, &code);
    body["expected_session_id"] = Value::Null;
    let first = recover(&client, &base, &body).await;
    assert_eq!(first.status(), 200);
    let first = first.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(first["outcome"], "ok");
    body = grant_request(body, &first);
    let replacement = mobile_admin_token(&client, &base, INST_ID).await;
    let (a, b) = tokio::join!(
        recover(&client, &base, &body),
        recover(&client, &base, &body)
    );
    for response in [a, b] {
        assert_eq!(response.status(), 200);
        let data = response.json::<Value>().await.unwrap()["data"].clone();
        assert_eq!(data["outcome"], "already_absent");
        assert_eq!(
            data["mobile_session_id"],
            original["data"]["mobile_session_id"]
        );
    }
    assert_exists(
        &registration_db(&tmp).await,
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn qr_grant_rejects_changed_account_installation_and_each_captured_secret() {
    for field in [
        "expected_user_id",
        "installation_id",
        "access_token",
        "refresh_token",
        "revocation_token",
    ] {
        let (base, tmp) = start_test_server().await;
        let client = http_client();
        let original = mobile_admin_token(&client, &base, INST_ID).await;
        let code = pairing_code(&client, &base).await;
        let mut body = qr_recovery_request(&original, &code);
        discard_captured_mapping(&mut body);
        let offered = recover(&client, &base, &body).await;
        assert_eq!(offered.status(), 200);
        body = grant_request(body, &offered.json::<Value>().await.unwrap()["data"]);
        body[field] = json!("changed-binding");
        assert_eq!(
            recover(&client, &base, &body).await.status(),
            401,
            "changed {field}"
        );
        assert_exists(
            &registration_db(&tmp).await,
            original["data"]["mobile_session_id"].as_str().unwrap(),
        )
        .await;
    }
}

#[tokio::test]
async fn qr_code_wrong_user_is_consumed_and_cannot_be_replayed_for_normal_login() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let code = pairing_code(&client, &base).await;
    let mut body = qr_recovery_request(&original, &code);
    body["expected_user_id"] = json!(uuid::Uuid::new_v4().to_string());
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    body["expected_user_id"] = original["data"]["user"]["id"].clone();
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    let ordinary = client
        .post(format!("{base}/api/mobile/auth/pair"))
        .json(&json!({"code":code,"installation_id":INST_ID,"device_name":"replay"}))
        .send()
        .await
        .unwrap();
    assert_eq!(ordinary.status(), 400);
    assert_exists(
        &registration_db(&tmp).await,
        original["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn qr_and_grant_modes_reject_mixed_password_and_totp_without_consuming_code() {
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let code = pairing_code(&client, &base).await;
    let body = qr_recovery_request(&original, &code);
    for (field, value) in [
        ("username", json!("admin")),
        ("password", json!("testpass")),
        ("totp_code", json!("123456")),
        ("recovery_token", json!("sb_recover_invalid")),
    ] {
        let mut mixed = body.clone();
        mixed[field] = value;
        assert_eq!(recover(&client, &base, &mixed).await.status(), 422);
    }
    let mut missing = body.clone();
    missing.as_object_mut().unwrap().remove("pairing_code");
    assert_eq!(recover(&client, &base, &missing).await.status(), 422);
    assert_exists(
        &registration_db(&tmp).await,
        original["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
    let good = recover(&client, &base, &body).await;
    assert_eq!(good.status(), 200);
    let mut grant = grant_request(body, &good.json::<Value>().await.unwrap()["data"]);
    grant["totp_code"] = json!("123456");
    assert_eq!(recover(&client, &base, &grant).await.status(), 422);
}

#[tokio::test]
async fn qr_code_and_cleanup_grant_expire_without_deleting_original() {
    for expire_grant in [false, true] {
        let (base, _tmp, state) = super::common::start_test_server_with_state().await;
        let client = http_client();
        let original = mobile_admin_token(&client, &base, INST_ID).await;
        let code = pairing_code(&client, &base).await;
        let mut body = qr_recovery_request(&original, &code);
        if expire_grant {
            discard_captured_mapping(&mut body);
            let offered = recover(&client, &base, &body).await;
            assert_eq!(offered.status(), 200);
            body = grant_request(body, &offered.json::<Value>().await.unwrap()["data"]);
            let mut entries = state.mobile_recovery_grants.entries.lock().await;
            for grant in entries.values_mut() {
                std::sync::Arc::get_mut(grant).unwrap().created_at -= chrono::Duration::minutes(6);
            }
        } else {
            state.pending_pairs.get_mut(&code).unwrap().created_at -= chrono::Duration::minutes(6);
        }
        assert_eq!(recover(&client, &base, &body).await.status(), 401);
        assert_exists(
            &state.db,
            original["data"]["mobile_session_id"].as_str().unwrap(),
        )
        .await;
        assert!(state.mobile_recovery_grants.entries.lock().await.is_empty());
    }
}

#[tokio::test]
async fn qr_snapshot_and_grant_reject_password_totp_and_onboarding_policy_changes() {
    for after_grant in [false, true] {
        for mutation in ["password_hash", "totp_secret", "must_change_password"] {
            let (base, tmp) = start_test_server().await;
            let client = http_client();
            let original = mobile_admin_token(&client, &base, INST_ID).await;
            let code = pairing_code(&client, &base).await;
            let mut body = qr_recovery_request(&original, &code);
            if after_grant {
                discard_captured_mapping(&mut body);
                let offered = recover(&client, &base, &body).await;
                assert_eq!(offered.status(), 200);
                body = grant_request(body, &offered.json::<Value>().await.unwrap()["data"]);
                body["expected_session_id"] = original["data"]["mobile_session_id"].clone();
            }
            let db = registration_db(&tmp).await;
            let statement = match mutation {
                "password_hash" => "UPDATE users SET password_hash='changed-security-snapshot'",
                "totp_secret" => "UPDATE users SET totp_secret='changed-security-snapshot'",
                _ => "UPDATE users SET must_change_password=1",
            };
            db.execute_unprepared(statement).await.unwrap();
            assert_eq!(
                recover(&client, &base, &body).await.status(),
                if mutation == "must_change_password" {
                    403
                } else {
                    401
                }
            );
            assert_exists(&db, original["data"]["mobile_session_id"].as_str().unwrap()).await;
        }
    }
}

#[tokio::test]
async fn qr_grant_capacity_is_bounded_and_expired_entries_are_cleaned() {
    use serverbee_server::service::mobile_recovery_grant::MAX_RECOVERY_GRANTS;
    let (base, _tmp, state) = super::common::start_test_server_with_login_limit(30).await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let code = pairing_code(&client, &base).await;
    let mut body = qr_recovery_request(&original, &code);
    discard_captured_mapping(&mut body);
    assert_eq!(recover(&client, &base, &body).await.status(), 200);
    {
        let mut entries = state.mobile_recovery_grants.entries.lock().await;
        let fixture = entries.values().next().unwrap().clone();
        for index in 1..MAX_RECOVERY_GRANTS {
            entries.insert(format!("fixture-{index}"), fixture.clone());
        }
    }
    let code = pairing_code(&client, &base).await;
    body["pairing_code"] = json!(code);
    assert_eq!(recover(&client, &base, &body).await.status(), 429);
    assert_eq!(
        state.mobile_recovery_grants.entries.lock().await.len(),
        MAX_RECOVERY_GRANTS
    );
    assert_exists(
        &state.db,
        original["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
    {
        let mut entries = state.mobile_recovery_grants.entries.lock().await;
        entries.retain(|key, _| !key.starts_with("fixture-"));
        std::sync::Arc::get_mut(entries.values_mut().next().unwrap())
            .unwrap()
            .created_at -= chrono::Duration::minutes(6);
    }
    let code = pairing_code(&client, &base).await;
    body["pairing_code"] = json!(code);
    assert_eq!(recover(&client, &base, &body).await.status(), 200);
    assert_eq!(state.mobile_recovery_grants.entries.lock().await.len(), 1);
}

#[tokio::test]
async fn qr_absence_only_terminal_grant_cannot_offer_or_delete_replacement() {
    let (base, tmp, _) = super::common::start_test_server_with_login_limit(30).await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let cleanup = recover(&client, &base, &recovery_request(&original)).await;
    assert_eq!(cleanup.status(), 200);
    assert!(
        cleanup.json::<Value>().await.unwrap()["data"]
            .get("recovery_token")
            .is_none()
    );
    let code = pairing_code(&client, &base).await;
    let mut body = qr_recovery_request(&original, &code);
    discard_captured_mapping(&mut body);
    let response = recover(&client, &base, &body).await;
    assert_eq!(response.status(), 200);
    let response = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(response["outcome"], "already_absent");
    assert_eq!(response["mobile_session_id"], Value::Null);
    body = grant_request(body, &response);
    let replacement = mobile_admin_token(&client, &base, INST_ID).await;
    assert_eq!(recover(&client, &base, &body).await.status(), 409);
    body["expected_session_id"] = replacement["data"]["mobile_session_id"].clone();
    assert_eq!(recover(&client, &base, &body).await.status(), 401);
    assert_exists(
        &registration_db(&tmp).await,
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn qr_code_is_single_use_under_concurrency_and_grant_cannot_authenticate_normal_routes() {
    let (base, tmp, _) = super::common::start_test_server_with_login_limit(30).await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let code = pairing_code(&client, &base).await;
    let body = qr_recovery_request(&original, &code);
    let (a, b) = tokio::join!(
        recover(&client, &base, &body),
        recover(&client, &base, &body)
    );
    let (success, failed) = if a.status() == 200 { (a, b) } else { (b, a) };
    assert_eq!(success.status(), 200);
    assert_eq!(failed.status(), 401);
    let token = success.json::<Value>().await.unwrap()["data"]["recovery_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let replacement = mobile_admin_token(&client, &base, INST_ID).await;
    let anonymous = http_client();
    let pair = anonymous
        .post(format!("{base}/api/mobile/auth/pair"))
        .json(&json!({"code":token,"installation_id":INST_ID,"device_name":"No grant login"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pair.status(), 400);
    let refresh = anonymous
        .post(format!("{base}/api/mobile/auth/refresh"))
        .json(&json!({"refresh_token":token,"installation_id":INST_ID}))
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), 401);
    let logout = anonymous
        .post(format!("{base}/api/mobile/auth/logout"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), 401);
    let login = mobile_login(&anonymous, &base, "admin", &token, INST_ID).await;
    assert_eq!(login.status(), 401);
    let db = registration_db(&tmp).await;
    assert_exists(
        &db,
        replacement["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
    assert_eq!(
        mobile_session::Entity::find().all(&db).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn qr_recovery_shares_password_login_rate_limit_and_audit_without_logging_code() {
    let (base, _tmp, state) = super::common::start_test_server_with_state().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let code = pairing_code(&client, &base).await;
    let mut body = qr_recovery_request(&original, &code);
    body["pairing_code"] = json!("sb_pair_invalid-not-logged");
    for _ in 0..3 {
        assert_eq!(recover(&client, &base, &body).await.status(), 401);
    }
    body["pairing_code"] = json!(code);
    assert_eq!(recover(&client, &base, &body).await.status(), 429);
    assert!(
        state.pending_pairs.contains_key(&code),
        "rate denial does not consume the valid QR"
    );
    assert_eq!(
        mobile_login(&client, &base, "admin", "testpass", INST_ID)
            .await
            .status(),
        429
    );
    let row=state.db.query_one(sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite,
        "SELECT COUNT(*) AS count FROM audit_logs WHERE action='login_failed' AND detail LIKE '%qr-recovery%'".to_owned())).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "count").unwrap(), 3);
    let row=state.db.query_one(sea_orm::Statement::from_string(sea_orm::DatabaseBackend::Sqlite,
        "SELECT COUNT(*) AS count FROM audit_logs WHERE detail LIKE '%sb_pair_%' OR detail LIKE '%sb_recover_%'".to_owned())).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "count").unwrap(), 0);
    assert_exists(
        &state.db,
        original["data"]["mobile_session_id"].as_str().unwrap(),
    )
    .await;
}

#[tokio::test]
async fn qr_recovery_preserves_orphan_operational_state_and_refuses_unknown_absence() {
    use sqlx::Connection;
    let (base, tmp) = start_test_server().await;
    let client = http_client();
    let original = mobile_admin_token(&client, &base, INST_ID).await;
    let code = pairing_code(&client, &base).await;
    let mut body = qr_recovery_request(&original, &code);
    discard_captured_mapping(&mut body);
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
    sqlx::query("DELETE FROM mobile_sessions")
        .execute(&mut fixture)
        .await
        .unwrap();
    assert_eq!(recover(&client, &base, &body).await.status(), 409);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE source='mobile'")
        .fetch_one(&mut fixture)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "unattributed access authority must remain untouched"
    );
}
