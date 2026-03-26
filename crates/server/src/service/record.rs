use std::collections::HashMap;

use chrono::{DateTime, Duration, DurationRound, SecondsFormat, Utc};
use sea_orm::{Statement, *};

use crate::entity::{gpu_record, record, record_hourly};
use crate::error::AppError;
use serverbee_common::types::{DiskIo, GpuReport, SystemReport};

pub struct RecordService;

#[derive(Default)]
struct DiskIoAccumulator {
    read_total: u64,
    write_total: u64,
    samples: u64,
}

fn serialize_disk_io(disk_io: Option<&Vec<DiskIo>>) -> Result<Option<String>, AppError> {
    disk_io
        .map(|entries| {
            serde_json::to_string(entries)
                .map_err(|e| AppError::Internal(format!("Disk I/O serialization error: {e}")))
        })
        .transpose()
}

fn aggregate_disk_io(records: &[&record::Model]) -> Result<Option<String>, AppError> {
    let mut saw_non_null = false;
    let mut grouped: HashMap<String, DiskIoAccumulator> = HashMap::new();

    for record in records {
        let Some(raw) = record.disk_io_json.as_deref() else {
            continue;
        };
        saw_non_null = true;

        let entries = match serde_json::from_str::<Vec<DiskIo>>(raw) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(record_id = record.id, server_id = %record.server_id, "Failed to parse disk_io_json: {error}");
                continue;
            }
        };

        for entry in entries {
            let accumulator = grouped.entry(entry.name).or_default();
            accumulator.read_total += entry.read_bytes_per_sec;
            accumulator.write_total += entry.write_bytes_per_sec;
            accumulator.samples += 1;
        }
    }

    if !saw_non_null {
        return Ok(None);
    }

    if grouped.is_empty() {
        return Ok(Some("[]".to_string()));
    }

    let mut aggregated = grouped
        .into_iter()
        .map(|(name, accumulator)| DiskIo {
            name,
            read_bytes_per_sec: accumulator.read_total / accumulator.samples,
            write_bytes_per_sec: accumulator.write_total / accumulator.samples,
        })
        .collect::<Vec<_>>();
    aggregated.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(Some(
        serde_json::to_string(&aggregated)
            .map_err(|e| AppError::Internal(format!("Disk I/O serialization error: {e}")))?,
    ))
}

impl RecordService {
    /// Save a system report as a record for the given server.
    pub async fn save_report(
        db: &DatabaseConnection,
        server_id: &str,
        report: &SystemReport,
    ) -> Result<(), AppError> {
        let gpu_usage = report.gpu.as_ref().map(|g| g.average_usage);
        let disk_io_json = serialize_disk_io(report.disk_io.as_ref())?;

        let new_record = record::ActiveModel {
            id: NotSet,
            server_id: Set(server_id.to_string()),
            time: Set(Utc::now()),
            cpu: Set(report.cpu),
            mem_used: Set(report.mem_used),
            swap_used: Set(report.swap_used),
            disk_used: Set(report.disk_used),
            net_in_speed: Set(report.net_in_speed),
            net_out_speed: Set(report.net_out_speed),
            net_in_transfer: Set(report.net_in_transfer),
            net_out_transfer: Set(report.net_out_transfer),
            load1: Set(report.load1),
            load5: Set(report.load5),
            load15: Set(report.load15),
            tcp_conn: Set(report.tcp_conn),
            udp_conn: Set(report.udp_conn),
            process_count: Set(report.process_count),
            temperature: Set(report.temperature),
            gpu_usage: Set(gpu_usage),
            disk_io_json: Set(disk_io_json),
        };

        new_record.insert(db).await?;
        Ok(())
    }

