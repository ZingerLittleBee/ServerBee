use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260315_000004_network_probe"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TABLE network_probe_target (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                location TEXT NOT NULL,
                target TEXT NOT NULL,
                probe_type TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TABLE network_probe_config (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(server_id, target_id)
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE TABLE network_probe_record (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL,
                avg_latency REAL,
                min_latency REAL,
                max_latency REAL,
                packet_loss REAL NOT NULL,
                packet_sent INTEGER NOT NULL,
                packet_received INTEGER NOT NULL,
                timestamp TEXT NOT NULL
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX idx_network_probe_record_lookup ON network_probe_record (server_id, target_id, timestamp)"
        ).await?;

        db.execute_unprepared(
            "CREATE TABLE network_probe_record_hourly (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL,
                avg_latency REAL,
                min_latency REAL,
                max_latency REAL,
                avg_packet_loss REAL NOT NULL,
                sample_count INTEGER NOT NULL,
                hour TEXT NOT NULL,
                UNIQUE(server_id, target_id, hour)
            )",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_record_hourly")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_record")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_config")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_target")
            .await?;
        Ok(())
    }
}
