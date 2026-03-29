use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260314_000003_add_capabilities"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN capabilities INTEGER NOT NULL DEFAULT 56",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN protocol_version INTEGER NOT NULL DEFAULT 1",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite does not support DROP COLUMN in older versions;
        // for simplicity, this migration is not reversible.
        Ok(())
    }
}