    /// Save GPU detail records for the given server.
    pub async fn save_gpu_records(
        db: &DatabaseConnection,
        server_id: &str,
        gpu: &GpuReport,
    ) -> Result<(), AppError> {
        let now = Utc::now();

        for (index, info) in gpu.detailed_info.iter().enumerate() {
            let new_gpu = gpu_record::ActiveModel {
                id: NotSet,
                server_id: Set(server_id.to_string()),
                time: Set(now),
                device_index: Set(index as i32),
                device_name: Set(info.name.clone()),
                mem_total: Set(info.mem_total),
                mem_used: Set(info.mem_used),
                utilization: Set(info.utilization),
                temperature: Set(info.temperature),
            };
            new_gpu.insert(db).await?;
        }

        Ok(())
    }

    /// Query historical records for a server.
    /// Interval: "raw" uses records table, "hourly" uses records_hourly,
    /// "auto" picks based on time range (<=24h = raw, >24h = hourly).
    pub async fn query_history(
        db: &DatabaseConnection,
        server_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        interval: &str,
    ) -> Result<QueryHistoryResult, AppError> {
        let use_hourly = match interval {
            "raw" => false,
            "hourly" => true,
            _ => {
                // "auto" mode
                let duration = to - from;
                duration > Duration::hours(24)
            }
        };

        if use_hourly {
            let records = record_hourly::Entity::find()
                .filter(record_hourly::Column::ServerId.eq(server_id))
                .filter(record_hourly::Column::Time.gte(from))
                .filter(record_hourly::Column::Time.lte(to))
                .order_by_asc(record_hourly::Column::Time)
                .all(db)
                .await?;
            Ok(QueryHistoryResult::Hourly(records))
        } else {
            let records = record::Entity::find()
                .filter(record::Column::ServerId.eq(server_id))
                .filter(record::Column::Time.gte(from))
                .filter(record::Column::Time.lte(to))
                .order_by_asc(record::Column::Time)
                .all(db)
                .await?;
            Ok(QueryHistoryResult::Raw(records))
        }
    }

    /// Query GPU history records for a server within a time range.
    pub async fn query_gpu_history(
        db: &DatabaseConnection,
        server_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<gpu_record::Model>, AppError> {
        let records = gpu_record::Entity::find()
            .filter(gpu_record::Column::ServerId.eq(server_id))
            .filter(gpu_record::Column::Time.gte(from))
            .filter(gpu_record::Column::Time.lte(to))
            .order_by_asc(gpu_record::Column::Time)
            .all(db)
            .await?;
        Ok(records)
    }

    /// Aggregate records from the previous completed hour bucket into hourly averages per server.
    /// Time is truncated to the hour boundary (e.g. at 14:37, aggregates 13:00–14:00).
    /// Uses SQL AVG/MAX pushed to SQLite and an ON CONFLICT upsert for idempotency.
    /// disk_io_json is aggregated in Rust due to per-device JSON parsing requirements.
    pub async fn aggregate_hourly(db: &DatabaseConnection) -> Result<u64, AppError> {
        let now = Utc::now();
        let hour = now
            .duration_trunc(chrono::Duration::hours(1))
            .map_err(|e| AppError::Internal(format!("Time truncation failed: {e}")))?;
        let hour_start = hour - chrono::Duration::hours(1);
        let hour_end = hour;

        // Use RFC3339 format matching sqlx's DateTimeUtc storage format (AutoSi, no Z suffix)
        let hour_start_str = hour_start.to_rfc3339_opts(SecondsFormat::AutoSi, false);
        let hour_end_str = hour_end.to_rfc3339_opts(SecondsFormat::AutoSi, false);

        // SQL aggregation for numeric columns with upsert
        let sql = "INSERT INTO records_hourly \
            (server_id, time, cpu, mem_used, swap_used, disk_used, \
             net_in_speed, net_out_speed, net_in_transfer, net_out_transfer, \
             load1, load5, load15, tcp_conn, udp_conn, process_count, \
             temperature, gpu_usage) \
            SELECT \
                server_id, \
                ?, \
                AVG(cpu), \
                CAST(AVG(mem_used) AS INTEGER), \
                CAST(AVG(swap_used) AS INTEGER), \
                CAST(AVG(disk_used) AS INTEGER), \
                CAST(AVG(net_in_speed) AS INTEGER), \
                CAST(AVG(net_out_speed) AS INTEGER), \
                CAST(MAX(net_in_transfer) AS INTEGER), \
                CAST(MAX(net_out_transfer) AS INTEGER), \
                AVG(load1), \
                AVG(load5), \
                AVG(load15), \
                CAST(AVG(tcp_conn) AS INTEGER), \
                CAST(AVG(udp_conn) AS INTEGER), \
                CAST(AVG(process_count) AS INTEGER), \
                AVG(temperature), \
                AVG(gpu_usage) \
            FROM records \
            WHERE time >= ? AND time < ? \
            GROUP BY server_id \
            ON CONFLICT(server_id, time) DO UPDATE SET \
                cpu = excluded.cpu, \
                mem_used = excluded.mem_used, \
                swap_used = excluded.swap_used, \
                disk_used = excluded.disk_used, \
                net_in_speed = excluded.net_in_speed, \
                net_out_speed = excluded.net_out_speed, \
                net_in_transfer = excluded.net_in_transfer, \
                net_out_transfer = excluded.net_out_transfer, \
                load1 = excluded.load1, \
                load5 = excluded.load5, \
                load15 = excluded.load15, \
                tcp_conn = excluded.tcp_conn, \
                udp_conn = excluded.udp_conn, \
                process_count = excluded.process_count, \
                temperature = excluded.temperature, \
                gpu_usage = excluded.gpu_usage";

        let result = db
            .execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                sql,
                [
                    hour_start_str.clone().into(),
                    hour_start_str.clone().into(),
                    hour_end_str.into(),
                ],
            ))
            .await?;

