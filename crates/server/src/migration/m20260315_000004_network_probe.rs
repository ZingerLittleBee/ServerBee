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
                is_builtin INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )"
        ).await?;

        db.execute_unprepared(
            "CREATE TABLE network_probe_config (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL REFERENCES network_probe_target(id) ON DELETE CASCADE,
                created_at TEXT NOT NULL,
                UNIQUE(server_id, target_id)
            )"
        ).await?;

        db.execute_unprepared(
            "CREATE TABLE network_probe_record (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL REFERENCES network_probe_target(id) ON DELETE CASCADE,
                avg_latency REAL,
                min_latency REAL,
                max_latency REAL,
                packet_loss REAL NOT NULL,
                packet_sent INTEGER NOT NULL,
                packet_received INTEGER NOT NULL,
                timestamp TEXT NOT NULL
            )"
        ).await?;

        db.execute_unprepared(
            "CREATE INDEX idx_network_probe_record_lookup ON network_probe_record (server_id, target_id, timestamp)"
        ).await?;

        db.execute_unprepared(
            "CREATE TABLE network_probe_record_hourly (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                target_id TEXT NOT NULL REFERENCES network_probe_target(id) ON DELETE CASCADE,
                avg_latency REAL,
                min_latency REAL,
                max_latency REAL,
                avg_packet_loss REAL NOT NULL,
                sample_count INTEGER NOT NULL,
                hour TEXT NOT NULL,
                UNIQUE(server_id, target_id, hour)
            )"
        ).await?;

        // Seed builtin probe targets
        let now = chrono::Utc::now().to_rfc3339();
        let targets = [
            ("cn-telecom-shanghai",  "Shanghai Telecom",  "Telecom",    "Shanghai",  "61.129.2.3"),
            ("cn-telecom-beijing",   "Beijing Telecom",   "Telecom",    "Beijing",   "106.37.67.29"),
            ("cn-telecom-guangzhou", "Guangzhou Telecom", "Telecom",    "Guangzhou", "14.215.116.1"),
            ("cn-unicom-shanghai",   "Shanghai Unicom",   "Unicom",     "Shanghai",  "210.22.84.3"),
            ("cn-unicom-beijing",    "Beijing Unicom",    "Unicom",     "Beijing",   "202.106.50.1"),
            ("cn-unicom-guangzhou",  "Guangzhou Unicom",  "Unicom",     "Guangzhou", "221.5.88.88"),
            ("cn-mobile-shanghai",   "Shanghai Mobile",   "Mobile",     "Shanghai",  "117.131.19.23"),
            ("cn-mobile-beijing",    "Beijing Mobile",    "Mobile",     "Beijing",   "221.179.155.161"),
            ("cn-mobile-guangzhou",  "Guangzhou Mobile",  "Mobile",     "Guangzhou", "120.196.165.24"),
            ("intl-cloudflare",      "Cloudflare",        "Cloudflare", "US",        "1.1.1.1"),
            ("intl-google",          "Google DNS",        "Google",     "US",        "8.8.8.8"),
            ("intl-aws-tokyo",       "AWS Tokyo",         "AWS",        "Tokyo",     "13.112.63.251"),
        ];

        for (id, name, provider, location, target) in &targets {
            db.execute_unprepared(&format!(
                "INSERT INTO network_probe_target (id, name, provider, location, target, probe_type, is_builtin, created_at, updated_at) VALUES ('{id}', '{name}', '{provider}', '{location}', '{target}', 'icmp', 1, '{now}', '{now}')"
            )).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_record_hourly").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_record").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_config").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS network_probe_target").await?;
        Ok(())
    }
}
