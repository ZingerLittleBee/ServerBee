use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260621_000072_add_geo_manual"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Marks a server whose `region`/`country_code` (the flag shown in the UI)
        // were set manually by an operator to correct a wrong GeoIP guess. When
        // true, the agent SystemInfo and IP-change code paths must NOT overwrite
        // those two columns with auto-detected values. Default false = follow
        // GeoIP automatically, which is the historical behavior for every row.
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN geo_manual BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
