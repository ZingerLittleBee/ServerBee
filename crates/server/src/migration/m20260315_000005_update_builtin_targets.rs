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

        // Remove all existing builtin targets (cascades to configs and records via FK)
        db.execute_unprepared("DELETE FROM network_probe_config WHERE target_id IN (SELECT id FROM network_probe_target WHERE is_builtin = 1)").await?;
        db.execute_unprepared("DELETE FROM network_probe_record WHERE target_id IN (SELECT id FROM network_probe_target WHERE is_builtin = 1)").await?;
        db.execute_unprepared("DELETE FROM network_probe_record_hourly WHERE target_id IN (SELECT id FROM network_probe_target WHERE is_builtin = 1)").await?;
        db.execute_unprepared("DELETE FROM network_probe_target WHERE is_builtin = 1").await?;

        // Re-insert all 96 builtin targets (31 provinces × 3 ISPs + 3 international)
        let now = chrono::Utc::now().to_rfc3339();

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

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Not reversible — old IPs were unreliable anyway
        Ok(())
    }
}
