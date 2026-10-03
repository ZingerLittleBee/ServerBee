use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "CREATE TABLE mobile_push_outbox (
                event_id TEXT NOT NULL,
                installation_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                mobile_session_id TEXT NOT NULL,
                registration_revision INTEGER NOT NULL,
                recipient_role TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                envelope TEXT,
                outcome TEXT NOT NULL DEFAULT 'pending',
                reason TEXT NOT NULL DEFAULT 'Queued',
                attempts INTEGER NOT NULL DEFAULT 0,
                next_attempt_at INTEGER NOT NULL,
                lease_id TEXT,
                lease_until INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (event_id, installation_id)
             );
             CREATE INDEX idx_mobile_push_outbox_due ON mobile_push_outbox(outcome, next_attempt_at, lease_until);"
        ).await?;
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
