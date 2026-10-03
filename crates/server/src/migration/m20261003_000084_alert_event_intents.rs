use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "CREATE TABLE alert_event_intents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                rule_id TEXT NOT NULL,
                server_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                trigger_mode TEXT NOT NULL,
                first_triggered_at TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                count INTEGER NOT NULL,
                should_notify BOOLEAN NOT NULL
             );
             CREATE INDEX idx_alert_event_intents_dimension ON alert_event_intents(rule_id, server_id, id);"
        ).await?;
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
