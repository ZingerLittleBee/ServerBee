use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260619_000071_add_password_changed_at"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Records when a user's password was last changed. Mobile refresh uses
        // this as a token-version check: a refresh token whose mobile_session
        // was issued before this timestamp is rejected, so a password change /
        // admin reset invalidates stolen refresh tokens even if the session row
        // survives a revocation race. NULL means "never changed" (initial
        // password still in effect), which never rejects.
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE users ADD COLUMN password_changed_at TIMESTAMP")
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
