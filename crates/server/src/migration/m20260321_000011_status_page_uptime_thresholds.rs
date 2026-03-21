use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260321_000011_status_page_uptime_thresholds"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // SQLite requires separate ALTER TABLE statements for each column
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN uptime_yellow_threshold REAL NOT NULL DEFAULT 100.0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN uptime_red_threshold REAL NOT NULL DEFAULT 95.0",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
