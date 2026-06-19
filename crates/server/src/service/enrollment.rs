use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::entity::agent_enrollment;
use crate::error::AppError;
use crate::service::auth::AuthService;

pub const DEFAULT_TTL_SECS: i64 = 600;

pub struct EnrollmentService;

impl EnrollmentService {
    /// Mint an enrollment bound to a specific server. May run inside a tx so
    /// that callers (T7 server-create, T9 recover, T10 regenerate) can make
    /// the mint atomic with surrounding state changes.
    ///
    /// Returns the stored row and the plaintext code (shown to the operator
    /// exactly once). The DB enforces at most one outstanding (not consumed,
    /// not revoked) enrollment per server via partial unique index
    /// `idx_enrollments_active_per_server`; a second concurrent mint without
    /// first revoking the outstanding one will surface as a DB error.
    pub async fn mint_for_server<C: ConnectionTrait>(
        conn: &C,
        target_server_id: &str,
        created_by: &str,
        ttl_secs: i64,
    ) -> Result<(agent_enrollment::Model, String), AppError> {
        let now = Utc::now();
        let plaintext = AuthService::generate_session_token();
        let hash = AuthService::hash_password(&plaintext)?;
        let prefix = plaintext[..8.min(plaintext.len())].to_string();
        let id = Uuid::new_v4().to_string();

        let model = agent_enrollment::ActiveModel {
            id: Set(id),
            code_hash: Set(hash),
            code_prefix: Set(prefix),
            target_server_id: Set(target_server_id.to_string()),
            created_by: Set(created_by.to_string()),
            expires_at: Set(now + Duration::seconds(ttl_secs)),
            consumed_at: Set(None),
            revoked_at: Set(None),
            created_at: Set(now),
        }
        .insert(conn)
        .await?;

        Ok((model, plaintext))
    }

    /// Verify a bearer code and consume it atomically.
    ///
    /// Accepts rows where `consumed_at IS NULL AND revoked_at IS NULL AND
    /// expires_at > now()`. On match, sets `consumed_at = now()` in the same
    /// connection (which the caller is expected to run inside a tx so the
    /// consume is committed atomically with the surrounding registration
    /// flow). On no match — wrong code, expired, revoked, or already consumed
    /// — returns `Ok(None)`.
    pub async fn verify_and_consume_tx<C: ConnectionTrait>(
        tx: &C,
        code: &str,
    ) -> Result<Option<agent_enrollment::Model>, AppError> {
        if code.len() < 8 {
            return Ok(None);
        }
        let prefix = &code[..8];
        let candidates = agent_enrollment::Entity::find()
            .filter(agent_enrollment::Column::CodePrefix.eq(prefix))
            .filter(agent_enrollment::Column::ConsumedAt.is_null())
            .filter(agent_enrollment::Column::RevokedAt.is_null())
            .all(tx)
            .await?;

        let now = Utc::now();
        for cand in candidates {
            if cand.expires_at <= now {
                continue;
            }
            if AuthService::verify_password(code, &cand.code_hash)? {
                let mut active: agent_enrollment::ActiveModel = cand.clone().into();
                active.consumed_at = Set(Some(now));
                let updated = active.update(tx).await?;
                return Ok(Some(updated));
            }
        }
        Ok(None)
    }

    /// List all enrollments (admin UI). Oldest first; callers sort if needed.
    pub async fn list(
        db: &DatabaseConnection,
    ) -> Result<Vec<agent_enrollment::Model>, AppError> {
        Ok(agent_enrollment::Entity::find()
            .order_by_asc(agent_enrollment::Column::CreatedAt)
            .all(db)
            .await?)
    }

    /// Mark an enrollment revoked. Idempotent: a missing row or an already-
    /// revoked row both succeed without error.
    pub async fn revoke(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let row = agent_enrollment::Entity::find_by_id(id).one(db).await?;
        let Some(row) = row else {
            return Ok(());
        };
        if row.revoked_at.is_some() {
            return Ok(());
        }
        let mut active: agent_enrollment::ActiveModel = row.into();
        active.revoked_at = Set(Some(Utc::now()));
        active.update(db).await?;
        Ok(())
    }

