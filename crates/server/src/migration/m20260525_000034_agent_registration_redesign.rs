use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260525_000034_agent_registration_redesign"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 1. Drop the fingerprint unique index. Fingerprint is informational
        //    only after this redesign; multiple servers may legally share one.
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_fingerprint")
            .await?;

        // 2. Make the servers token columns nullable (SQLite table-rebuild).
        db.execute_unprepared(
            r#"
            CREATE TABLE servers_new (
                id TEXT PRIMARY KEY NOT NULL,
                token_hash TEXT,
                token_prefix TEXT,
                name TEXT NOT NULL,
                cpu_name TEXT,
                cpu_cores INTEGER,
                cpu_arch TEXT,
                os TEXT,
                kernel_version TEXT,
                mem_total INTEGER,
                swap_total INTEGER,
                disk_total INTEGER,
                ipv4 TEXT,
                ipv6 TEXT,
                region TEXT,
                country_code TEXT,
                virtualization TEXT,
                agent_version TEXT,
                group_id TEXT,
                weight INTEGER NOT NULL DEFAULT 0,
                hidden INTEGER NOT NULL DEFAULT 0,
                remark TEXT,
                public_remark TEXT,
                price REAL,
                billing_cycle TEXT,
                currency TEXT,
                expired_at TIMESTAMP,
                traffic_limit INTEGER,
                traffic_limit_type TEXT,
                billing_start_day INTEGER,
                capabilities INTEGER NOT NULL,
                protocol_version INTEGER NOT NULL,
                features TEXT NOT NULL DEFAULT '[]',
                last_remote_addr TEXT,
                fingerprint TEXT,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL
            );
            INSERT INTO servers_new (
                id, token_hash, token_prefix, name,
                cpu_name, cpu_cores, cpu_arch, os, kernel_version,
                mem_total, swap_total, disk_total,
                ipv4, ipv6, region, country_code, virtualization, agent_version,
                group_id, weight, hidden, remark, public_remark,
                price, billing_cycle, currency, expired_at,
                traffic_limit, traffic_limit_type,
                created_at, updated_at,
                capabilities, protocol_version, billing_start_day,
                features, last_remote_addr, fingerprint
            )
            SELECT
                id, token_hash, token_prefix, name,
                cpu_name, cpu_cores, cpu_arch, os, kernel_version,
                mem_total, swap_total, disk_total,
                ipv4, ipv6, region, country_code, virtualization, agent_version,
                group_id, weight, hidden, remark, public_remark,
                price, billing_cycle, currency, expired_at,
                traffic_limit, traffic_limit_type,
                created_at, updated_at,
                capabilities, protocol_version, billing_start_day,
                features, last_remote_addr, fingerprint
            FROM servers;
            DROP TABLE servers;
            ALTER TABLE servers_new RENAME TO servers;
            CREATE INDEX idx_servers_group_id ON servers(group_id);
            "#,
        )
        .await?;

        // 3. Wipe legacy enrollment + recovery_job tables; rebuild enrollments.
        db.execute_unprepared("DROP TABLE IF EXISTS agent_enrollments").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS recovery_job").await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE agent_enrollments (
                id TEXT PRIMARY KEY NOT NULL,
                code_hash TEXT NOT NULL,
                code_prefix TEXT NOT NULL,
                target_server_id TEXT NOT NULL
                    REFERENCES servers(id) ON DELETE CASCADE,
                created_by TEXT NOT NULL REFERENCES users(id),
                expires_at TIMESTAMP NOT NULL,
                consumed_at TIMESTAMP,
                revoked_at TIMESTAMP,
                created_at TIMESTAMP NOT NULL
            );
            CREATE UNIQUE INDEX idx_enrollments_active_per_server
                ON agent_enrollments(target_server_id)
                WHERE consumed_at IS NULL AND revoked_at IS NULL;
            CREATE INDEX idx_enrollments_code_prefix
                ON agent_enrollments(code_prefix);
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
