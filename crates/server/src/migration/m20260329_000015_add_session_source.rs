use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260329_000015_add_session_source"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE sessions ADD COLUMN source TEXT NOT NULL DEFAULT 'web'",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE sessions ADD COLUMN mobile_session_id TEXT REFERENCES mobile_sessions(id)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
