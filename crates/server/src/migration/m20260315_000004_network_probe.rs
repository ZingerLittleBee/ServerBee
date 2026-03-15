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
        // China ISP targets: 31 provinces × 3 ISPs using Zstatic CDN TCP Ping nodes
        // (CDN backbone, auto-updated DNS every 30min, stable and reliable)
        // Format: {province_code}-{isp_code}-v4.ip.zstaticcdn.com:80 for TCP probe type
        // International targets use well-known IPs that reliably respond to ICMP
        let provinces: &[(&str, &str)] = &[
            ("bj", "Beijing"), ("tj", "Tianjin"), ("he", "Hebei"), ("sx", "Shanxi"), ("nm", "InnerMongolia"),
            ("ln", "Liaoning"), ("jl", "Jilin"), ("hl", "Heilongjiang"),
            ("sh", "Shanghai"), ("js", "Jiangsu"), ("zj", "Zhejiang"), ("ah", "Anhui"), ("fj", "Fujian"),
            ("jx", "Jiangxi"), ("sd", "Shandong"),
            ("ha", "Henan"), ("hb", "Hubei"), ("hn", "Hunan"), ("gd", "Guangdong"), ("gx", "Guangxi"), ("hi", "Hainan"),
            ("cq", "Chongqing"), ("sc", "Sichuan"), ("gz", "Guizhou"), ("yn", "Yunnan"), ("xz", "Tibet"),
            ("sn", "Shaanxi"), ("gs", "Gansu"), ("qh", "Qinghai"), ("nx", "Ningxia"), ("xj", "Xinjiang"),
        ];

        let isps: &[(&str, &str)] = &[
            ("ct", "Telecom"),
            ("cu", "Unicom"),
            ("cm", "Mobile"),
        ];

        for (code, location) in provinces {
            for (isp_code, isp_name) in isps {
                let id = format!("cn-{code}-{isp_code}");
                let name = format!("{location} {isp_name}");
                let target = format!("{code}-{isp_code}-v4.ip.zstaticcdn.com:80");
                db.execute_unprepared(&format!(
                    "INSERT INTO network_probe_target (id, name, provider, location, target, probe_type, is_builtin, created_at, updated_at) \
                     VALUES ('{id}', '{name}', '{isp_name}', '{location}', '{target}', 'tcp', 1, '{now}', '{now}')"
                )).await?;
            }
        }

        // International targets (ICMP)
        let intl_targets = [
            ("intl-cloudflare", "Cloudflare", "Cloudflare", "US", "1.1.1.1", "icmp"),
            ("intl-google", "Google DNS", "Google", "US", "8.8.8.8", "icmp"),
            ("intl-aws-tokyo", "AWS Tokyo", "AWS", "Tokyo", "13.112.63.251", "icmp"),
        ];
        for (id, name, provider, location, target, probe_type) in &intl_targets {
            db.execute_unprepared(&format!(
                "INSERT INTO network_probe_target (id, name, provider, location, target, probe_type, is_builtin, created_at, updated_at) \
                 VALUES ('{id}', '{name}', '{provider}', '{location}', '{target}', '{probe_type}', 1, '{now}', '{now}')"
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
