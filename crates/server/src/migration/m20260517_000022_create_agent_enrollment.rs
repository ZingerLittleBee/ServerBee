use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260517_000022_create_agent_enrollment"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS agent_enrollments (
                id TEXT PRIMARY KEY NOT NULL,
                code_hash TEXT NOT NULL,
                code_prefix TEXT NOT NULL,
                label TEXT,
                created_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                expires_at DATETIME NOT NULL,
                consumed_at DATETIME,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_agent_enrollments_code_prefix
                ON agent_enrollments(code_prefix)",
        )
        .await?;
        db.execute_unprepared(
            "DELETE FROM configs WHERE key = 'auto_discovery_key'",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
