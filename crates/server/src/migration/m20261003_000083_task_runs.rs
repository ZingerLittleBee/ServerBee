use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE task_runs (
                run_id TEXT PRIMARY KEY NOT NULL,
                task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                owner_id TEXT NOT NULL,
                manual BOOLEAN NOT NULL,
                targets_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'running',
                completed_at INTEGER,
                summary_json TEXT
            );
            CREATE INDEX idx_task_runs_task ON task_runs(task_id);
            CREATE INDEX idx_task_runs_recovery ON task_runs(status, completed_at);
            ALTER TABLE mobile_push_outbox ADD COLUMN task_run_id TEXT;",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
