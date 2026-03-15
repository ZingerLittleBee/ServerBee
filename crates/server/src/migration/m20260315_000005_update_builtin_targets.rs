use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260315_000005_update_builtin_targets"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let now = chrono::Utc::now().to_rfc3339();

        // Update China ISP targets from unreliable DNS IPs to Zstatic CDN TCP Ping nodes
        let updates = [
            ("cn-telecom-beijing",   "bj-ct-v4.ip.zstaticcdn.com:80", "tcp"),
            ("cn-telecom-shanghai",  "sh-ct-v4.ip.zstaticcdn.com:80", "tcp"),
            ("cn-telecom-guangzhou", "gd-ct-v4.ip.zstaticcdn.com:80", "tcp"),
            ("cn-unicom-beijing",    "bj-cu-v4.ip.zstaticcdn.com:80", "tcp"),
            ("cn-unicom-shanghai",   "sh-cu-v4.ip.zstaticcdn.com:80", "tcp"),
            ("cn-unicom-guangzhou",  "gd-cu-v4.ip.zstaticcdn.com:80", "tcp"),
            ("cn-mobile-beijing",    "bj-cm-v4.ip.zstaticcdn.com:80", "tcp"),
            ("cn-mobile-shanghai",   "sh-cm-v4.ip.zstaticcdn.com:80", "tcp"),
            ("cn-mobile-guangzhou",  "gd-cm-v4.ip.zstaticcdn.com:80", "tcp"),
        ];

        for (id, target, probe_type) in &updates {
            db.execute_unprepared(&format!(
                "UPDATE network_probe_target SET target = '{target}', probe_type = '{probe_type}', updated_at = '{now}' WHERE id = '{id}' AND is_builtin = 1"
            )).await?;
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Not reversible — old IPs were unreliable anyway
        Ok(())
    }
}
