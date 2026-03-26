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

        // Step 1: Deduplicate existing data.
        // Existing time values are RFC3339: 'YYYY-MM-DDTHH:MM:SS+00:00'
        // substr(time,1,13) extracts 'YYYY-MM-DDTHH' as the hour bucket key.
        db.execute_unprepared(
            "DELETE FROM records_hourly WHERE id NOT IN (
                SELECT MAX(id) FROM records_hourly
                GROUP BY server_id, substr(time, 1, 13)
            )",
        )
        .await?;

        // Step 2: Truncate existing timestamps to hour boundary in RFC3339 format.
        // 'YYYY-MM-DDTHH:MM:SS+00:00' → 'YYYY-MM-DDTHH:00:00+00:00'
        // This matches sqlx/sea-orm's DateTimeUtc storage format.
        db.execute_unprepared(
            "UPDATE records_hourly SET time = substr(time, 1, 13) || ':00:00+00:00'",
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
