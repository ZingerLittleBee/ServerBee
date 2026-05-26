use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use sea_orm::*;

use crate::entity::{audit_log, ping_record, security_event};
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

        // Clean up security events
        cleanup_security_events(&state.db, retention.security_event_days).await;

        // Enrollment pruning: removed in T6 — enrollments are now bound 1:1
        // to a server (revoked_at, target_server_id). They expire / get
        // revoked / get consumed but stay in the table for audit. A later
        // task may add a windowed cleanup if the table grows.

        // Clean up expired file transfers (idle for > 30 minutes)
        state.file_transfers.cleanup_expired(Duration::from_secs(
            serverbee_common::constants::FILE_TRANSFER_TIMEOUT_SECS,
        ));

        // Clean up IP quality event history
        match cleanup_ip_quality_events(&state.db, retention.ip_quality_event_days).await {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired IP quality events"),
            Err(e) => tracing::error!("Failed to clean up IP quality events: {e}"),
            _ => {}
        }

        // Clean up stale IP risk cache entries (fixed 30-day window)
        match cleanup_ip_risk_cache(&state.db, 30).await {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {n} stale IP risk cache entries"),
            Err(e) => tracing::error!("Failed to clean up IP risk cache: {e}"),
            _ => {}
        }
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
            tracing::info!("Cleaned up {} expired ping records", result.rows_affected);
        }
        Err(e) => tracing::error!("Failed to clean up ping records: {e}"),
        _ => {}
    }
}

async fn cleanup_security_events(db: &DatabaseConnection, retention_days: u32) {
    let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);
    match security_event::Entity::delete_many()
        .filter(security_event::Column::CreatedAt.lt(cutoff))
        .exec(db)
        .await
    {
        Ok(result) if result.rows_affected > 0 => {
            tracing::info!(
                "Cleaned up {} expired security events",
                result.rows_affected
            );
        }
        Err(e) => tracing::error!("Failed to clean up security events: {e}"),
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
            tracing::info!("Cleaned up {} expired audit logs", result.rows_affected);
        }
        Err(e) => tracing::error!("Failed to clean up audit logs: {e}"),
        _ => {}
    }
}

pub async fn cleanup_ip_quality_events(db: &DatabaseConnection, retention_days: u32) -> Result<u64, sea_orm::DbErr> {
    let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);
    let result = crate::entity::unlock_event::Entity::delete_many()
        .filter(crate::entity::unlock_event::Column::ChangedAt.lt(cutoff))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

pub async fn cleanup_ip_risk_cache(db: &DatabaseConnection, retention_days: u32) -> Result<u64, sea_orm::DbErr> {
    let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);
    let result = crate::entity::ip_risk_cache::Entity::delete_many()
        .filter(crate::entity::ip_risk_cache::Column::CheckedAt.lt(cutoff))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use uuid::Uuid;

    use crate::entity::{ip_risk_cache, unlock_event};
    use crate::test_utils::setup_test_db;

    use super::*;

    async fn ensure_server(db: &sea_orm::DatabaseConnection) {
        use crate::entity::server;
        use crate::service::auth::AuthService;
        use serverbee_common::constants::CAP_DEFAULT;

        // Avoid duplicate insertion
        if server::Entity::find_by_id("srv-cleanup")
            .one(db)
            .await
            .expect("server lookup")
            .is_some()
        {
            return;
        }
        let hash = AuthService::hash_password("tok").expect("hash");
        let now = Utc::now();
        server::ActiveModel {
            id: Set("srv-cleanup".to_string()),
            token_hash: Set(Some(hash)),
            token_prefix: Set(Some("serverbee_test".to_string())),
            name: Set("cleanup-test-server".to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server");
    }

    /// Insert an `unlock_event` with the given `changed_at` offset (negative = in the past).
    async fn insert_event(
        db: &sea_orm::DatabaseConnection,
        days_ago: i64,
    ) -> String {
        use crate::entity::unlock_service;

        ensure_server(db).await;

        // Pick the first available service id (seeded by migration)
        let svc = unlock_service::Entity::find()
            .one(db)
            .await
            .expect("find service")
            .expect("at least one service should be seeded");

        let id = Uuid::new_v4().to_string();
        let changed_at = Utc::now() - ChronoDuration::days(days_ago);
        unlock_event::ActiveModel {
            id: Set(id.clone()),
            server_id: Set("srv-cleanup".to_string()),
            service_id: Set(svc.id),
            old_status: Set("unlocked".to_string()),
            new_status: Set("blocked".to_string()),
            changed_at: Set(changed_at),
        }
        .insert(db)
        .await
        .expect("insert unlock_event");
        id
    }

    /// Insert an `ip_risk_cache` row with the given `checked_at` offset.
    async fn insert_cache(
        db: &sea_orm::DatabaseConnection,
        ip: &str,
        days_ago: i64,
    ) {
        let checked_at = Utc::now() - ChronoDuration::days(days_ago);
        ip_risk_cache::ActiveModel {
            ip: Set(ip.to_string()),
            asn: Set(None),
            as_org: Set(None),
            country: Set(None),
            region: Set(None),
            city: Set(None),
            ip_type: Set("unknown".to_string()),
            is_proxy: Set(false),
            is_vpn: Set(false),
            is_hosting: Set(false),
            risk_score: Set(None),
            risk_level: Set("unknown".to_string()),
            is_tor: Set(false),
            is_abuser: Set(false),
            is_mobile: Set(false),
            asn_abuser_score: Set(None),
            abuse_email: Set(None),
            providers: Set("{}".to_string()),
            checked_at: Set(checked_at),
        }
        .insert(db)
        .await
        .expect("insert ip_risk_cache");
    }

    #[tokio::test]
    async fn cleanup_ip_quality_events_removes_old_rows_and_keeps_recent() {
        let (db, _tmp) = setup_test_db().await;

        // Insert old event (100 days ago) and a recent one (1 day ago)
        insert_event(&db, 100).await;
        let recent_id = insert_event(&db, 1).await;

        // Cleanup with 90-day retention
        let removed = cleanup_ip_quality_events(&db, 90).await.unwrap();
        assert_eq!(removed, 1, "should delete the old event");

        let remaining = unlock_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(remaining.len(), 1, "only the recent event should remain");
        assert_eq!(remaining[0].id, recent_id);
    }

    #[tokio::test]
    async fn cleanup_ip_risk_cache_removes_old_rows_and_keeps_recent() {
        let (db, _tmp) = setup_test_db().await;

        // Insert old cache entry (40 days ago) and a recent one (10 days ago)
        insert_cache(&db, "10.0.0.1", 40).await;
        insert_cache(&db, "10.0.0.2", 10).await;

        // Cleanup with 30-day retention
        let removed = cleanup_ip_risk_cache(&db, 30).await.unwrap();
        assert_eq!(removed, 1, "should delete the old cache entry");

        let remaining = ip_risk_cache::Entity::find().all(&db).await.unwrap();
        assert_eq!(remaining.len(), 1, "only the recent cache entry should remain");
        assert_eq!(remaining[0].ip, "10.0.0.2");
    }
}
