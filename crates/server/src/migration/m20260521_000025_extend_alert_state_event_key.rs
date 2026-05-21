use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260521_000025_extend_alert_state_event_key"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            "ALTER TABLE alert_states ADD COLUMN event_key TEXT NOT NULL DEFAULT ''",
        )
        .await?;

        // Drop the legacy (rule_id, server_id) unique index so we can replace it
        // with a unique index that also includes event_key.
        db.execute_unprepared(
            "DROP INDEX IF EXISTS idx_alert_states_rule_id_server_id",
        )
        .await?;

        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_alert_states_rule_id_server_id_event_key
                ON alert_states(rule_id, server_id, event_key)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
