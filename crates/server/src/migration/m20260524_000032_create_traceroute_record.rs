// crates/server/src/migration/m20260524_000032_create_traceroute_record.rs
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260524_000032_create_traceroute_record"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS traceroute_record (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL,
                target TEXT NOT NULL,
                protocol TEXT NOT NULL
                    CHECK (protocol IN ('icmp', 'udp', 'tcp', 'legacy')),
                started_at INTEGER NOT NULL,
                completed_at INTEGER,
                total_rounds INTEGER NOT NULL,
                completed_rounds INTEGER NOT NULL,
                hops_json TEXT NOT NULL,
                error TEXT,
                FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_traceroute_record_server_started
                ON traceroute_record(server_id, started_at DESC)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
