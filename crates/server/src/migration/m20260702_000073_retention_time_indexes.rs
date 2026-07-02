use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260702_000073_retention_time_indexes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Retention cleanup and hourly aggregation filter these time-series
        // tables purely by their time column (e.g. `WHERE time < cutoff`,
        // `WHERE created_at < cutoff`). The existing composite indexes lead with
        // `server_id`/`task_id`, so a time-only predicate cannot use them and
        // SQLite falls back to a full table scan — increasingly expensive (and a
        // recurring write-lock stall, since cleanup runs in a write transaction)
        // as the largest tables grow into the millions of rows. Add a single
        // dedicated index on each time column these jobs scan.
        let db = manager.get_connection();
        for (idx, table, col) in [
            ("idx_records_time", "records", "time"),
            ("idx_records_hourly_time", "records_hourly", "time"),
            ("idx_gpu_records_time", "gpu_records", "time"),
            ("idx_ping_records_time", "ping_records", "time"),
            ("idx_security_event_created_at", "security_event", "created_at"),
            ("idx_audit_logs_created_at", "audit_logs", "created_at"),
            ("idx_unlock_event_changed_at", "unlock_event", "changed_at"),
            ("idx_ip_risk_cache_checked_at", "ip_risk_cache", "checked_at"),
            ("idx_traffic_hourly_hour", "traffic_hourly", "hour"),
        ] {
            db.execute_unprepared(&format!(
                "CREATE INDEX IF NOT EXISTS {idx} ON {table} ({col})"
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
