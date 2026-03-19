use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use sea_orm::*;

use crate::entity::{audit_log, ping_record};
use crate::service::network_probe::NetworkProbeService;
use crate::service::record::RecordService;
use crate::service::service_monitor::ServiceMonitorService;
use crate::service::traffic::TrafficService;
use crate::state::AppState;

/// Periodically cleans up expired records based on retention config.
/// Runs every 3600 seconds, offset by 60s from the aggregator.
pub async fn run(state: Arc<AppState>) {
    // Offset by 60 seconds so cleanup doesn't run at the same time as aggregation
    tokio::time::sleep(Duration::from_secs(60)).await;

    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;

        let retention = &state.config.retention;

        // Clean up raw records
        match RecordService::cleanup_expired(&state.db, retention.records_days, "records").await {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired raw records"),
            Err(e) => tracing::error!("Failed to clean up raw records: {e}"),
            _ => {}
        }

        // Clean up hourly records
        match RecordService::cleanup_expired(
            &state.db,
            retention.records_hourly_days,
            "records_hourly",
        )
        .await
        {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired hourly records"),
            Err(e) => tracing::error!("Failed to clean up hourly records: {e}"),
            _ => {}
        }

        // Clean up GPU records
        match RecordService::cleanup_expired(&state.db, retention.gpu_records_days, "gpu_records")
            .await
        {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired GPU records"),
            Err(e) => tracing::error!("Failed to clean up GPU records: {e}"),
            _ => {}
        }

        // Clean up ping records
        cleanup_ping_records(&state.db, retention.ping_records_days).await;

        // Clean up audit logs
        cleanup_audit_logs(&state.db, retention.audit_logs_days).await;

        // Clean up network probe records
        match NetworkProbeService::cleanup_old_records(&state.db, retention).await {
            Ok((raw, hourly)) => {
                if raw > 0 {
                    tracing::info!("Cleaned up {raw} expired network probe raw records");
                }
                if hourly > 0 {
                    tracing::info!("Cleaned up {hourly} expired network probe hourly records");
                }
            }
            Err(e) => tracing::error!("Failed to clean up network probe records: {e}"),
        }

        // Clean up traffic hourly
        match TrafficService::cleanup_hourly(&state.db, retention.traffic_hourly_days).await {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired traffic hourly records"),
            Err(e) => tracing::error!("Failed to clean up traffic hourly records: {e}"),
            _ => {}
        }

        // Clean up traffic daily
        match TrafficService::cleanup_daily(&state.db, retention.traffic_daily_days).await {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired traffic daily records"),
            Err(e) => tracing::error!("Failed to clean up traffic daily records: {e}"),
            _ => {}
        }

        // Clean up task results
        match TrafficService::cleanup_task_results(&state.db, retention.task_results_days).await {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired task results"),
            Err(e) => tracing::error!("Failed to clean up task results: {e}"),
            _ => {}
        }

        // Clean up docker events
        match crate::service::docker::DockerService::cleanup_expired(
            &state.db,
            retention.docker_events_days,
        )
        .await
        {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired docker events"),
            Err(e) => tracing::error!("Failed to clean up docker events: {e}"),
            _ => {}
        }

        // Clean up service monitor records
        match ServiceMonitorService::cleanup_records(&state.db, retention.service_monitor_days)
            .await
        {
            Ok(n) if n > 0 => {
                tracing::info!("Cleaned up {n} expired service monitor records")
            }
            Err(e) => tracing::error!("Failed to clean up service monitor records: {e}"),
            _ => {}
        }

        // Clean up expired file transfers (idle for > 30 minutes)
        state
            .file_transfers
            .cleanup_expired(Duration::from_secs(
                serverbee_common::constants::FILE_TRANSFER_TIMEOUT_SECS,
            ));
    }
}

async fn cleanup_ping_records(db: &DatabaseConnection, retention_days: u32) {
    let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);
    match ping_record::Entity::delete_many()
        .filter(ping_record::Column::Time.lt(cutoff))
        .exec(db)
        .await
    {
        Ok(result) if result.rows_affected > 0 => {
            tracing::info!(
                "Cleaned up {} expired ping records",
                result.rows_affected
            );
        }
        Err(e) => tracing::error!("Failed to clean up ping records: {e}"),
        _ => {}
    }
}

async fn cleanup_audit_logs(db: &DatabaseConnection, retention_days: u32) {
    let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);
    match audit_log::Entity::delete_many()
        .filter(audit_log::Column::CreatedAt.lt(cutoff))
        .exec(db)
        .await
    {
        Ok(result) if result.rows_affected > 0 => {
            tracing::info!(
                "Cleaned up {} expired audit logs",
                result.rows_affected
            );
        }
        Err(e) => tracing::error!("Failed to clean up audit logs: {e}"),
        _ => {}
    }
}
