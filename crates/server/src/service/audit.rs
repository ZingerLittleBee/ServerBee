use chrono::Utc;
use sea_orm::*;

use crate::entity::{audit_log, user};
use crate::error::AppError;

pub struct AuditService;

#[derive(Debug, Default)]
pub struct AuditListFilters {
    pub action: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug)]
pub struct AuditUserOption {
    pub id: String,
    pub label: String,
}

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
        filters: AuditListFilters,
    ) -> Result<(Vec<audit_log::Model>, u64), AppError> {
        let mut query = audit_log::Entity::find();
        if let Some(action) = filters.action.as_ref().filter(|s| !s.is_empty()) {
            query = query.filter(audit_log::Column::Action.eq(action.clone()));
        }
        if let Some(user_id) = filters.user_id.as_ref().filter(|s| !s.is_empty()) {
            query = query.filter(audit_log::Column::UserId.eq(user_id.clone()));
        }

        let total = query.clone().count(db).await?;

        let entries = query
            .order_by_desc(audit_log::Column::CreatedAt)
            .limit(limit)
            .offset(offset)
            .all(db)
            .await?;

        Ok((entries, total))
    }

    /// Distinct action values present in the audit log table, sorted ascending.
    pub async fn distinct_actions(db: &DatabaseConnection) -> Result<Vec<String>, AppError> {
        let rows: Vec<String> = audit_log::Entity::find()
            .select_only()
            .column(audit_log::Column::Action)
            .distinct()
            .order_by_asc(audit_log::Column::Action)
            .into_tuple()
            .all(db)
            .await?;
        Ok(rows)
    }

    /// Distinct user_ids present in the audit log, paired with their username when known.
    /// Entries without a matching user row (e.g. "system") use the raw id as the label.
    pub async fn distinct_users(
        db: &DatabaseConnection,
    ) -> Result<Vec<AuditUserOption>, AppError> {
        let ids: Vec<String> = audit_log::Entity::find()
            .select_only()
            .column(audit_log::Column::UserId)
            .distinct()
            .order_by_asc(audit_log::Column::UserId)
            .into_tuple()
            .all(db)
            .await?;

        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let users: Vec<(String, String)> = user::Entity::find()
            .select_only()
            .column(user::Column::Id)
            .column(user::Column::Username)
            .filter(user::Column::Id.is_in(ids.clone()))
            .into_tuple()
            .all(db)
            .await?;

        let lookup: std::collections::HashMap<String, String> = users.into_iter().collect();
        Ok(ids
            .into_iter()
            .map(|id| {
                let label = lookup.get(&id).cloned().unwrap_or_else(|| id.clone());
                AuditUserOption { id, label }
            })
            .collect())
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
        let (entries, total) = AuditService::list(&db, 10, 0, AuditListFilters::default())
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

        let (entries, total) = AuditService::list(&db, 10, 0, AuditListFilters::default())
            .await
            .expect("list should succeed");

        assert_eq!(total, 1, "Should have one entry");
        assert_eq!(entries[0].detail, None, "detail should be None");
        assert_eq!(entries[0].action, "logout", "action should match");
    }

    #[tokio::test]
    async fn test_list_filters_by_action_and_user() {
        let (db, _tmp) = setup_test_db().await;

        AuditService::log(&db, "alice", "login", None, "1.1.1.1")
            .await
            .unwrap();
        AuditService::log(&db, "alice", "logout", None, "1.1.1.1")
            .await
            .unwrap();
        AuditService::log(&db, "bob", "login", None, "2.2.2.2")
            .await
            .unwrap();

        let (entries, total) = AuditService::list(
            &db,
            10,
            0,
            AuditListFilters {
                action: Some("login".into()),
                user_id: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(total, 2);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.action == "login"));

        let (entries, total) = AuditService::list(
            &db,
            10,
            0,
            AuditListFilters {
                action: Some("login".into()),
                user_id: Some("alice".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(total, 1);
        assert_eq!(entries[0].user_id, "alice");
        assert_eq!(entries[0].action, "login");

        let actions = AuditService::distinct_actions(&db).await.unwrap();
        assert_eq!(actions, vec!["login".to_string(), "logout".to_string()]);

        let users = AuditService::distinct_users(&db).await.unwrap();
        // Both users present in audit log but neither exists in the users table, so the label
        // falls back to the raw id.
        let ids: Vec<&str> = users.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(ids, vec!["alice", "bob"]);
        assert!(users.iter().all(|u| u.label == u.id));
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

        let (entries, total) = AuditService::list(&db, 10, 0, AuditListFilters::default())
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