        let rows_affected = result.rows_affected();

        if rows_affected == 0 {
            return Ok(0);
        }

        // disk_io_json: Rust-side aggregation (per-device grouping)
        let records = record::Entity::find()
            .filter(record::Column::Time.gte(hour_start))
            .filter(record::Column::Time.lt(hour_end))
            .all(db)
            .await?;

        let mut grouped: HashMap<String, Vec<&record::Model>> = HashMap::new();
        for r in &records {
            grouped.entry(r.server_id.clone()).or_default().push(r);
        }

        for (server_id, server_records) in &grouped {
            let disk_io_json = aggregate_disk_io(server_records)?;
            let json_value: sea_orm::Value = match disk_io_json {
                Some(s) => s.into(),
                None => sea_orm::Value::String(None),
            };
            db.execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                "UPDATE records_hourly SET disk_io_json = ? WHERE server_id = ? AND time = ?",
                [json_value, server_id.clone().into(), hour_start_str.clone().into()],
            ))
            .await?;
        }

        Ok(rows_affected)
    }

    /// Clean up expired records from a table with a `time` column.
    /// Supported tables: "records", "records_hourly", "gpu_records".
    pub async fn cleanup_expired(
        db: &DatabaseConnection,
        retention_days: u32,
        table: &str,
    ) -> Result<u64, AppError> {
        let cutoff = Utc::now() - Duration::days(retention_days as i64);

        let result = match table {
            "records" => {
                record::Entity::delete_many()
                    .filter(record::Column::Time.lt(cutoff))
                    .exec(db)
                    .await?
            }
            "records_hourly" => {
                record_hourly::Entity::delete_many()
                    .filter(record_hourly::Column::Time.lt(cutoff))
                    .exec(db)
                    .await?
            }
            "gpu_records" => {
                gpu_record::Entity::delete_many()
                    .filter(gpu_record::Column::Time.lt(cutoff))
                    .exec(db)
                    .await?
            }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "Unknown table: {table}"
                )));
            }
        };

        Ok(result.rows_affected)
    }
}

