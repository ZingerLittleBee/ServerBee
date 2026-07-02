use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260702_000074_hash_existing_session_tokens"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // `sessions.token` now stores a SHA-256 hash of the token instead of the
        // plaintext. Existing rows hold plaintext that cannot be rehashed in
        // place (SQLite has no sha256), and keeping them would both fail the new
        // hash-based lookup and leave replayable plaintext in any snapshot. Drop
        // all existing sessions: web users re-login, mobile clients transparently
        // refresh (the refresh secret lives in mobile_session, already hashed).
        let db = manager.get_connection();
        db.execute_unprepared("DELETE FROM sessions").await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
