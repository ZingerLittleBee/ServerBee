use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sea_orm::*;

use crate::entity::session;
use crate::state::AppState;

/// Deletes all sessions whose `expires_at` is strictly before `now`.
///
/// Returns the number of rows removed so callers can log it. Extracted from the
/// `run` loop body so the per-tick database work is unit-testable without the
/// infinite loop or the full `AppState`.
pub(crate) async fn cleanup_once(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> u64 {
    match session::Entity::delete_many()
        .filter(session::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
    {
        Ok(result) if result.rows_affected > 0 => {
            tracing::info!("Cleaned up {} expired sessions", result.rows_affected);
            result.rows_affected
        }
        Ok(result) => result.rows_affected,
        Err(e) => {
            tracing::error!("Failed to clean up expired sessions: {e}");
            0
        }
    }
}

/// Periodically deletes expired sessions and cleans up in-memory caches (every 3600 seconds).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;

        let now = Utc::now();

        // Clean expired sessions from database
        cleanup_once(&state.db, now).await;

        // Clean expired pending TOTP secrets (older than 10 minutes)
        let totp_cutoff = now - chrono::Duration::minutes(10);
        state.pending_totp.retain(|_, v| v.created_at > totp_cutoff);

        // Clean expired login rate limit entries (older than 15 minutes)
        let rate_cutoff = now - chrono::Duration::minutes(15);
        state
            .login_rate_limit
            .retain(|_, v| v.window_start > rate_cutoff);

        // Clean expired registration rate limit entries (older than 15 minutes)
        state
            .register_rate_limit
            .retain(|_, v| v.window_start > rate_cutoff);

        // Clean expired public-status rate limit entries (older than 5 minutes;
        // the bucket window is 60s so this is a generous safety margin).
        let public_cutoff = now - chrono::Duration::minutes(5);
        state
            .public_rate_limit
            .retain(|_, v| v.window_start > public_cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{session, user};
    use crate::test_utils::setup_test_db;
    use chrono::TimeZone;
    use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, PaginatorTrait, Set};

    /// Seed a parent user row so sessions satisfy the `user_id` foreign key.
    async fn insert_test_user(db: &DatabaseConnection, id: &str) {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        user::ActiveModel {
            id: Set(id.to_string()),
            username: Set(format!("user-{id}")),
            password_hash: Set("hash".to_string()),
            role: Set("admin".to_string()),
            totp_secret: Set(None),
            must_change_password: Set(false),
            password_changed_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("insert test user should succeed");
    }

    async fn insert_test_session(
        db: &DatabaseConnection,
        id: &str,
        user_id: &str,
        expires_at: chrono::DateTime<Utc>,
    ) {
        let created_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        session::ActiveModel {
            id: Set(id.to_string()),
            user_id: Set(user_id.to_string()),
            token: Set(format!("token-{id}")),
            ip: Set("127.0.0.1".to_string()),
            user_agent: Set("test-agent".to_string()),
            expires_at: Set(expires_at),
            created_at: Set(created_at),
            source: Set("web".to_string()),
            mobile_session_id: Set(None),
        }
        .insert(db)
        .await
        .expect("insert test session should succeed");
    }

    /// Empty input: no sessions present -> no-op, returns 0.
    #[tokio::test]
    async fn cleanup_once_with_no_sessions_is_noop() {
        let (db, _tmp) = setup_test_db().await;
        let now = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();

        let removed = cleanup_once(&db, now).await;

        assert_eq!(removed, 0);
        assert_eq!(
            session::Entity::find().count(&db).await.unwrap(),
            0,
            "no sessions should exist after cleanup on empty table"
        );
    }

    /// Happy path: only sessions whose `expires_at < now` are removed; future
    /// sessions survive. Also exercises the equality boundary (`expires_at == now`
    /// must be kept, since the filter is strictly less-than).
    #[tokio::test]
    async fn cleanup_once_removes_only_expired_sessions() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_user(&db, "u1").await;

        let now = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        let past = Utc.with_ymd_and_hms(2026, 6, 1, 11, 0, 0).unwrap();
        let future = Utc.with_ymd_and_hms(2026, 6, 1, 13, 0, 0).unwrap();

        // Expired (strictly before now) -> removed.
        insert_test_session(&db, "expired", "u1", past).await;
        // Boundary: exactly at now -> kept (filter is `< now`, not `<= now`).
        insert_test_session(&db, "boundary", "u1", now).await;
        // Future -> kept.
        insert_test_session(&db, "future", "u1", future).await;

        let removed = cleanup_once(&db, now).await;

        assert_eq!(removed, 1, "exactly one expired session should be removed");

        let remaining: Vec<String> = session::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert!(
            !remaining.contains(&"expired".to_string()),
            "expired session must be deleted"
        );
        assert!(
            remaining.contains(&"boundary".to_string()),
            "session expiring exactly at now must be kept"
        );
        assert!(
            remaining.contains(&"future".to_string()),
            "future session must be kept"
        );
        assert_eq!(remaining.len(), 2);
    }

    /// All sessions expired -> all removed, returns the full count.
    #[tokio::test]
    async fn cleanup_once_removes_all_when_all_expired() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_user(&db, "u1").await;

        let now = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        insert_test_session(
            &db,
            "old1",
            "u1",
            Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        )
        .await;
        insert_test_session(
            &db,
            "old2",
            "u1",
            Utc.with_ymd_and_hms(2026, 5, 31, 23, 59, 59).unwrap(),
        )
        .await;

        let removed = cleanup_once(&db, now).await;

        assert_eq!(removed, 2);
        assert_eq!(session::Entity::find().count(&db).await.unwrap(), 0);
    }
}
