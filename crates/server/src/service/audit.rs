use chrono::Utc;
use sea_orm::*;

use crate::entity::audit_log;
use crate::error::AppError;

pub struct AuditService;

impl AuditService {
    /// Log an audit event.
    pub async fn log(
        db: &DatabaseConnection,
        user_id: &str,
        action: &str,
        detail: Option<&str>,
        ip: &str,
    ) -> Result<(), AppError> {
        let entry = audit_log::ActiveModel {
            id: NotSet,
            user_id: Set(user_id.to_string()),
            action: Set(action.to_string()),
            detail: Set(detail.map(|s| s.to_string())),
            ip: Set(ip.to_string()),
            created_at: Set(Utc::now()),
        };

        entry.insert(db).await?;
        Ok(())
    }

    /// List audit log entries, ordered by newest first.
    pub async fn list(
        db: &DatabaseConnection,
        limit: u64,
        offset: u64,
    ) -> Result<(Vec<audit_log::Model>, u64), AppError> {
        let total = audit_log::Entity::find().count(db).await?;

        let entries = audit_log::Entity::find()
            .order_by_desc(audit_log::Column::CreatedAt)
            .limit(limit)
            .offset(offset)
            .all(db)
            .await?;

        Ok((entries, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn test_log_and_list() {
        let (db, _tmp) = setup_test_db().await;

        // Log an audit action
        AuditService::log(&db, "user-1", "login", Some("via password"), "127.0.0.1")
            .await
            .expect("log should succeed");

        // List and verify the entry appears
        let (entries, total) = AuditService::list(&db, 10, 0)
            .await
            .expect("list should succeed");

        assert_eq!(total, 1, "Should have exactly one audit log entry");
        assert_eq!(entries.len(), 1, "entries vec should have one item");

        let entry = &entries[0];
        assert_eq!(entry.user_id, "user-1", "user_id should match");
        assert_eq!(entry.action, "login", "action should match");
        assert_eq!(
            entry.detail,
            Some("via password".to_string()),
            "detail should match"
        );
        assert_eq!(entry.ip, "127.0.0.1", "ip should match");
    }

    #[tokio::test]
    async fn test_log_without_detail() {
        let (db, _tmp) = setup_test_db().await;

        AuditService::log(&db, "user-2", "logout", None, "10.0.0.1")
            .await
            .expect("log without detail should succeed");

        let (entries, total) = AuditService::list(&db, 10, 0)
            .await
            .expect("list should succeed");

        assert_eq!(total, 1, "Should have one entry");
        assert_eq!(entries[0].detail, None, "detail should be None");
        assert_eq!(entries[0].action, "logout", "action should match");
    }

    #[tokio::test]
    async fn test_list_ordering() {
        let (db, _tmp) = setup_test_db().await;

        // Insert two entries — list should return newest first
        AuditService::log(&db, "user-3", "action-first", None, "1.1.1.1")
            .await
            .expect("first log should succeed");
        AuditService::log(&db, "user-3", "action-second", None, "1.1.1.1")
            .await
            .expect("second log should succeed");

        let (entries, total) = AuditService::list(&db, 10, 0)
            .await
            .expect("list should succeed");

        assert_eq!(total, 2, "Should have two entries");
        // Newest first: action-second should come before action-first
        assert_eq!(
            entries[0].action, "action-second",
            "newest entry should be first"
        );
        assert_eq!(
            entries[1].action, "action-first",
            "older entry should be second"
        );
    }
}
