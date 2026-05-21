use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260521_000026_backfill_capability_default"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Backfill the CAP_SECURITY_EVENTS (256) bit on any existing server rows
        // that still carry the pre-existing default capability mask (60). New
        // server rows already include the security_events bit because
        // CAP_DEFAULT is now 316.
        let db = manager.get_connection();
        db.execute_unprepared(
            "UPDATE servers SET capabilities = capabilities | 256 WHERE capabilities = 60",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