    /// Revoke any outstanding enrollment for a server. Used by recover /
    /// regenerate flows so a fresh mint won't trip the partial unique index.
    /// Returns the id of the revoked row, if any.
    pub async fn revoke_outstanding_tx<C: ConnectionTrait>(
        tx: &C,
        server_id: &str,
    ) -> Result<Option<String>, AppError> {
        let outstanding = agent_enrollment::Entity::find()
            .filter(agent_enrollment::Column::TargetServerId.eq(server_id))
            .filter(agent_enrollment::Column::ConsumedAt.is_null())
            .filter(agent_enrollment::Column::RevokedAt.is_null())
            .one(tx)
            .await?;
        let Some(row) = outstanding else {
            return Ok(None);
        };
        let id = row.id.clone();
        let mut active: agent_enrollment::ActiveModel = row.into();
        active.revoked_at = Set(Some(Utc::now()));
        active.update(tx).await?;
        Ok(Some(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{server, user};
    use crate::test_utils::setup_test_db;
    use sea_orm::TransactionTrait;
    use serverbee_common::constants::CAP_DEFAULT;

    async fn seed_user(db: &DatabaseConnection) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        user::ActiveModel {
            id: Set(id.clone()),
            username: Set(format!("user-{id}")),
            password_hash: Set("$argon2id$v=19$m=19456,t=2,p=1$x$x".to_string()),
            role: Set("admin".to_string()),
            totp_secret: Set(None),
            must_change_password: Set(false),
            password_changed_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed user");
        id
    }

    async fn seed_pending_server(db: &DatabaseConnection) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.clone()),
            token_hash: Set(None),
            token_prefix: Set(None),
            name: Set("t".to_string()),
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
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            features: Set("[]".to_string()),
            last_remote_addr: Set(None),
            fingerprint: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed pending server");
        id
    }

    #[tokio::test]
    async fn mint_for_server_returns_plaintext_once() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (model, code) = EnrollmentService::mint_for_server(&db, &s, &u, 600)
            .await
            .expect("mint");
        assert_eq!(model.target_server_id, s);
        assert_eq!(model.code_prefix, code[..8]);
        assert!(model.consumed_at.is_none());
        assert!(model.revoked_at.is_none());
    }

    #[tokio::test]
    async fn verify_and_consume_accepts_usable_code() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (_m, code) = EnrollmentService::mint_for_server(&db, &s, &u, 600)
            .await
            .expect("mint");

        let row = db
            .transaction::<_, _, AppError>(|tx| {
                Box::pin(async move { EnrollmentService::verify_and_consume_tx(tx, &code).await })
            })
            .await
            .expect("tx ok");
        assert!(row.is_some());
        assert!(row.unwrap().consumed_at.is_some());
    }

    #[tokio::test]
    async fn verify_and_consume_rejects_revoked_code() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (m, code) = EnrollmentService::mint_for_server(&db, &s, &u, 600)
            .await
            .expect("mint");
        EnrollmentService::revoke(&db, &m.id).await.expect("revoke");

        let row = db
            .transaction::<_, _, AppError>(|tx| {
                Box::pin(async move { EnrollmentService::verify_and_consume_tx(tx, &code).await })
            })
            .await
            .expect("tx ok");
        assert!(row.is_none(), "revoked code must not consume");
    }

    #[tokio::test]
    async fn verify_and_consume_rejects_expired_code() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        // ttl = -1 to make it already expired
        let (_m, code) = EnrollmentService::mint_for_server(&db, &s, &u, -1)
            .await
            .expect("mint");
        let row = db
            .transaction::<_, _, AppError>(|tx| {
                Box::pin(async move { EnrollmentService::verify_and_consume_tx(tx, &code).await })
            })
            .await
            .expect("tx ok");
        assert!(row.is_none(), "expired code must not consume");
    }

    #[tokio::test]
    async fn verify_and_consume_single_use() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (_m, code) = EnrollmentService::mint_for_server(&db, &s, &u, 600)
            .await
            .expect("mint");

        let c1 = code.clone();
        let first = db
            .transaction::<_, _, AppError>(|tx| {
                Box::pin(async move { EnrollmentService::verify_and_consume_tx(tx, &c1).await })
            })
            .await
            .expect("tx ok");
        assert!(first.is_some());

        let c2 = code.clone();
        let second = db
            .transaction::<_, _, AppError>(|tx| {
                Box::pin(async move { EnrollmentService::verify_and_consume_tx(tx, &c2).await })
            })
            .await
            .expect("tx ok");
        assert!(second.is_none(), "second use must be rejected");
    }

    #[tokio::test]
    async fn partial_index_blocks_two_outstanding() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        EnrollmentService::mint_for_server(&db, &s, &u, 600)
            .await
            .expect("first mint");
        let second = EnrollmentService::mint_for_server(&db, &s, &u, 600).await;
        assert!(
            second.is_err(),
            "second mint must violate the partial unique index"
        );
    }

    #[tokio::test]
    async fn revoke_outstanding_then_mint_succeeds() {
        let (db, _t) = setup_test_db().await;
        let u = seed_user(&db).await;
        let s = seed_pending_server(&db).await;
        let (_first, _code) = EnrollmentService::mint_for_server(&db, &s, &u, 600)
            .await
            .expect("first mint");

        let sid = s.clone();
        let uid = u.clone();
        let (_second, _code2) = db
            .transaction::<_, _, AppError>(|tx| {
                Box::pin(async move {
                    EnrollmentService::revoke_outstanding_tx(tx, &sid).await?;
                    EnrollmentService::mint_for_server(tx, &sid, &uid, 600).await
                })
            })
            .await
            .expect("revoke + remint tx");
    }
}
