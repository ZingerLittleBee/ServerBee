use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE mobile_push_registrations ADD COLUMN content_key_id TEXT;
             ALTER TABLE mobile_push_registrations ADD COLUMN content_key TEXT;
             ALTER TABLE mobile_push_registrations ADD COLUMN deployment_id TEXT;
             CREATE TABLE mobile_push_migrations (
                installation_id TEXT NOT NULL,
                user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                PRIMARY KEY (installation_id, user_id)
             );",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
