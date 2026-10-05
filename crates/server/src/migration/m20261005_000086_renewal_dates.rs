use chrono::{DateTime, Utc};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE servers ADD COLUMN renewal_state TEXT")
            .await?;
        // Encode historical instants with the same RFC3339 representation used
        // by the application, without rewriting the predecessor expiry column.
        let backend = db.get_database_backend();
        for row in db
            .query_all(sea_orm::Statement::from_string(
                backend,
                "SELECT id, expired_at FROM servers",
            ))
            .await?
        {
            let id: String = row.try_get("", "id")?;
            let expiry: Option<DateTime<Utc>> = row.try_get("", "expired_at")?;
            let state =
                serde_json::json!({"billing_timezone":"UTC", "confirmed_expired_at":expiry})
                    .to_string();
            db.execute(sea_orm::Statement::from_sql_and_values(
                backend,
                "UPDATE servers SET renewal_state = ? WHERE id = ?",
                [state.into(), id.into()],
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
