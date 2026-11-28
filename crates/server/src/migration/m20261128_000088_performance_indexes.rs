use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261128_000088_performance_indexes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for (idx, table, cols) in [
            ("idx_api_keys_key_prefix", "api_keys", "key_prefix"),
            ("idx_servers_token_prefix", "servers", "token_prefix"),
            ("idx_sessions_expires_at", "sessions", "expires_at"),
            (
                "idx_sessions_mobile_session_id",
                "sessions",
                "mobile_session_id",
            ),
            (
                "idx_service_monitor_record_time",
                "service_monitor_record",
                "time",
            ),
            (
                "idx_network_probe_record_timestamp",
                "network_probe_record",
                "timestamp",
            ),
            (
                "idx_network_probe_record_server_timestamp",
                "network_probe_record",
                "server_id, timestamp",
            ),
            (
                "idx_network_probe_record_hourly_hour",
                "network_probe_record_hourly",
                "hour",
            ),
            (
                "idx_task_results_finished_at",
                "task_results",
                "finished_at",
            ),
            (
                "idx_task_results_task_run",
                "task_results",
                "task_id, run_id",
            ),
            (
                "idx_security_event_push_intent",
                "security_event",
                "push_intent",
            ),
            (
                "idx_mobile_push_outbox_expires_at",
                "mobile_push_outbox",
                "expires_at",
            ),
            (
                "idx_traceroute_record_started_at",
                "traceroute_record",
                "started_at",
            ),
            (
                "idx_agent_authority_events_created_at",
                "agent_authority_events",
                "created_at",
            ),
            (
                "idx_enrollment_offers_expires_at",
                "enrollment_offers",
                "expires_at",
            ),
        ] {
            db.execute_unprepared(&format!(
                "CREATE INDEX IF NOT EXISTS {idx} ON {table} ({cols})"
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
