use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260525_000033_ip_quality_snapshot_extra_fields"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE ip_quality_snapshot ADD COLUMN is_tor INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE ip_quality_snapshot ADD COLUMN is_abuser INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE ip_quality_snapshot ADD COLUMN is_mobile INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE ip_quality_snapshot ADD COLUMN asn_abuser_score INTEGER",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE ip_quality_snapshot ADD COLUMN abuse_email TEXT",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
