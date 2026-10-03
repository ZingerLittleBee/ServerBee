use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE mobile_session_revocation_proofs (
                id TEXT PRIMARY KEY NOT NULL,
                mobile_session_id TEXT NOT NULL REFERENCES mobile_sessions(id) ON DELETE CASCADE,
                token_hash TEXT NOT NULL,
                UNIQUE(mobile_session_id, token_hash)
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_mobile_revocation_proofs_token_hash ON mobile_session_revocation_proofs(token_hash)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
