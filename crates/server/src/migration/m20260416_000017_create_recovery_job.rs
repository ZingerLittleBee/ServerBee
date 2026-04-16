use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260416_000017_create_recovery_job"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS recovery_job (
                job_id TEXT PRIMARY KEY NOT NULL,
                target_server_id TEXT NOT NULL,
                source_server_id TEXT NOT NULL,
                status TEXT NOT NULL,
                stage TEXT NOT NULL,
                checkpoint_json TEXT NULL,
                error TEXT NULL,
                started_at DATETIME NOT NULL,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL,
                last_heartbeat_at DATETIME NULL
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_recovery_job_target_running
                ON recovery_job(target_server_id)
                WHERE status = 'running'",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_recovery_job_source_running
                ON recovery_job(source_server_id)
                WHERE status = 'running'",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS trg_recovery_job_running_insert
                BEFORE INSERT ON recovery_job
                WHEN NEW.status = 'running'
                BEGIN
                    SELECT RAISE(ABORT, 'recovery_job_active_conflict')
                    WHERE EXISTS (
                        SELECT 1
                        FROM recovery_job
                        WHERE status = 'running'
                          AND job_id <> NEW.job_id
                          AND (
                              target_server_id IN (NEW.target_server_id, NEW.source_server_id)
                              OR source_server_id IN (NEW.target_server_id, NEW.source_server_id)
                          )
                    );
                END",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS trg_recovery_job_running_update
                BEFORE UPDATE OF target_server_id, source_server_id, status ON recovery_job
                WHEN NEW.status = 'running'
                BEGIN
                    SELECT RAISE(ABORT, 'recovery_job_active_conflict')
                    WHERE EXISTS (
                        SELECT 1
                        FROM recovery_job
                        WHERE status = 'running'
                          AND job_id <> NEW.job_id
                          AND (
                              target_server_id IN (NEW.target_server_id, NEW.source_server_id)
                              OR source_server_id IN (NEW.target_server_id, NEW.source_server_id)
                          )
                    );
                END",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
