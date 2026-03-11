use chrono::{DateTime, Duration, Utc};
use sea_orm::*;

use crate::entity::{gpu_record, record, record_hourly};
use crate::error::AppError;
use serverbee_common::types::{GpuReport, SystemReport};

pub struct RecordService;

impl RecordService {
    /// Save a system report as a record for the given server.
    pub async fn save_report(
        db: &DatabaseConnection,
        server_id: &str,
        report: &SystemReport,
    ) -> Result<(), AppError> {
        let gpu_usage = report.gpu.as_ref().map(|g| g.average_usage);

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

    /// Aggregate records from the last hour into hourly averages per server.
    pub async fn aggregate_hourly(db: &DatabaseConnection) -> Result<u64, AppError> {
        let now = Utc::now();
        let one_hour_ago = now - Duration::hours(1);

        // Get distinct server IDs with records in the last hour
        let records = record::Entity::find()
            .filter(record::Column::Time.gte(one_hour_ago))
            .filter(record::Column::Time.lt(now))
            .all(db)
            .await?;

        if records.is_empty() {
            return Ok(0);
        }

        // Group records by server_id
        let mut grouped: std::collections::HashMap<String, Vec<&record::Model>> =
            std::collections::HashMap::new();
        for r in &records {
            grouped.entry(r.server_id.clone()).or_default().push(r);
        }

        let mut inserted = 0u64;

        for (server_id, server_records) in &grouped {
            let count = server_records.len() as f64;

            let avg_cpu = server_records.iter().map(|r| r.cpu).sum::<f64>() / count;
            let avg_mem = (server_records.iter().map(|r| r.mem_used).sum::<i64>() as f64 / count)
                as i64;
            let avg_swap = (server_records.iter().map(|r| r.swap_used).sum::<i64>() as f64 / count)
                as i64;
            let avg_disk = (server_records.iter().map(|r| r.disk_used).sum::<i64>() as f64 / count)
                as i64;
            let avg_net_in_speed =
                (server_records.iter().map(|r| r.net_in_speed).sum::<i64>() as f64 / count) as i64;
            let avg_net_out_speed =
                (server_records.iter().map(|r| r.net_out_speed).sum::<i64>() as f64 / count)
                    as i64;
            let avg_net_in_transfer =
                (server_records.iter().map(|r| r.net_in_transfer).sum::<i64>() as f64 / count)
                    as i64;
            let avg_net_out_transfer =
                (server_records.iter().map(|r| r.net_out_transfer).sum::<i64>() as f64 / count)
                    as i64;
            let avg_load1 = server_records.iter().map(|r| r.load1).sum::<f64>() / count;
            let avg_load5 = server_records.iter().map(|r| r.load5).sum::<f64>() / count;
            let avg_load15 = server_records.iter().map(|r| r.load15).sum::<f64>() / count;
            let avg_tcp =
                (server_records.iter().map(|r| r.tcp_conn as i64).sum::<i64>() as f64 / count)
                    as i32;
            let avg_udp =
                (server_records.iter().map(|r| r.udp_conn as i64).sum::<i64>() as f64 / count)
                    as i32;
            let avg_process = (server_records
                .iter()
                .map(|r| r.process_count as i64)
                .sum::<i64>() as f64
                / count) as i32;

            let temps: Vec<f64> = server_records
                .iter()
                .filter_map(|r| r.temperature)
                .collect();
            let avg_temp = if temps.is_empty() {
                None
            } else {
                Some(temps.iter().sum::<f64>() / temps.len() as f64)
            };

            let gpus: Vec<f64> = server_records
                .iter()
                .filter_map(|r| r.gpu_usage)
                .collect();
            let avg_gpu = if gpus.is_empty() {
                None
            } else {
                Some(gpus.iter().sum::<f64>() / gpus.len() as f64)
            };

            let hourly = record_hourly::ActiveModel {
                id: NotSet,
                server_id: Set(server_id.clone()),
                time: Set(one_hour_ago),
                cpu: Set(avg_cpu),
                mem_used: Set(avg_mem),
                swap_used: Set(avg_swap),
                disk_used: Set(avg_disk),
                net_in_speed: Set(avg_net_in_speed),
                net_out_speed: Set(avg_net_out_speed),
                net_in_transfer: Set(avg_net_in_transfer),
                net_out_transfer: Set(avg_net_out_transfer),
                load1: Set(avg_load1),
                load5: Set(avg_load5),
                load15: Set(avg_load15),
                tcp_conn: Set(avg_tcp),
                udp_conn: Set(avg_udp),
                process_count: Set(avg_process),
                temperature: Set(avg_temp),
                gpu_usage: Set(avg_gpu),
            };

            hourly.insert(db).await?;
            inserted += 1;
        }

        Ok(inserted)
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
