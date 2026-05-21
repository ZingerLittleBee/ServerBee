use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260521_000027_create_block_list"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS block_list (
                id TEXT PRIMARY KEY NOT NULL,
                target TEXT NOT NULL,
                family INTEGER NOT NULL,
                cover_type TEXT NOT NULL,
                server_ids_json TEXT,
                comment TEXT,
                origin TEXT NOT NULL,
                origin_event_id TEXT,
                origin_rule_id TEXT,
                created_by TEXT,
                created_at TIMESTAMP WITH TIME ZONE NOT NULL
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_block_list_target_unique
                ON block_list(target)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_block_list_created_at
                ON block_list(created_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_block_list_origin
                ON block_list(origin)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