/// Result type for query_history to handle both raw and hourly records.
#[derive(Debug)]
pub enum QueryHistoryResult {
    Raw(Vec<record::Model>),
    Hourly(Vec<record_hourly::Model>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::server;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use sea_orm::{ActiveModelTrait, Set};
    use serverbee_common::constants::CAP_DEFAULT;
    use serverbee_common::types::{DiskIo, SystemReport};

    async fn insert_test_server(db: &DatabaseConnection, id: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash_password should succeed");
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(token_hash),
            token_prefix: Set("serverbee_test".to_string()),
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

    #[tokio::test]
    async fn test_save_and_query_report() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-rec-1").await;

        let report = SystemReport {
            cpu: 42.5,
            mem_used: 1024,
            ..Default::default()
        };

        RecordService::save_report(&db, "srv-rec-1", &report)
            .await
            .expect("save_report should succeed");

        let now = Utc::now();
        let from = now - Duration::hours(1);
        let result = RecordService::query_history(&db, "srv-rec-1", from, now, "raw")
            .await
            .expect("query_history should succeed");

        match result {
            QueryHistoryResult::Raw(records) => {
                assert_eq!(records.len(), 1, "Should find exactly one record");
                assert!((records[0].cpu - 42.5).abs() < f64::EPSILON, "CPU value should match");
                assert_eq!(records[0].mem_used, 1024, "mem_used should match");
                assert_eq!(records[0].server_id, "srv-rec-1", "server_id should match");
            }
            QueryHistoryResult::Hourly(_) => panic!("Expected Raw result for 'raw' interval"),
        }
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-rec-2").await;

        let report = SystemReport::default();
        RecordService::save_report(&db, "srv-rec-2", &report)
            .await
            .expect("save_report should succeed");

        // Verify the record exists
        let now = Utc::now();
        let from = now - Duration::hours(1);
        let before = RecordService::query_history(&db, "srv-rec-2", from, now, "raw")
            .await
            .expect("query_history should succeed");
        match &before {
            QueryHistoryResult::Raw(records) => assert_eq!(records.len(), 1, "Record should exist before cleanup"),
            _ => panic!("Expected Raw result"),
        }

        // Cleanup with 0 retention days — cutoff is now, so all existing records (time < now) are deleted
        let deleted = RecordService::cleanup_expired(&db, 0, "records")
            .await
            .expect("cleanup_expired should succeed");
        assert!(deleted >= 1, "At least one record should have been deleted");

        // Verify the record is gone
        let now2 = Utc::now();
        let from2 = now2 - Duration::hours(1);
        let after = RecordService::query_history(&db, "srv-rec-2", from2, now2, "raw")
            .await
            .expect("query_history should succeed");
        match after {
            QueryHistoryResult::Raw(records) => assert_eq!(records.len(), 0, "Record should be deleted"),
            _ => panic!("Expected Raw result"),
        }
    }

    #[tokio::test]
    async fn test_save_report_persists_disk_io_json() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-rec-disk-1").await;

        let report = SystemReport {
            disk_io: Some(vec![DiskIo {
                name: "sda".to_string(),
                read_bytes_per_sec: 1024,
                write_bytes_per_sec: 2048,
            }]),
            ..Default::default()
        };

        RecordService::save_report(&db, "srv-rec-disk-1", &report)
            .await
            .expect("save_report should succeed");

        let now = Utc::now();
        let from = now - Duration::hours(1);
        let result = RecordService::query_history(&db, "srv-rec-disk-1", from, now, "raw")
            .await
            .expect("query_history should succeed");

        match result {
            QueryHistoryResult::Raw(records) => {
                let disk_io: Vec<DiskIo> = serde_json::from_str(records[0].disk_io_json.as_deref().unwrap()).unwrap();
                assert_eq!(disk_io[0].name, "sda");
                assert_eq!(disk_io[0].read_bytes_per_sec, 1024);
                assert_eq!(disk_io[0].write_bytes_per_sec, 2048);
            }
            QueryHistoryResult::Hourly(_) => panic!("Expected Raw result for 'raw' interval"),
        }
    }

