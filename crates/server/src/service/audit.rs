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
