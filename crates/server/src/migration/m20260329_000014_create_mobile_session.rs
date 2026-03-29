use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260329_000014_create_mobile_session"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS mobile_sessions (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id),
                refresh_token_hash TEXT NOT NULL,
                installation_id TEXT NOT NULL,
                device_name TEXT NOT NULL DEFAULT '',
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at DATETIME NOT NULL,
                last_used_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_mobile_sessions_user_id ON mobile_sessions(user_id)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_mobile_sessions_installation_id ON mobile_sessions(installation_id)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