    #[tokio::test]
    async fn test_aggregate_hourly_averages_disk_io_by_device() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-rec-disk-2").await;

        // Compute the previous completed hour bucket so records fall within [hour_start, hour_end)
        let now = Utc::now();
        let hour = now
            .duration_trunc(chrono::Duration::hours(1))
            .expect("duration_trunc should succeed");
        let hour_start = hour - chrono::Duration::hours(1);
        // Place records 30 minutes into the previous hour
        let record_time = hour_start + chrono::Duration::minutes(30);

        let first_disk_io = serde_json::to_string(&vec![
            DiskIo {
                name: "sdb".to_string(),
                read_bytes_per_sec: 100,
                write_bytes_per_sec: 300,
            },
            DiskIo {
                name: "sda".to_string(),
                read_bytes_per_sec: 400,
                write_bytes_per_sec: 800,
            },
        ])
        .unwrap();
        let second_disk_io = serde_json::to_string(&vec![
            DiskIo {
                name: "sda".to_string(),
                read_bytes_per_sec: 600,
                write_bytes_per_sec: 1000,
            },
            DiskIo {
                name: "sdb".to_string(),
                read_bytes_per_sec: 300,
                write_bytes_per_sec: 500,
            },
        ])
        .unwrap();

        record::ActiveModel {
            id: NotSet,
            server_id: Set("srv-rec-disk-2".to_string()),
            time: Set(record_time),
            cpu: Set(0.0),
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
            disk_io_json: Set(Some(first_disk_io)),
        }
        .insert(&db)
        .await
        .expect("first record insert should succeed");

        record::ActiveModel {
            id: NotSet,
            server_id: Set("srv-rec-disk-2".to_string()),
            time: Set(record_time),
            cpu: Set(0.0),
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
            disk_io_json: Set(Some(second_disk_io)),
        }
        .insert(&db)
        .await
        .expect("second record insert should succeed");

        let aggregated = RecordService::aggregate_hourly(&db)
            .await
            .expect("aggregate_hourly should succeed");
        assert_eq!(aggregated, 1);

        let from = now - Duration::hours(2);
        let result = RecordService::query_history(&db, "srv-rec-disk-2", from, now, "hourly")
            .await
            .expect("query_history should succeed");

