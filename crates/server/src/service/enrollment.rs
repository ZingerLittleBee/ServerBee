use chrono::{Duration, Utc};
use sea_orm::*;
use uuid::Uuid;

use crate::entity::agent_enrollment;
use crate::error::AppError;
use crate::service::auth::AuthService;

pub const DEFAULT_TTL_SECS: i64 = 600;
pub const MAX_TTL_SECS: i64 = 86_400;

pub struct EnrollmentService;

impl EnrollmentService {
    /// Mint a new single-use enrollment code. Returns the stored model and
    /// the plaintext code (shown to the operator exactly once).
    ///
    /// TODO: T6 will rewrite this method to accept a `target_server_id` and
    /// bind the enrollment 1:1 to a pre-created pending server. The
    /// `label` parameter is preserved here only so existing call sites
    /// continue to compile until T8 rewires them. The body currently
    /// returns an error so that any call at runtime fails loudly.
    #[allow(unused_variables)]
    pub async fn mint(
        db: &DatabaseConnection,
        created_by: &str,
        label: Option<String>,
        ttl_secs: i64,
    ) -> Result<(agent_enrollment::Model, String), AppError> {
        if ttl_secs <= 0 || ttl_secs > MAX_TTL_SECS {
            return Err(AppError::BadRequest(format!(
                "ttl_secs must be between 1 and {MAX_TTL_SECS}"
            )));
        }
        let _ = (Uuid::new_v4(), Utc::now(), Duration::seconds(0));
        let _ = AuthService::generate_session_token();
        Err(AppError::Internal(
            "EnrollmentService::mint is awaiting T6 rewrite".to_string(),
        ))
    }

    /// Verify a presented code and atomically consume it. Returns the
    /// enrollment row on success, `None` if unknown / expired / already used.
    pub async fn verify_and_consume(
        db: &DatabaseConnection,
        code: &str,
    ) -> Result<Option<agent_enrollment::Model>, AppError> {
        if code.len() < 8 {
            return Ok(None);
        }
        let prefix = &code[..8];
        let candidates = agent_enrollment::Entity::find()
            .filter(agent_enrollment::Column::CodePrefix.eq(prefix))
            .all(db)
            .await?;

        let now = Utc::now();
        for candidate in candidates {
            if AuthService::verify_password(code, &candidate.code_hash)? {
                if candidate.consumed_at.is_some() || candidate.expires_at < now {
                    return Ok(None);
                }
                let res = agent_enrollment::Entity::update_many()
                    .col_expr(
                        agent_enrollment::Column::ConsumedAt,
                        sea_orm::sea_query::Expr::value(now),
                    )
                    .filter(agent_enrollment::Column::Id.eq(&candidate.id))
                    .filter(agent_enrollment::Column::ConsumedAt.is_null())
                    .exec(db)
                    .await?;
                if res.rows_affected == 0 {
                    return Ok(None);
                }
                return Ok(Some(candidate));
            }
        }
        Ok(None)
    }

