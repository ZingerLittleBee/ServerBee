use sea_orm::DbErr;

pub(crate) fn is_unique_violation(err: &DbErr) -> bool {
    let message = err.to_string();
    message.contains("UNIQUE constraint failed") || message.contains("UNIQUE")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::user;
    use crate::test_utils::setup_test_db;
    use chrono::{TimeZone, Utc};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, Statement};

    // Build a deterministic `user` ActiveModel with a caller-supplied id and username.
    fn make_user(id: &str, username: &str) -> user::ActiveModel {
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        user::ActiveModel {
            id: Set(id.to_string()),
            username: Set(username.to_string()),
            password_hash: Set("$argon2id$v=19$m=19456,t=2,p=1$x$x".to_string()),
            role: Set("admin".to_string()),
            totp_secret: Set(None),
            must_change_password: Set(false),
            password_changed_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
    }

    // A duplicate primary key insert yields a real UNIQUE-violation DbErr -> true.
    #[tokio::test]
    async fn duplicate_primary_key_is_unique_violation() {
        let (db, _tmp) = setup_test_db().await;
        make_user("dup-id", "alice").insert(&db).await.expect("first insert");
        // Same primary key, different username: primary-key UNIQUE constraint trips.
        let err = make_user("dup-id", "bob")
            .insert(&db)
            .await
            .err()
            .expect("duplicate primary key should fail");
        assert!(is_unique_violation(&err), "expected unique violation, got: {err}");
    }

    // A duplicate value on a `#[sea_orm(unique)]` column also reports as a UNIQUE violation -> true.
    #[tokio::test]
    async fn duplicate_unique_column_is_unique_violation() {
        let (db, _tmp) = setup_test_db().await;
        make_user("id-1", "carol").insert(&db).await.expect("first insert");
        // Different primary key but the same unique `username` column.
        let err = make_user("id-2", "carol")
            .insert(&db)
            .await
            .err()
            .expect("duplicate username should fail");
        assert!(is_unique_violation(&err), "expected unique violation, got: {err}");
    }

    // A non-unique DbErr (querying a table that does not exist) -> false.
    #[tokio::test]
    async fn non_unique_db_error_is_not_unique_violation() {
        let (db, _tmp) = setup_test_db().await;
        let err = db
            .execute(Statement::from_string(
                db.get_database_backend(),
                "SELECT * FROM this_table_does_not_exist".to_string(),
            ))
            .await
            .err()
            .expect("querying a missing table should fail");
        assert!(
            !is_unique_violation(&err),
            "missing-table error must not be a unique violation, got: {err}"
        );
    }

    // A synthetic error whose message lacks any UNIQUE marker -> false (covers the negative branch deterministically).
    #[test]
    fn unrelated_error_message_is_not_unique_violation() {
        let err = DbErr::Custom("connection reset by peer".to_string());
        assert!(!is_unique_violation(&err));
    }

    // The bare "UNIQUE" substring branch alone is enough to return true even without the full message.
    #[test]
    fn bare_unique_keyword_is_unique_violation() {
        let err = DbErr::Custom("some UNIQUE index conflict".to_string());
        assert!(is_unique_violation(&err));
    }
}
