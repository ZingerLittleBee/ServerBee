use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;

use crate::service::network_probe::NetworkProbeService;
use crate::service::record::RecordService;
use crate::service::traffic::TrafficService;
use crate::service::uptime::UptimeService;
use crate::state::AppState;

/// Periodically aggregates raw records into hourly summaries (every 3600 seconds).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;

        aggregate_once(&state.db, &state.config.scheduler.timezone).await;
    }
}

/// Performs the work of a single aggregator tick: roll up raw records into
/// hourly summaries, hourly network probe summaries, daily traffic, and daily
/// uptime. Each step is independent — a failure in one is logged and does not
/// prevent the others from running. Extracted from the `run` loop so it can be
/// exercised directly in tests without driving the infinite interval loop.
pub(crate) async fn aggregate_once(db: &DatabaseConnection, timezone: &str) {
    match RecordService::aggregate_hourly(db).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!("Aggregated hourly records for {count} servers");
            }
        }
        Err(e) => {
            tracing::error!("Failed to aggregate hourly records: {e}");
        }
    }

    match NetworkProbeService::aggregate_hourly(db).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!("Aggregated {count} hourly network probe records");
            }
        }
        Err(e) => {
            tracing::error!("Failed to aggregate hourly network probe records: {e}");
        }
    }

    match TrafficService::aggregate_daily(db, timezone).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!("Aggregated daily traffic for {count} server-date pairs");
            }
        }
        Err(e) => {
            tracing::error!("Failed to aggregate daily traffic: {e}");
        }
    }

    match UptimeService::aggregate_daily(db).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!("Aggregated daily uptime for {count} servers");
            }
        }
        Err(e) => {
            tracing::error!("Failed to aggregate daily uptime: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{record, record_hourly, server};
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use chrono::{DurationRound, Utc};
    use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, NotSet, Set};
    use serverbee_common::constants::CAP_DEFAULT;

    async fn insert_test_server(db: &DatabaseConnection, id: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash_password should succeed");
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some(token_hash)),
            token_prefix: Set(Some("serverbee_test".to_string())),
            name: Set(id.to_string()),
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
        .expect("insert test server should succeed");
    }

    /// Insert a raw record at the given time with a fixed cpu value so the
    /// hourly average is deterministic.
    async fn insert_record(db: &DatabaseConnection, server_id: &str, time: chrono::DateTime<Utc>, cpu: f64) {
        record::ActiveModel {
            id: NotSet,
            server_id: Set(server_id.to_string()),
            time: Set(time),
            cpu: Set(cpu),
            mem_used: Set(0),
            swap_used: Set(0),
            disk_used: Set(0),
            net_in_speed: Set(0),
            net_out_speed: Set(0),
            net_in_transfer: Set(0),
            net_out_transfer: Set(0),
            load1: Set(0.0),
            load5: Set(0.0),
            load15: Set(0.0),
            tcp_conn: Set(0),
            udp_conn: Set(0),
            process_count: Set(0),
            temperature: Set(None),
            gpu_usage: Set(None),
            disk_io_json: Set(None),
        }
        .insert(db)
        .await
        .expect("insert record should succeed");
    }

    /// Empty input: no raw records anywhere -> no hourly rows produced, and the
    /// tick completes without panicking (all four steps no-op).
    #[tokio::test]
    async fn aggregate_once_empty_produces_no_hourly_rows() {
        let (db, _tmp) = setup_test_db().await;

        aggregate_once(&db, "UTC").await;

        let hourly = record_hourly::Entity::find()
            .all(&db)
            .await
            .expect("query records_hourly should succeed");
        assert!(hourly.is_empty(), "no records should yield no hourly rows");
    }

    /// Happy path: raw records seeded into the previous completed hour bucket are
    /// rolled up into a single hourly row per server with the correct average.
    /// aggregate_hourly looks back exactly one hour from the truncated current
    /// hour, so we seed inside [prev_hour_start, prev_hour_end).
    #[tokio::test]
    async fn aggregate_once_rolls_up_previous_hour_records() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-agg").await;

        // Previous completed hour window that aggregate_hourly scans.
        let now = Utc::now();
        let hour = now
            .duration_trunc(chrono::Duration::hours(1))
            .expect("duration_trunc should succeed");
        let prev_hour_start = hour - chrono::Duration::hours(1);
        let t1 = prev_hour_start + chrono::Duration::minutes(10);
        let t2 = prev_hour_start + chrono::Duration::minutes(50);

        // Two records: cpu 20.0 and 40.0 -> expected hourly avg 30.0.
        insert_record(&db, "srv-agg", t1, 20.0).await;
        insert_record(&db, "srv-agg", t2, 40.0).await;

        aggregate_once(&db, "UTC").await;

        let hourly = record_hourly::Entity::find()
            .all(&db)
            .await
            .expect("query records_hourly should succeed");
        assert_eq!(hourly.len(), 1, "one server-hour bucket should be produced");
        assert_eq!(hourly[0].server_id, "srv-agg");
        assert_eq!(hourly[0].time, prev_hour_start);
        assert!(
            (hourly[0].cpu - 30.0).abs() < f64::EPSILON,
            "hourly cpu should be the average of the two raw records, got {}",
            hourly[0].cpu
        );
    }

    /// Boundary: records that fall OUTSIDE the previous completed hour window
    /// (here, in the hour before that) are not aggregated by this tick.
    #[tokio::test]
    async fn aggregate_once_ignores_records_outside_prev_hour() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-old").await;

        let now = Utc::now();
        let hour = now
            .duration_trunc(chrono::Duration::hours(1))
            .expect("duration_trunc should succeed");
        // Two hours back: before the [prev_hour_start, prev_hour_end) window.
        let too_old = hour - chrono::Duration::hours(2) + chrono::Duration::minutes(30);
        insert_record(&db, "srv-old", too_old, 99.0).await;

        aggregate_once(&db, "UTC").await;

        let hourly = record_hourly::Entity::find()
            .all(&db)
            .await
            .expect("query records_hourly should succeed");
        assert!(
            hourly.is_empty(),
            "records outside the previous-hour window must not be aggregated"
        );
    }

    /// An invalid timezone makes the traffic step return an error, which is
    /// logged and swallowed; the record aggregation step still runs and writes
    /// its hourly row. Verifies the tick is resilient to per-step failures.
    #[tokio::test]
    async fn aggregate_once_invalid_timezone_does_not_block_record_rollup() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-tz").await;

        let now = Utc::now();
        let hour = now
            .duration_trunc(chrono::Duration::hours(1))
            .expect("duration_trunc should succeed");
        let prev_hour_start = hour - chrono::Duration::hours(1);
        insert_record(&db, "srv-tz", prev_hour_start + chrono::Duration::minutes(5), 12.0).await;

        // "Not/AZone" is rejected by TrafficService::aggregate_daily.
        aggregate_once(&db, "Not/AZone").await;

        let hourly = record_hourly::Entity::find()
            .all(&db)
            .await
            .expect("query records_hourly should succeed");
        assert_eq!(
            hourly.len(),
            1,
            "record rollup must still run even when the traffic step fails"
        );
        assert_eq!(hourly[0].server_id, "srv-tz");
    }
}
