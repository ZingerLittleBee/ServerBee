use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Timelike, Utc};
use sea_orm::DatabaseConnection;
use serverbee_common::types::SystemReport;

use crate::service::record::RecordService;
use crate::service::traffic::{TrafficService, compute_delta};
use crate::state::AppState;

/// Periodically writes cached agent reports to the database (every 60 seconds).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));

    // Initialize transfer cache from traffic_state table
    let mut transfer_cache: HashMap<String, (i64, i64)> =
        match TrafficService::load_transfer_cache(&state.db).await {
            Ok(cache) => {
                tracing::info!("Loaded {} transfer cache entries", cache.len());
                cache
            }
            Err(e) => {
                tracing::error!("Failed to load transfer cache: {e}");
                HashMap::new()
            }
        };

    loop {
        interval.tick().await;

        let reports = state.agent_manager.all_latest_reports();
        let now = Utc::now();
        flush_reports(&state.db, &reports, &mut transfer_cache, now).await;
    }
}

/// Persist one batch of cached agent reports and update traffic accounting.
///
/// This is the per-tick body of [`run`], extracted so it can be unit-tested.
/// It writes each report as a metrics record, then computes per-server traffic
/// deltas against `transfer_cache` (carried across ticks) and upserts the
/// hourly/state traffic rows. `now` is the wall-clock instant for the tick;
/// it is truncated to the hour boundary for hourly bucketing. Returns the
/// number of metric records successfully written.
pub(crate) async fn flush_reports(
    db: &DatabaseConnection,
    reports: &[(String, Arc<SystemReport>)],
    transfer_cache: &mut HashMap<String, (i64, i64)>,
    now: DateTime<Utc>,
) -> usize {
    if reports.is_empty() {
        return 0;
    }

    // Truncate to hour boundary for hourly bucketing
    let hour = now
        .date_naive()
        .and_hms_opt(now.time().hour(), 0, 0)
        .unwrap()
        .and_utc();

    let mut count = 0;
    for (server_id, report) in reports {
        // Save metrics record
        if let Err(e) = RecordService::save_report(db, server_id, report).await {
            tracing::error!("Failed to save record for {server_id}: {e}");
        } else {
            count += 1;
        }

        // Compute traffic delta
        let curr_in = report.net_in_transfer;
        let curr_out = report.net_out_transfer;

        if curr_in == 0 && curr_out == 0 {
            // No traffic data available, skip
            continue;
        }

        let (delta_in, delta_out) = if let Some(&(prev_in, prev_out)) =
            transfer_cache.get(server_id)
        {
            compute_delta(prev_in, prev_out, curr_in, curr_out)
        } else {
            // First observation: no previous state, skip delta (just record state)
            transfer_cache.insert(server_id.clone(), (curr_in, curr_out));
            if let Err(e) = TrafficService::upsert_state(db, server_id, curr_in, curr_out).await {
                tracing::error!("Failed to upsert traffic state for {server_id}: {e}");
            }
            continue;
        };

        // Update cache
        transfer_cache.insert(server_id.clone(), (curr_in, curr_out));

        // Only write if there's actual traffic
        if (delta_in > 0 || delta_out > 0)
            && let Err(e) =
                TrafficService::upsert_hourly(db, server_id, hour, delta_in, delta_out).await
        {
            tracing::error!("Failed to upsert traffic hourly for {server_id}: {e}");
        }

        // Always update state
        if let Err(e) = TrafficService::upsert_state(db, server_id, curr_in, curr_out).await {
            tracing::error!("Failed to upsert traffic state for {server_id}: {e}");
        }
    }

    tracing::debug!("Wrote {count} metric records");
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{record, server, traffic_hourly, traffic_state};
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use chrono::TimeZone;
    use sea_orm::{
        ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    };
    use serverbee_common::constants::CAP_DEFAULT;
    use serverbee_common::types::SystemReport;

    /// Fixed instant used for all deterministic assertions (2026-03-17 10:30:00 UTC).
    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 17, 10, 30, 0).unwrap()
    }

    /// Insert a parent `server` row so FK-constrained record/traffic writes succeed.
    async fn insert_test_server(db: &DatabaseConnection, id: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash_password should succeed");
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some(token_hash)),
            token_prefix: Set(Some("serverbee_test".to_string())),
            name: Set("Test Server".to_string()),
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

    fn report_with_transfer(net_in: i64, net_out: i64) -> Arc<SystemReport> {
        Arc::new(SystemReport {
            cpu: 12.5,
            mem_used: 2048,
            net_in_transfer: net_in,
            net_out_transfer: net_out,
            ..Default::default()
        })
    }

    /// Empty batch is a no-op: returns 0 and writes nothing.
    #[tokio::test]
    async fn flush_reports_empty_batch_is_noop() {
        let (db, _tmp) = setup_test_db().await;
        let mut cache: HashMap<String, (i64, i64)> = HashMap::new();

        let written = flush_reports(&db, &[], &mut cache, fixed_now()).await;
        assert_eq!(written, 0, "empty batch should write nothing");

        let records = record::Entity::find()
            .all(&db)
            .await
            .expect("query records should succeed");
        assert!(records.is_empty(), "no records should be persisted");
        assert!(cache.is_empty(), "cache should be untouched for empty batch");
    }

    /// Happy path: a batch of reports is persisted as records, one per server.
    #[tokio::test]
    async fn flush_reports_persists_records_for_batch() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-a").await;
        insert_test_server(&db, "srv-b").await;

        // Zero transfer so the traffic branch short-circuits; we only assert records here.
        let reports = vec![
            ("srv-a".to_string(), report_with_transfer(0, 0)),
            ("srv-b".to_string(), report_with_transfer(0, 0)),
        ];
        let mut cache: HashMap<String, (i64, i64)> = HashMap::new();

        let written = flush_reports(&db, &reports, &mut cache, fixed_now()).await;
        assert_eq!(written, 2, "both reports should be written as records");

        let rec_a = record::Entity::find()
            .filter(record::Column::ServerId.eq("srv-a"))
            .all(&db)
            .await
            .expect("query srv-a records should succeed");
        assert_eq!(rec_a.len(), 1, "srv-a should have exactly one record");
        assert!(
            (rec_a[0].cpu - 12.5).abs() < f64::EPSILON,
            "persisted cpu should match the report"
        );
        assert_eq!(rec_a[0].mem_used, 2048, "persisted mem_used should match");

        let rec_b = record::Entity::find()
            .filter(record::Column::ServerId.eq("srv-b"))
            .all(&db)
            .await
            .expect("query srv-b records should succeed");
        assert_eq!(rec_b.len(), 1, "srv-b should have exactly one record");

        // Zero-transfer reports never touch the traffic tables or the cache.
        assert!(cache.is_empty(), "zero-transfer report must not seed cache");
        let states = traffic_state::Entity::find()
            .all(&db)
            .await
            .expect("query traffic_state should succeed");
        assert!(
            states.is_empty(),
            "no traffic_state rows for zero-transfer reports"
        );
    }

    /// First observation with non-zero transfer: state is seeded, cache primed,
    /// but no hourly delta is written (no previous baseline).
    #[tokio::test]
    async fn flush_reports_first_observation_seeds_state_only() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-c").await;

        let reports = vec![("srv-c".to_string(), report_with_transfer(1_000, 2_000))];
        let mut cache: HashMap<String, (i64, i64)> = HashMap::new();

        let written = flush_reports(&db, &reports, &mut cache, fixed_now()).await;
        assert_eq!(written, 1, "the single report should be recorded");

        // Cache is primed with the current counters.
        assert_eq!(
            cache.get("srv-c"),
            Some(&(1_000i64, 2_000i64)),
            "cache should hold the first observation"
        );

        // traffic_state seeded with the raw counters.
        let state = traffic_state::Entity::find()
            .filter(traffic_state::Column::ServerId.eq("srv-c"))
            .one(&db)
            .await
            .expect("query traffic_state should succeed")
            .expect("traffic_state row should exist");
        assert_eq!(state.last_in, 1_000);
        assert_eq!(state.last_out, 2_000);

        // No hourly delta on first observation (no prior baseline).
        let hourly = traffic_hourly::Entity::find()
            .filter(traffic_hourly::Column::ServerId.eq("srv-c"))
            .all(&db)
            .await
            .expect("query traffic_hourly should succeed");
        assert!(
            hourly.is_empty(),
            "first observation must not write an hourly delta"
        );
    }

    /// Two consecutive ticks share the carried cache: the second tick computes a
    /// positive delta and writes it into the hourly bucket for the tick's hour.
    #[tokio::test]
    async fn flush_reports_second_tick_writes_hourly_delta() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-d").await;

        let mut cache: HashMap<String, (i64, i64)> = HashMap::new();
        let now = fixed_now();

        // Tick 1: baseline observation (1000, 2000) -> seeds state, no delta.
        let tick1 = vec![("srv-d".to_string(), report_with_transfer(1_000, 2_000))];
        flush_reports(&db, &tick1, &mut cache, now).await;

        // Tick 2: counters advanced to (1500, 2300) -> delta (500, 300).
        let tick2 = vec![("srv-d".to_string(), report_with_transfer(1_500, 2_300))];
        let written = flush_reports(&db, &tick2, &mut cache, now).await;
        assert_eq!(written, 1, "second tick should record one more record");

        // Cache now reflects the latest counters.
        assert_eq!(cache.get("srv-d"), Some(&(1_500i64, 2_300i64)));

        // Hourly bucket for the tick's hour holds the accumulated delta.
        let expected_hour = now
            .date_naive()
            .and_hms_opt(now.time().hour(), 0, 0)
            .unwrap()
            .and_utc();
        let hourly = traffic_hourly::Entity::find()
            .filter(traffic_hourly::Column::ServerId.eq("srv-d"))
            .all(&db)
            .await
            .expect("query traffic_hourly should succeed");
        assert_eq!(hourly.len(), 1, "exactly one hourly bucket should exist");
        assert_eq!(hourly[0].bytes_in, 500, "delta_in should be 1500 - 1000");
        assert_eq!(hourly[0].bytes_out, 300, "delta_out should be 2300 - 2000");
        assert_eq!(
            hourly[0].hour, expected_hour,
            "hourly bucket should be keyed at the tick's hour boundary"
        );

        // State is updated to the latest counters.
        let state = traffic_state::Entity::find()
            .filter(traffic_state::Column::ServerId.eq("srv-d"))
            .one(&db)
            .await
            .expect("query traffic_state should succeed")
            .expect("traffic_state row should exist");
        assert_eq!(state.last_in, 1_500);
        assert_eq!(state.last_out, 2_300);

        // Two records total (one per tick).
        let records = record::Entity::find()
            .filter(record::Column::ServerId.eq("srv-d"))
            .all(&db)
            .await
            .expect("query records should succeed");
        assert_eq!(records.len(), 2, "two ticks should yield two records");
    }
}
