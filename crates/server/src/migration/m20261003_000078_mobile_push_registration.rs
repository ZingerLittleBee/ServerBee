use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE mobile_push_registrations (
                installation_id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                mobile_session_id TEXT NOT NULL REFERENCES mobile_sessions(id) ON DELETE CASCADE,
                revision INTEGER NOT NULL DEFAULT 0,
                enabled BOOLEAN NOT NULL DEFAULT 0,
                alerts BOOLEAN NOT NULL DEFAULT 0,
                security BOOLEAN NOT NULL DEFAULT 0,
                task_failure BOOLEAN NOT NULL DEFAULT 0,
                task_success BOOLEAN NOT NULL DEFAULT 0,
                device_token TEXT,
                environment TEXT CHECK(environment IN ('sandbox', 'production')),
                key_id TEXT,
                grant_id TEXT,
                grant_token TEXT,
                grant_expires_at TEXT,
                updated_at TEXT NOT NULL
            )",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