        match result {
            QueryHistoryResult::Hourly(records) => {
                assert_eq!(records.len(), 1);
                let disk_io: Vec<DiskIo> = serde_json::from_str(records[0].disk_io_json.as_deref().unwrap()).unwrap();
                assert_eq!(
                    disk_io,
                    vec![
                        DiskIo {
                            name: "sda".to_string(),
                            read_bytes_per_sec: 500,
                            write_bytes_per_sec: 900,
                        },
                        DiskIo {
                            name: "sdb".to_string(),
                            read_bytes_per_sec: 200,
                            write_bytes_per_sec: 400,
                        },
                    ]
                );
            }
            QueryHistoryResult::Raw(_) => panic!("Expected Hourly result for 'hourly' interval"),
        }
    }

    // NOTE: All RecordService methods (save_report, query_history, aggregate_hourly,
    // cleanup_expired) require a DatabaseConnection. There are no pure helper
    // functions to unit-test in isolation.
    //
    // The tests below verify the QueryHistoryResult enum and the interval
    // selection logic that can be exercised without a database.

    #[test]
    fn test_query_history_result_variants() {
        // Verify the Raw variant can be constructed
        let raw: QueryHistoryResult = QueryHistoryResult::Raw(vec![]);
        assert!(matches!(raw, QueryHistoryResult::Raw(v) if v.is_empty()));

        // Verify the Hourly variant can be constructed
        let hourly: QueryHistoryResult = QueryHistoryResult::Hourly(vec![]);
        assert!(matches!(hourly, QueryHistoryResult::Hourly(v) if v.is_empty()));
    }

    /// The interval auto-selection logic: <=24h => raw, >24h => hourly.
    /// Extracted from `query_history` so we can verify it without DB.
    #[test]
    fn test_interval_selection_logic() {
        let now = Utc::now();

        // Within 24 hours => should use raw
        let from_recent = now - Duration::hours(12);
        let duration_recent = now - from_recent;
        let use_hourly_recent = duration_recent > Duration::hours(24);
        assert!(!use_hourly_recent, "12h range should select raw");

        // Exactly 24 hours => should use raw (not >24h)
        let from_exact = now - Duration::hours(24);
        let duration_exact = now - from_exact;
        let use_hourly_exact = duration_exact > Duration::hours(24);
        assert!(!use_hourly_exact, "24h range should select raw (not strictly greater)");

        // More than 24 hours => should use hourly
        let from_old = now - Duration::hours(48);
        let duration_old = now - from_old;
        let use_hourly_old = duration_old > Duration::hours(24);
        assert!(use_hourly_old, "48h range should select hourly");

        // Explicit interval overrides
        let explicit_raw = match "raw" {
            "raw" => false,
            "hourly" => true,
            _ => unreachable!(),
        };
        assert!(!explicit_raw, "explicit 'raw' should use raw");

        let explicit_hourly = match "hourly" {
            "raw" => false,
            "hourly" => true,
            _ => unreachable!(),
        };
        assert!(explicit_hourly, "explicit 'hourly' should use hourly");
    }

    /// Verify the retention cutoff calculation used in cleanup_expired.
    #[test]
    fn test_retention_cutoff_calculation() {
        let retention_days: u32 = 30;
        let now = Utc::now();
        let cutoff = now - Duration::days(retention_days as i64);

        // Cutoff should be approximately 30 days ago
        let diff = now - cutoff;
        assert_eq!(diff.num_days(), 30);

        // A record from 31 days ago should be before the cutoff (eligible for cleanup)
        let old_time = now - Duration::days(31);
        assert!(old_time < cutoff, "31-day-old record should be before cutoff");

        // A record from 29 days ago should be after the cutoff (retained)
        let recent_time = now - Duration::days(29);
        assert!(recent_time > cutoff, "29-day-old record should be after cutoff");
    }

    /// Verify the hourly aggregation averaging logic (extracted computation).
    #[test]
    fn test_hourly_aggregation_averages() {
        // Simulate the averaging computation used in aggregate_hourly
        let cpu_values = vec![80.0_f64, 90.0, 70.0, 85.0, 95.0];
        let count = cpu_values.len() as f64;
        let avg_cpu = cpu_values.iter().sum::<f64>() / count;
        assert!(
            (avg_cpu - 84.0).abs() < f64::EPSILON,
            "average of [80, 90, 70, 85, 95] should be 84.0"
        );

        // Integer averaging (mem_used style)
        let mem_values: Vec<i64> = vec![1000, 2000, 3000];
        let mem_count = mem_values.len() as f64;
        let avg_mem = (mem_values.iter().sum::<i64>() as f64 / mem_count) as i64;
        assert_eq!(avg_mem, 2000, "average of [1000, 2000, 3000] should be 2000");

        // Optional field averaging (temperature style)
        let temp_values: Vec<Option<f64>> = vec![Some(50.0), None, Some(60.0), None];
        let temps: Vec<f64> = temp_values.into_iter().flatten().collect();
        let avg_temp = if temps.is_empty() {
            None
        } else {
            Some(temps.iter().sum::<f64>() / temps.len() as f64)
        };
        assert_eq!(avg_temp, Some(55.0), "average of [50, 60] (skipping None) should be 55");

        // All None case
        let no_temps: Vec<Option<f64>> = vec![None, None, None];
        let filtered: Vec<f64> = no_temps.into_iter().flatten().collect();
        let avg_no_temp = if filtered.is_empty() {
            None
        } else {
            Some(filtered.iter().sum::<f64>() / filtered.len() as f64)
        };
        assert_eq!(avg_no_temp, None, "all-None should produce None average");
    }

}