    pub async fn list(
        db: &DatabaseConnection,
    ) -> Result<Vec<agent_enrollment::Model>, AppError> {
        Ok(agent_enrollment::Entity::find()
            .order_by_desc(agent_enrollment::Column::CreatedAt)
            .all(db)
            .await?)
    }

    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let res = agent_enrollment::Entity::delete_by_id(id).exec(db).await?;
        if res.rows_affected == 0 {
            return Err(AppError::NotFound("Enrollment not found".to_string()));
        }
        Ok(())
    }

    /// Delete expired or consumed enrollments. Returns the number removed.
    pub async fn prune(db: &DatabaseConnection) -> Result<u64, AppError> {
        let now = Utc::now();
        let res = agent_enrollment::Entity::delete_many()
            .filter(
                agent_enrollment::Column::ConsumedAt
                    .is_not_null()
                    .or(agent_enrollment::Column::ExpiresAt.lt(now)),
            )
            .exec(db)
            .await?;
        Ok(res.rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::user;
    use crate::test_utils::setup_test_db;

    /// Seed a user so the `created_by` FK on `agent_enrollments` is satisfied.
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
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed user");
        id
    }

    #[tokio::test]
    #[ignore = "TODO: T6 will rewrite mint() to bind enrollments to a target server"]
    async fn mint_returns_plaintext_and_stores_hash() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (model, code) = EnrollmentService::mint(&db, &uid, None, 600)
            .await
            .expect("mint");
        assert_eq!(code.len(), 43, "code is a 43-char base64url token");
        assert_ne!(model.code_hash, code, "hash must not equal plaintext");
        assert!(model.code_hash.starts_with("$argon2"));
        assert_eq!(model.code_prefix, &code[..8]);
        assert!(model.consumed_at.is_none());
    }

    #[tokio::test]
    async fn mint_rejects_bad_ttl() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        assert!(EnrollmentService::mint(&db, &uid, None, 0).await.is_err());
        assert!(
            EnrollmentService::mint(&db, &uid, None, MAX_TTL_SECS + 1)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "TODO: T6 will rewrite mint(); verify_and_consume tests depend on it"]
    async fn verify_and_consume_succeeds_once() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (_m, code) = EnrollmentService::mint(&db, &uid, None, 600)
            .await
            .expect("mint");

        let first = EnrollmentService::verify_and_consume(&db, &code)
            .await
            .expect("verify ok");
        assert!(first.is_some(), "first redemption succeeds");

        let second = EnrollmentService::verify_and_consume(&db, &code)
            .await
            .expect("verify ok");
        assert!(second.is_none(), "second redemption rejected (single-use)");
    }

    #[tokio::test]
    #[ignore = "TODO: T6 will rewrite mint(); expiry test depends on it"]
    async fn verify_rejects_expired() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (_m, code) = EnrollmentService::mint(&db, &uid, None, 600)
            .await
            .expect("mint");
        agent_enrollment::Entity::update_many()
            .col_expr(
                agent_enrollment::Column::ExpiresAt,
                sea_orm::sea_query::Expr::value(Utc::now() - Duration::seconds(10)),
            )
            .exec(&db)
            .await
            .expect("expire");

        let r = EnrollmentService::verify_and_consume(&db, &code)
            .await
            .expect("verify ok");
        assert!(r.is_none(), "expired code rejected");
    }

    #[tokio::test]
    async fn verify_rejects_unknown_code() {
        let (db, _tmp) = setup_test_db().await;
        let r = EnrollmentService::verify_and_consume(&db, "totally-wrong-code-value-xyz")
            .await
            .expect("verify ok");
        assert!(r.is_none());
    }

    #[tokio::test]
    #[ignore = "TODO: T6 will rewrite mint(); prune test depends on it"]
    async fn prune_removes_expired_and_consumed() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (_m, code) = EnrollmentService::mint(&db, &uid, None, 600).await.unwrap();
        EnrollmentService::verify_and_consume(&db, &code)
            .await
            .unwrap();
        let removed = EnrollmentService::prune(&db).await.expect("prune");
        assert_eq!(removed, 1, "consumed enrollment pruned");
    }

    #[tokio::test]
    #[ignore = "TODO: T6 will rewrite mint(); concurrency test depends on it"]
    async fn concurrent_redemption_consumes_exactly_once() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (_m, code) = EnrollmentService::mint(&db, &uid, None, 600)
            .await
            .expect("mint");

        let db2 = db.clone();
        let c1 = code.clone();
        let c2 = code.clone();
        let h1 = tokio::spawn(async move {
            EnrollmentService::verify_and_consume(&db, &c1).await
        });
        let h2 = tokio::spawn(async move {
            EnrollmentService::verify_and_consume(&db2, &c2).await
        });
        let r1 = h1.await.expect("join1").expect("no db err");
        let r2 = h2.await.expect("join2").expect("no db err");

        let successes = [r1.is_some(), r2.is_some()]
            .iter()
            .filter(|&&s| s)
            .count();
        assert_eq!(successes, 1, "exactly one concurrent redemption must succeed");
    }

    #[tokio::test]
    #[ignore = "TODO: T6 will rewrite mint(); expired-unconsumed prune test depends on it"]
    async fn prune_removes_expired_unconsumed() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (_m, _code) = EnrollmentService::mint(&db, &uid, None, 600)
            .await
            .unwrap();
        agent_enrollment::Entity::update_many()
            .col_expr(
                agent_enrollment::Column::ExpiresAt,
                sea_orm::sea_query::Expr::value(Utc::now() - Duration::seconds(10)),
            )
            .exec(&db)
            .await
            .unwrap();
        let removed = EnrollmentService::prune(&db).await.expect("prune");
        assert_eq!(removed, 1, "expired-but-unconsumed enrollment pruned");
    }
}
