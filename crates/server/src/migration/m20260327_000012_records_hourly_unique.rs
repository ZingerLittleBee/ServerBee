use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260327_000012_records_hourly_unique"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Step 1: Deduplicate existing data
        db.execute_unprepared(
            "DELETE FROM records_hourly WHERE id NOT IN (
                SELECT MAX(id) FROM records_hourly
                GROUP BY server_id, strftime('%Y-%m-%d %H:00:00', time)
            )",
        )
        .await?;

        // Step 2: Align existing timestamps to hour boundaries
        db.execute_unprepared(
            "UPDATE records_hourly SET time = strftime('%Y-%m-%d %H:00:00', time)",
        )
        .await?;

        // Step 3: Add unique index
        db.execute_unprepared(
            "CREATE UNIQUE INDEX idx_records_hourly_server_time ON records_hourly(server_id, time)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
