use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate, SecondsFormat, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, EntityTrait, Statement};
use serde::Serialize;

use crate::entity::{server, traffic_state};
use crate::error::AppError;

pub struct TrafficService;

impl TrafficService {
    pub async fn merge_recovered_server_history(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError> {
        Self::merge_recovered_server_history_on_connection(db, target_server_id, source_server_id)
            .await
    }

    pub async fn merge_recovered_server_history_on_txn(
        txn: &DatabaseTransaction,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError> {
        Self::merge_recovered_server_history_on_connection(txn, target_server_id, source_server_id)
            .await
    }

    pub(crate) async fn merge_recovered_server_history_on_connection<C>(
        db: &C,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        Self::replace_unique_key_table_server_id_on_connection(
            db,
            "traffic_hourly",
            &["hour"],
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::replace_unique_key_table_server_id_on_connection(
            db,
            "traffic_daily",
            &["date"],
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::replace_unique_key_table_server_id_on_connection(
            db,
            "traffic_state",
            &[],
            target_server_id,
            source_server_id,
        )
        .await?;

        Ok(())
    }

    pub(crate) async fn replace_unique_key_table_server_id_on_connection<C>(
        db: &C,
        table: &str,
        key_columns: &[&str],
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        let join_predicate = if key_columns.is_empty() {
            "1 = 1".to_string()
        } else {
            key_columns
                .iter()
                .map(|column| format!("source.{column} = target.{column}"))
                .collect::<Vec<_>>()
                .join(" AND ")
        };

        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            format!(
                "DELETE FROM {table} AS target \
                 WHERE target.server_id = $1 \
                 AND EXISTS ( \
                     SELECT 1 FROM {table} AS source \
                     WHERE source.server_id = $2 \
                     AND {join_predicate} \
                 )"
            ),
            [target_server_id.into(), source_server_id.into()],
        ))
        .await?;

        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            format!("UPDATE {table} SET server_id = $1 WHERE server_id = $2"),
            [target_server_id.into(), source_server_id.into()],
        ))
        .await?;

        Ok(())
    }

    /// Upsert a traffic_hourly row, accumulating bytes_in/bytes_out on conflict.
    pub async fn upsert_hourly(
        db: &DatabaseConnection,
        server_id: &str,
        hour: chrono::DateTime<Utc>,
        delta_in: i64,
        delta_out: i64,
    ) -> Result<(), AppError> {
        let hour_str = hour.format("%Y-%m-%d %H:%M:%S").to_string();
        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            "INSERT INTO traffic_hourly (server_id, hour, bytes_in, bytes_out) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT(server_id, hour) DO UPDATE SET \
             bytes_in = traffic_hourly.bytes_in + excluded.bytes_in, \
             bytes_out = traffic_hourly.bytes_out + excluded.bytes_out",
            [
                server_id.into(),
                hour_str.into(),
                delta_in.into(),
                delta_out.into(),
            ],
        ))
        .await?;
        Ok(())
    }

    /// Upsert a traffic_state row.
    pub async fn upsert_state(
        db: &DatabaseConnection,
        server_id: &str,
        last_in: i64,
        last_out: i64,
    ) -> Result<(), AppError> {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            "INSERT INTO traffic_state (server_id, last_in, last_out, updated_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT(server_id) DO UPDATE SET \
             last_in = excluded.last_in, \
             last_out = excluded.last_out, \
             updated_at = excluded.updated_at",
            [
                server_id.into(),
                last_in.into(),
                last_out.into(),
                now.into(),
            ],
        ))
        .await?;
        Ok(())
    }

    /// Load all traffic_state rows into a HashMap for the transfer cache.
    pub async fn load_transfer_cache(
        db: &DatabaseConnection,
    ) -> Result<HashMap<String, (i64, i64)>, AppError> {
        let rows = traffic_state::Entity::find().all(db).await?;
        let mut cache = HashMap::new();
        for row in rows {
            cache.insert(row.server_id, (row.last_in, row.last_out));
        }
        Ok(cache)
    }

    /// Aggregate hourly traffic into daily buckets using the given timezone.
    pub async fn aggregate_daily(db: &DatabaseConnection, timezone: &str) -> Result<u64, AppError> {
        use chrono_tz::Tz;
        let tz: Tz = timezone
            .parse()
            .map_err(|_| AppError::Internal(format!("Invalid timezone: {timezone}")))?;

        // Get all hourly rows that haven't been aggregated into daily yet
        // We aggregate yesterday and today in local timezone
        let now = Utc::now().with_timezone(&tz);
        let today_local = now.date_naive();
        let yesterday_local = today_local - Duration::days(1);

        // Process yesterday and today
        let mut total_affected = 0u64;
        for date in [yesterday_local, today_local] {
            total_affected += Self::aggregate_daily_for_date(db, date, &tz).await?;
        }

        Ok(total_affected)
    }

    async fn aggregate_daily_for_date(
        db: &DatabaseConnection,
        date: NaiveDate,
        tz: &chrono_tz::Tz,
    ) -> Result<u64, AppError> {
        use chrono::TimeZone;
        // Convert local date boundaries to UTC
        let start_local = date.and_hms_opt(0, 0, 0).unwrap();
        let end_local = date.and_hms_opt(23, 59, 59).unwrap();

        let start_utc = tz
            .from_local_datetime(&start_local)
            .earliest()
            .unwrap_or_else(|| tz.from_local_datetime(&start_local).latest().unwrap())
            .with_timezone(&Utc);
        let end_utc = tz
            .from_local_datetime(&end_local)
            .latest()
            .unwrap_or_else(|| tz.from_local_datetime(&end_local).earliest().unwrap())
            .with_timezone(&Utc);

        let start_str = start_utc.format("%Y-%m-%d %H:%M:%S").to_string();
        let end_str = end_utc.format("%Y-%m-%d %H:%M:%S").to_string();
        let date_str = date.format("%Y-%m-%d").to_string();

        // Aggregate and upsert into traffic_daily
        let result = db
            .execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                "INSERT INTO traffic_daily (server_id, date, bytes_in, bytes_out) \
                 SELECT server_id, $1, SUM(bytes_in), SUM(bytes_out) \
                 FROM traffic_hourly \
                 WHERE hour >= $2 AND hour <= $3 \
                 GROUP BY server_id \
                 ON CONFLICT(server_id, date) DO UPDATE SET \
                 bytes_in = excluded.bytes_in, \
                 bytes_out = excluded.bytes_out",
                [date_str.into(), start_str.into(), end_str.into()],
            ))
            .await?;

        Ok(result.rows_affected())
    }

    /// Clean up traffic_hourly rows older than the given number of days.
    pub async fn cleanup_hourly(db: &DatabaseConnection, days: u32) -> Result<u64, AppError> {
        let cutoff = (Utc::now() - Duration::days(days as i64))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let result = db
            .execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                "DELETE FROM traffic_hourly WHERE hour < $1",
                [cutoff.into()],
            ))
            .await?;
        Ok(result.rows_affected())
    }

    /// Clean up traffic_daily rows older than the given number of days.
    pub async fn cleanup_daily(db: &DatabaseConnection, days: u32) -> Result<u64, AppError> {
        let cutoff = (Utc::now() - Duration::days(days as i64))
            .naive_utc()
            .date()
            .format("%Y-%m-%d")
            .to_string();
        let result = db
            .execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                "DELETE FROM traffic_daily WHERE date < $1",
                [cutoff.into()],
            ))
            .await?;
        Ok(result.rows_affected())
    }

    /// Clean up task_results older than the given number of days.
    pub async fn cleanup_task_results(db: &DatabaseConnection, days: u32) -> Result<u64, AppError> {
        // Use RFC3339 format to match sea-orm's DateTimeUtc storage format
        let cutoff = (Utc::now() - Duration::days(days as i64))
            .to_rfc3339_opts(SecondsFormat::AutoSi, false);
        let result = db
            .execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                "DELETE FROM task_results WHERE finished_at < $1",
                [cutoff.into()],
            ))
            .await?;
        Ok(result.rows_affected())
    }

    /// Query total traffic for a server within a date range, combining daily + hourly for today.
    pub async fn query_cycle_traffic(
        db: &DatabaseConnection,
        server_id: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<(i64, i64), AppError> {
        let start_str = start_date.format("%Y-%m-%d").to_string();
        let end_str = end_date.format("%Y-%m-%d").to_string();

        // Query daily totals
        let result = db
            .query_one(Statement::from_sql_and_values(
                db.get_database_backend(),
                "SELECT COALESCE(SUM(bytes_in), 0) as total_in, COALESCE(SUM(bytes_out), 0) as total_out \
                 FROM traffic_daily \
                 WHERE server_id = $1 AND date >= $2 AND date <= $3",
                [server_id.into(), start_str.into(), end_str.into()],
            ))
            .await?;

        let (daily_in, daily_out) = match result {
            Some(row) => {
                let bytes_in: i64 = row.try_get_by_index(0).unwrap_or(0);
                let bytes_out: i64 = row.try_get_by_index(1).unwrap_or(0);
                (bytes_in, bytes_out)
            }
            None => (0, 0),
        };

        // Also get today's hourly data that may not yet be aggregated
        let today = Utc::now().naive_utc().date();
        let today_str = today.format("%Y-%m-%d").to_string();

        // Check if today is within the cycle and get any hourly data not yet in daily
        let hourly_result = db
            .query_one(Statement::from_sql_and_values(
                db.get_database_backend(),
                "SELECT COALESCE(SUM(h.bytes_in), 0), COALESCE(SUM(h.bytes_out), 0) \
                 FROM traffic_hourly h \
                 WHERE h.server_id = $1 \
                 AND date(h.hour) = $2 \
                 AND NOT EXISTS (SELECT 1 FROM traffic_daily d WHERE d.server_id = h.server_id AND d.date = $2)",
                [server_id.into(), today_str.into()],
            ))
            .await?;

        let (hourly_in, hourly_out) = match hourly_result {
            Some(row) => {
                let bytes_in: i64 = row.try_get_by_index(0).unwrap_or(0);
                let bytes_out: i64 = row.try_get_by_index(1).unwrap_or(0);
                (bytes_in, bytes_out)
            }
            None => (0, 0),
        };

        Ok((daily_in + hourly_in, daily_out + hourly_out))
    }

    /// Query daily traffic breakdown for charts.
    pub async fn query_daily_breakdown(
        db: &DatabaseConnection,
        server_id: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<DailyTraffic>, AppError> {
        let start_str = start_date.format("%Y-%m-%d").to_string();
        let end_str = end_date.format("%Y-%m-%d").to_string();

        let rows = db
            .query_all(Statement::from_sql_and_values(
                db.get_database_backend(),
                "SELECT date, bytes_in, bytes_out FROM traffic_daily \
                 WHERE server_id = $1 AND date >= $2 AND date <= $3 \
                 ORDER BY date",
                [server_id.into(), start_str.into(), end_str.into()],
            ))
            .await?;

        let mut result = Vec::new();
        for row in rows {
            let date: String = row.try_get_by_index(0).unwrap_or_default();
            let bytes_in: i64 = row.try_get_by_index(1).unwrap_or(0);
            let bytes_out: i64 = row.try_get_by_index(2).unwrap_or(0);
            result.push(DailyTraffic {
                date,
                bytes_in,
                bytes_out,
            });
        }
        Ok(result)
    }

    /// Query hourly traffic for a specific date.
    pub async fn query_hourly_breakdown(
        db: &DatabaseConnection,
        server_id: &str,
        date: NaiveDate,
    ) -> Result<Vec<HourlyTraffic>, AppError> {
        let start = date.and_hms_opt(0, 0, 0).unwrap();
        let end = date.and_hms_opt(23, 59, 59).unwrap();
        let start_str = start.format("%Y-%m-%d %H:%M:%S").to_string();
        let end_str = end.format("%Y-%m-%d %H:%M:%S").to_string();

        let rows = db
            .query_all(Statement::from_sql_and_values(
                db.get_database_backend(),
                "SELECT hour, bytes_in, bytes_out FROM traffic_hourly \
                 WHERE server_id = $1 AND hour >= $2 AND hour <= $3 \
                 ORDER BY hour",
                [server_id.into(), start_str.into(), end_str.into()],
            ))
            .await?;

        let mut result = Vec::new();
        for row in rows {
            let hour: String = row.try_get_by_index(0).unwrap_or_default();
            let bytes_in: i64 = row.try_get_by_index(1).unwrap_or(0);
            let bytes_out: i64 = row.try_get_by_index(2).unwrap_or(0);
            result.push(HourlyTraffic {
                hour,
                bytes_in,
                bytes_out,
            });
        }
        Ok(result)
    }

    /// Traffic overview for all servers that have a billing cycle configured.
    pub async fn overview(db: &DatabaseConnection) -> Result<Vec<ServerTrafficOverview>, AppError> {
        let servers = server::Entity::find().all(db).await?;
        let today = Utc::now().date_naive();
        let mut result = Vec::new();

        for s in servers {
            let billing_cycle = match s.billing_cycle.as_deref() {
                Some(bc) if !bc.is_empty() => bc,
                _ => continue,
            };

            let (cycle_start, cycle_end) =
                get_cycle_range(billing_cycle, s.billing_start_day, today);
            let (cycle_in, cycle_out) =
                Self::query_cycle_traffic(db, &s.id, cycle_start, cycle_end).await?;

            let percent_used = s.traffic_limit.and_then(|limit| {
                if limit > 0 {
                    Some((cycle_in + cycle_out) as f64 / limit as f64 * 100.0)
                } else {
                    None
                }
            });

            let days_remaining = (cycle_end - today).num_days();

            result.push(ServerTrafficOverview {
                server_id: s.id,
                name: s.name,
                cycle_in,
                cycle_out,
                traffic_limit: s.traffic_limit,
                billing_cycle: s.billing_cycle,
                percent_used,
                days_remaining,
            });
        }

        Ok(result)
    }

    /// Global daily traffic aggregation across all servers.
    pub async fn overview_daily(
        db: &DatabaseConnection,
        days: u32,
    ) -> Result<Vec<DailyTraffic>, AppError> {
        let cutoff = (Utc::now().date_naive() - Duration::days(days as i64))
            .format("%Y-%m-%d")
            .to_string();

        let rows = db
            .query_all(Statement::from_sql_and_values(
                db.get_database_backend(),
                "SELECT date, SUM(bytes_in) as bytes_in, SUM(bytes_out) as bytes_out \
                 FROM traffic_daily \
                 WHERE date >= $1 \
                 GROUP BY date \
                 ORDER BY date",
                [cutoff.into()],
            ))
            .await?;

        let mut result = Vec::new();
        for row in rows {
            let date: String = row.try_get_by_index(0).unwrap_or_default();
            let bytes_in: i64 = row.try_get_by_index(1).unwrap_or(0);
            let bytes_out: i64 = row.try_get_by_index(2).unwrap_or(0);
            result.push(DailyTraffic {
                date,
                bytes_in,
                bytes_out,
            });
        }
        Ok(result)
    }

    /// Cycle history for a server: iterate backwards through `count` billing cycles.
    pub async fn cycle_history(
        db: &DatabaseConnection,
        server_id: &str,
        billing_cycle: &str,
        billing_start_day: Option<i32>,
        count: u32,
    ) -> Result<Vec<CycleTraffic>, AppError> {
        let today = Utc::now().date_naive();
        let mut result = Vec::new();

        // Start from the current cycle
        let (mut start, mut end) = get_cycle_range(billing_cycle, billing_start_day, today);

        for _ in 0..count {
            let (bytes_in, bytes_out) =
                Self::query_cycle_traffic(db, server_id, start, end).await?;

            result.push(CycleTraffic {
                period: format!("{} ~ {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d")),
                start: start.format("%Y-%m-%d").to_string(),
                end: end.format("%Y-%m-%d").to_string(),
                bytes_in,
                bytes_out,
            });

            // Move to the previous cycle: go to the day before `start`
            let prev_day = start - Duration::days(1);
            let (prev_start, prev_end) =
                get_cycle_range(billing_cycle, billing_start_day, prev_day);
            start = prev_start;
            end = prev_end;
        }

        Ok(result)
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DailyTraffic {
    pub date: String,
    pub bytes_in: i64,
    pub bytes_out: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct HourlyTraffic {
    pub hour: String,
    pub bytes_in: i64,
    pub bytes_out: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerTrafficOverview {
    pub server_id: String,
    pub name: String,
    pub cycle_in: i64,
    pub cycle_out: i64,
    pub traffic_limit: Option<i64>,
    pub billing_cycle: Option<String>,
    pub percent_used: Option<f64>,
    pub days_remaining: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CycleTraffic {
    pub period: String,
    pub start: String,
    pub end: String,
    pub bytes_in: i64,
    pub bytes_out: i64,
}

/// Compute per-direction independent delta.
/// If a direction's current value < previous, treat as restart (use raw value).
pub fn compute_delta(prev_in: i64, prev_out: i64, curr_in: i64, curr_out: i64) -> (i64, i64) {
    let delta_in = if curr_in >= prev_in {
        curr_in - prev_in
    } else {
        curr_in
    };
    let delta_out = if curr_out >= prev_out {
        curr_out - prev_out
    } else {
        curr_out
    };
    (delta_in, delta_out)
}

/// Compute billing cycle date range.
/// Returns (start_date_inclusive, end_date_inclusive).
pub fn get_cycle_range(
    billing_cycle: &str,
    billing_start_day: Option<i32>,
    today: NaiveDate,
) -> (NaiveDate, NaiveDate) {
    let anchor = billing_start_day.unwrap_or(1).clamp(1, 28);

    match billing_cycle {
        "quarterly" => get_quarterly_range(anchor, today),
        "yearly" => get_yearly_range(anchor, today),
        _ => get_monthly_range(anchor, today), // "monthly" or unknown
    }
}

fn get_monthly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let (y, m) = (today.year(), today.month());

    let cycle_start = if today.day() as i32 >= anchor {
        NaiveDate::from_ymd_opt(y, m, anchor as u32).unwrap()
    } else {
        // Go to previous month
        let prev = today - Duration::days(today.day() as i64);
        NaiveDate::from_ymd_opt(prev.year(), prev.month(), anchor as u32).unwrap()
    };

    // End = day before next anchor
    let cycle_end = if anchor == 1 {
        // Natural month: end is last day of start's month
        let next_month = if cycle_start.month() == 12 {
            NaiveDate::from_ymd_opt(cycle_start.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(cycle_start.year(), cycle_start.month() + 1, 1).unwrap()
        };
        next_month - Duration::days(1)
    } else {
        let next = add_months(cycle_start, 1);
        next - Duration::days(1)
    };

    (cycle_start, cycle_end)
}

fn get_quarterly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let (y, _m) = (today.year(), today.month());
    let quarter_start_months = [1, 4, 7, 10];

    let mut cycle_start = None;
    for &qm in quarter_start_months.iter().rev() {
        let candidate = NaiveDate::from_ymd_opt(y, qm, anchor as u32);
        if let Some(c) = candidate
            && c <= today
        {
            cycle_start = Some(c);
            break;
        }
    }
    let cycle_start =
        cycle_start.unwrap_or_else(|| NaiveDate::from_ymd_opt(y - 1, 10, anchor as u32).unwrap());

    let end = add_months(cycle_start, 3) - Duration::days(1);
    (cycle_start, end)
}

fn get_yearly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(today.year(), 1, anchor as u32).unwrap();
    if start <= today {
        let end = add_months(start, 12) - Duration::days(1);
        (start, end)
    } else {
        let start = NaiveDate::from_ymd_opt(today.year() - 1, 1, anchor as u32).unwrap();
        let end = add_months(start, 12) - Duration::days(1);
        (start, end)
    }
}

fn add_months(date: NaiveDate, months: u32) -> NaiveDate {
    let total_months = date.year() * 12 + date.month() as i32 - 1 + months as i32;
    let y = total_months / 12;
    let m = (total_months % 12) + 1;
    let d = date.day().min(days_in_month(y, m as u32));
    NaiveDate::from_ymd_opt(y, m as u32, d).unwrap()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(
        if month == 12 { year + 1 } else { year },
        if month == 12 { 1 } else { month + 1 },
        1,
    )
    .unwrap()
    .pred_opt()
    .unwrap()
    .day()
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TrafficPrediction {
    pub estimated_total: i64,
    pub estimated_percent: f64,
    pub will_exceed: bool,
}

/// Returns None if days_elapsed < 3 or no traffic_limit set.
pub fn compute_prediction(
    recent_sum: i64,
    days_elapsed: i64,
    days_remaining: i64,
    traffic_limit: Option<i64>,
    _traffic_limit_type: &str,
) -> Option<TrafficPrediction> {
    if days_elapsed < 3 {
        return None;
    }
    let traffic_limit = traffic_limit?;
    if traffic_limit <= 0 {
        return None;
    }

    let daily_avg = recent_sum as f64 / days_elapsed as f64;
    let estimated_total = recent_sum + (daily_avg * days_remaining as f64) as i64;
    let estimated_percent = estimated_total as f64 / traffic_limit as f64 * 100.0;
    let will_exceed = estimated_total > traffic_limit;

    Some(TrafficPrediction {
        estimated_total,
        estimated_percent,
        will_exceed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::traffic_hourly;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    async fn insert_test_server(db: &DatabaseConnection, id: &str) {
        use crate::entity::server;
        use serverbee_common::constants::CAP_DEFAULT;
        let token_hash =
            crate::service::auth::AuthService::hash_password("test").expect("hash should work");
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
        .expect("insert test server");
    }

    #[test]
    fn test_compute_delta_normal() {
        let (d_in, d_out) = compute_delta(100, 200, 150, 250);
        assert_eq!(d_in, 50);
        assert_eq!(d_out, 50);
    }

    #[test]
    fn test_compute_delta_both_restart() {
        let (d_in, d_out) = compute_delta(100_000, 50_000, 500, 300);
        assert_eq!(d_in, 500);
        assert_eq!(d_out, 300);
    }

    #[test]
    fn test_compute_delta_single_direction_restart_in() {
        let (d_in, d_out) = compute_delta(100_000, 50_000, 500, 51_000);
        assert_eq!(d_in, 500);
        assert_eq!(d_out, 1_000);
    }

    #[test]
    fn test_compute_delta_single_direction_restart_out() {
        let (d_in, d_out) = compute_delta(100_000, 50_000, 101_000, 300);
        assert_eq!(d_in, 1_000);
        assert_eq!(d_out, 300);
    }

    #[test]
    fn test_compute_delta_zero() {
        let (d_in, d_out) = compute_delta(100, 200, 100, 200);
        assert_eq!(d_in, 0);
        assert_eq!(d_out, 0);
    }

    #[test]
    fn test_cycle_range_natural_month() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let (start, end) = get_cycle_range("monthly", None, today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }

    #[test]
    fn test_cycle_range_billing_day_15() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let (start, end) = get_cycle_range("monthly", Some(15), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 4, 14).unwrap());
    }

    #[test]
    fn test_cycle_range_billing_day_before_anchor() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let (start, end) = get_cycle_range("monthly", Some(15), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 14).unwrap());
    }

    #[test]
    fn test_cycle_range_quarterly() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 10).unwrap();
        let (start, end) = get_cycle_range("quarterly", Some(1), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 6, 30).unwrap());
    }

    #[test]
    fn test_cycle_range_yearly() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
        let (start, end) = get_cycle_range("yearly", Some(1), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }

    #[test]
    fn test_cycle_range_unknown_falls_back_to_monthly() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let (start, end) = get_cycle_range("unknown", None, today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }

    #[test]
    fn test_prediction_normal() {
        let p = compute_prediction(60_000_000_000, 7, 10, Some(100_000_000_000), "sum");
        assert!(p.is_some());
        let p = p.unwrap();
        assert!(p.estimated_total > 60_000_000_000);
        assert!(p.will_exceed);
    }

    #[test]
    fn test_prediction_too_early() {
        let p = compute_prediction(5_000_000_000, 2, 28, Some(100_000_000_000), "sum");
        assert!(p.is_none());
    }

    #[test]
    fn test_prediction_no_limit() {
        let p = compute_prediction(60_000_000_000, 7, 10, None, "sum");
        assert!(p.is_none());
    }

    #[tokio::test]
    async fn test_upsert_traffic_hourly_accumulates() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let hour = Utc::now()
            .date_naive()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_utc();
        TrafficService::upsert_hourly(&db, "srv-1", hour, 100, 200)
            .await
            .unwrap();
        TrafficService::upsert_hourly(&db, "srv-1", hour, 50, 30)
            .await
            .unwrap();
        let row = traffic_hourly::Entity::find()
            .filter(traffic_hourly::Column::ServerId.eq("srv-1"))
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.bytes_in, 150);
        assert_eq!(row.bytes_out, 230);
    }

    #[tokio::test]
    async fn test_load_transfer_cache_from_traffic_state() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        TrafficService::upsert_state(&db, "srv-1", 1000, 2000)
            .await
            .unwrap();
        let cache = TrafficService::load_transfer_cache(&db).await.unwrap();
        assert_eq!(cache.get("srv-1"), Some(&(1000i64, 2000i64)));
    }

    #[tokio::test]
    async fn test_aggregate_daily_timezone_bucketing() {
        use crate::entity::traffic_daily;
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        // For Asia/Shanghai (UTC+8): Mar 17 local = Mar 16 16:00 UTC to Mar 17 15:59 UTC
        let h1 = NaiveDate::from_ymd_opt(2026, 3, 16)
            .unwrap()
            .and_hms_opt(20, 0, 0)
            .unwrap()
            .and_utc(); // Mar 17 04:00 CST
        let h2 = NaiveDate::from_ymd_opt(2026, 3, 17)
            .unwrap()
            .and_hms_opt(2, 0, 0)
            .unwrap()
            .and_utc(); // Mar 17 10:00 CST
        TrafficService::upsert_hourly(&db, "srv-1", h1, 100, 200)
            .await
            .unwrap();
        TrafficService::upsert_hourly(&db, "srv-1", h2, 300, 400)
            .await
            .unwrap();

        TrafficService::aggregate_daily_for_date(
            &db,
            NaiveDate::from_ymd_opt(2026, 3, 17).unwrap(),
            &chrono_tz::Asia::Shanghai,
        )
        .await
        .unwrap();

        let daily = traffic_daily::Entity::find()
            .filter(traffic_daily::Column::ServerId.eq("srv-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(daily.len(), 1);
        assert_eq!(daily[0].bytes_in, 400);
        assert_eq!(daily[0].bytes_out, 600);
    }

    #[tokio::test]
    async fn test_overview_empty() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        // No servers at all → overview returns empty vec
        let result = TrafficService::overview(&db).await.unwrap();
        assert!(
            result.is_empty(),
            "overview with no servers should return empty vec"
        );
    }

    #[tokio::test]
    async fn test_cycle_history_no_billing_cycle() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-no-cycle").await;
        // Server exists but has no billing_cycle set (default None) →
        // overview skips it, and cycle_history with explicit params returns data
        // but overview should not include it
        let overview = TrafficService::overview(&db).await.unwrap();
        assert!(
            !overview.iter().any(|o| o.server_id == "srv-no-cycle"),
            "server without billing_cycle should not appear in overview"
        );

        // cycle_history with explicit params should still work but return zero traffic
        let history = TrafficService::cycle_history(&db, "srv-no-cycle", "monthly", None, 3)
            .await
            .unwrap();
        assert_eq!(
            history.len(),
            3,
            "cycle_history should return requested count"
        );
        for cycle in &history {
            assert_eq!(cycle.bytes_in, 0, "empty server should have 0 bytes_in");
            assert_eq!(cycle.bytes_out, 0, "empty server should have 0 bytes_out");
        }
    }

    #[test]
    fn test_server_traffic_overview_serialization() {
        let overview = ServerTrafficOverview {
            server_id: "srv-1".to_string(),
            name: "Test Server".to_string(),
            cycle_in: 1_000_000_000,
            cycle_out: 500_000_000,
            traffic_limit: Some(10_000_000_000),
            billing_cycle: Some("monthly".to_string()),
            percent_used: Some(15.0),
            days_remaining: 20,
        };
        let json = serde_json::to_string(&overview).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["server_id"], "srv-1");
        assert_eq!(parsed["name"], "Test Server");
        assert_eq!(parsed["cycle_in"], 1_000_000_000_i64);
        assert_eq!(parsed["cycle_out"], 500_000_000_i64);
        assert_eq!(parsed["traffic_limit"], 10_000_000_000_i64);
        assert_eq!(parsed["billing_cycle"], "monthly");
        assert_eq!(parsed["percent_used"], 15.0);
        assert_eq!(parsed["days_remaining"], 20);
    }

    #[test]
    fn test_cycle_traffic_serialization() {
        let cycle = CycleTraffic {
            period: "2026-03-01 ~ 2026-03-31".to_string(),
            start: "2026-03-01".to_string(),
            end: "2026-03-31".to_string(),
            bytes_in: 2_000_000_000,
            bytes_out: 1_000_000_000,
        };
        let json = serde_json::to_string(&cycle).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["period"], "2026-03-01 ~ 2026-03-31");
        assert_eq!(parsed["start"], "2026-03-01");
        assert_eq!(parsed["end"], "2026-03-31");
        assert_eq!(parsed["bytes_in"], 2_000_000_000_i64);
        assert_eq!(parsed["bytes_out"], 1_000_000_000_i64);
    }

    // --- Helpers for seeding daily/state rows directly ---

    async fn insert_server_with_billing(
        db: &DatabaseConnection,
        id: &str,
        billing_cycle: Option<&str>,
        billing_start_day: Option<i32>,
        traffic_limit: Option<i64>,
    ) {
        use crate::entity::server;
        use serverbee_common::constants::CAP_DEFAULT;
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            name: Set("Billed Server".to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            billing_cycle: Set(billing_cycle.map(|s| s.to_string())),
            billing_start_day: Set(billing_start_day),
            traffic_limit: Set(traffic_limit),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert billed server");
    }

    async fn insert_daily(
        db: &DatabaseConnection,
        server_id: &str,
        date: NaiveDate,
        bytes_in: i64,
        bytes_out: i64,
    ) {
        use crate::entity::traffic_daily;
        traffic_daily::ActiveModel {
            server_id: Set(server_id.to_string()),
            date: Set(date),
            bytes_in: Set(bytes_in),
            bytes_out: Set(bytes_out),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert daily row");
    }

    // --- Billing cycle math: boundary & wrap-around cases ---

    #[test]
    fn test_cycle_range_anchor_clamped_above_28() {
        // billing_start_day above 28 is clamped to 28 to avoid invalid month days
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let (start, end) = get_cycle_range("monthly", Some(31), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 27).unwrap());
    }

    #[test]
    fn test_cycle_range_anchor_clamped_below_1() {
        // billing_start_day below 1 is clamped to 1 (natural-month path)
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
        let (start, end) = get_cycle_range("monthly", Some(0), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }

    #[test]
    fn test_cycle_range_monthly_december_wraps_year() {
        // Natural-month December must roll the end into the next year
        let today = NaiveDate::from_ymd_opt(2026, 12, 10).unwrap();
        let (start, end) = get_cycle_range("monthly", None, today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }

    #[test]
    fn test_cycle_range_monthly_anchor_crosses_year() {
        // anchor=15 in January, today before anchor -> previous cycle starts in December
        let today = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let (start, end) = get_cycle_range("monthly", Some(15), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2025, 12, 15).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 1, 14).unwrap());
    }

    #[test]
    fn test_cycle_range_monthly_feb_anchor_28_to_march() {
        // anchor=28 in February maps end to the day before next 28th (March 27)
        let today = NaiveDate::from_ymd_opt(2026, 2, 28).unwrap();
        let (start, end) = get_cycle_range("monthly", Some(28), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 27).unwrap());
    }

    #[test]
    fn test_cycle_range_quarterly_first_quarter() {
        // Date in Q1 with anchor=1 -> Jan 1 to Mar 31
        let today = NaiveDate::from_ymd_opt(2026, 2, 14).unwrap();
        let (start, end) = get_cycle_range("quarterly", Some(1), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }

    #[test]
    fn test_cycle_range_quarterly_last_quarter_wraps_year() {
        // Q4 quarter (Oct start) must end Dec 31 of the same year
        let today = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let (start, end) = get_cycle_range("quarterly", Some(1), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }

    #[test]
    fn test_cycle_range_quarterly_before_first_anchor_falls_to_prev_year() {
        // anchor=15, today=Jan 10 is before any quarter anchor this year ->
        // previous-year Q4 fallback (Oct 15 of prior year)
        let today = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let (start, end) = get_cycle_range("quarterly", Some(15), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2025, 10, 15).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 1, 14).unwrap());
    }

    #[test]
    fn test_cycle_range_yearly_before_anchor_uses_previous_year() {
        // anchor day 15; Jan 10 is before the Jan 15 anchor -> previous-year cycle
        let today = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let (start, end) = get_cycle_range("yearly", Some(15), today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2025, 1, 15).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 1, 14).unwrap());
    }

    // --- Prediction edge cases ---

    #[test]
    fn test_prediction_zero_limit_returns_none() {
        // traffic_limit of 0 must be rejected to avoid divide-by-zero
        let p = compute_prediction(60_000, 7, 10, Some(0), "sum");
        assert!(p.is_none());
    }

    #[test]
    fn test_prediction_will_not_exceed() {
        // Low usage projection should not flag will_exceed
        let p = compute_prediction(1_000, 5, 25, Some(1_000_000), "sum").unwrap();
        assert!(!p.will_exceed);
        assert!(p.estimated_total < 1_000_000);
        // estimated = 1000 + (1000/5)*25 = 1000 + 5000 = 6000
        assert_eq!(p.estimated_total, 6_000);
        assert!((p.estimated_percent - 0.6).abs() < 1e-9);
    }

    // --- query_cycle_traffic: daily + today's un-aggregated hourly ---

    #[tokio::test]
    async fn test_query_cycle_traffic_combines_daily_and_today_hourly() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let today = Utc::now().naive_utc().date();
        let yesterday = today - Duration::days(1);
        // A prior day already aggregated into daily
        insert_daily(&db, "srv-1", yesterday, 1_000, 2_000).await;
        // Today's hourly that has NOT been aggregated into daily yet
        let hour = today.and_hms_opt(10, 0, 0).unwrap().and_utc();
        TrafficService::upsert_hourly(&db, "srv-1", hour, 300, 400)
            .await
            .unwrap();

        let (total_in, total_out) =
            TrafficService::query_cycle_traffic(&db, "srv-1", yesterday, today)
                .await
                .unwrap();
        // daily(1000,2000) + today's hourly(300,400)
        assert_eq!(total_in, 1_300);
        assert_eq!(total_out, 2_400);
    }

    #[tokio::test]
    async fn test_query_cycle_traffic_excludes_today_hourly_when_already_daily() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let today = Utc::now().naive_utc().date();
        // Today already has a daily row, so the hourly NOT-EXISTS guard skips hourly
        insert_daily(&db, "srv-1", today, 5_000, 6_000).await;
        let hour = today.and_hms_opt(11, 0, 0).unwrap().and_utc();
        TrafficService::upsert_hourly(&db, "srv-1", hour, 999, 999)
            .await
            .unwrap();

        let (total_in, total_out) = TrafficService::query_cycle_traffic(&db, "srv-1", today, today)
            .await
            .unwrap();
        // Only the daily value counts; hourly is excluded because a daily row exists
        assert_eq!(total_in, 5_000);
        assert_eq!(total_out, 6_000);
    }

    #[tokio::test]
    async fn test_query_cycle_traffic_empty_returns_zero() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let d = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        // No data at all -> COALESCE yields zeros
        let (total_in, total_out) = TrafficService::query_cycle_traffic(&db, "srv-1", d, d)
            .await
            .unwrap();
        assert_eq!(total_in, 0);
        assert_eq!(total_out, 0);
    }

    // --- query_daily_breakdown ---

    #[tokio::test]
    async fn test_query_daily_breakdown_ordered_and_filtered() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let d1 = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        let d3 = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        // Insert out of order to verify ORDER BY date
        insert_daily(&db, "srv-1", d2, 20, 21).await;
        insert_daily(&db, "srv-1", d1, 10, 11).await;
        insert_daily(&db, "srv-1", d3, 30, 31).await;

        let rows = TrafficService::query_daily_breakdown(&db, "srv-1", d1, d2)
            .await
            .unwrap();
        // d3 is outside [d1,d2]; result must be ordered ascending
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].date, "2026-03-01");
        assert_eq!(rows[0].bytes_in, 10);
        assert_eq!(rows[1].date, "2026-03-02");
        assert_eq!(rows[1].bytes_out, 21);
    }

    #[tokio::test]
    async fn test_query_daily_breakdown_empty() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let d = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        // No rows -> empty vec
        let rows = TrafficService::query_daily_breakdown(&db, "srv-1", d, d)
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    // --- query_hourly_breakdown ---

    #[tokio::test]
    async fn test_query_hourly_breakdown_filters_to_date() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let date = NaiveDate::from_ymd_opt(2026, 3, 17).unwrap();
        let in_day = date.and_hms_opt(8, 0, 0).unwrap().and_utc();
        let next_day = NaiveDate::from_ymd_opt(2026, 3, 18)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap()
            .and_utc();
        TrafficService::upsert_hourly(&db, "srv-1", in_day, 100, 200)
            .await
            .unwrap();
        TrafficService::upsert_hourly(&db, "srv-1", next_day, 999, 999)
            .await
            .unwrap();

        let rows = TrafficService::query_hourly_breakdown(&db, "srv-1", date)
            .await
            .unwrap();
        // Only the in-day row is returned (next_day is outside the [00:00,23:59:59] window)
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].bytes_in, 100);
        assert_eq!(rows[0].bytes_out, 200);
    }

    #[tokio::test]
    async fn test_query_hourly_breakdown_empty() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        // No hourly rows -> empty vec
        let rows = TrafficService::query_hourly_breakdown(
            &db,
            "srv-1",
            NaiveDate::from_ymd_opt(2026, 3, 17).unwrap(),
        )
        .await
        .unwrap();
        assert!(rows.is_empty());
    }

    // --- overview with configured data (percent_used + days_remaining) ---

    #[tokio::test]
    async fn test_overview_with_billing_and_limit() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let today = Utc::now().date_naive();
        // Configure a monthly natural-month cycle with a 1000-byte limit
        insert_server_with_billing(&db, "srv-bill", Some("monthly"), Some(1), Some(1_000)).await;
        // Seed a daily row inside the current natural-month cycle (cycle start = 1st)
        let cycle_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
        insert_daily(&db, "srv-bill", cycle_start, 200, 300).await;

        let result = TrafficService::overview(&db).await.unwrap();
        let entry = result
            .iter()
            .find(|o| o.server_id == "srv-bill")
            .expect("server with billing cycle should be present");
        assert_eq!(entry.name, "Billed Server");
        assert_eq!(entry.cycle_in, 200);
        assert_eq!(entry.cycle_out, 300);
        assert_eq!(entry.traffic_limit, Some(1_000));
        // percent_used = (200 + 300) / 1000 * 100 = 50.0
        assert!((entry.percent_used.unwrap() - 50.0).abs() < 1e-9);
        // days_remaining should be within the current month and non-negative
        assert!(entry.days_remaining >= 0);
    }

    #[tokio::test]
    async fn test_overview_no_limit_yields_none_percent() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let today = Utc::now().date_naive();
        // Billing cycle set but no traffic_limit -> percent_used is None
        insert_server_with_billing(&db, "srv-nolimit", Some("monthly"), Some(1), None).await;
        let cycle_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
        insert_daily(&db, "srv-nolimit", cycle_start, 50, 60).await;

        let result = TrafficService::overview(&db).await.unwrap();
        let entry = result
            .iter()
            .find(|o| o.server_id == "srv-nolimit")
            .expect("server present");
        assert!(entry.percent_used.is_none());
        assert_eq!(entry.cycle_in, 50);
        assert_eq!(entry.cycle_out, 60);
    }

    #[tokio::test]
    async fn test_overview_zero_limit_yields_none_percent() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        // traffic_limit of 0 must yield None percent (the limit>0 guard)
        insert_server_with_billing(&db, "srv-zero", Some("monthly"), Some(1), Some(0)).await;
        let result = TrafficService::overview(&db).await.unwrap();
        let entry = result
            .iter()
            .find(|o| o.server_id == "srv-zero")
            .expect("server present");
        assert!(entry.percent_used.is_none());
    }

    #[tokio::test]
    async fn test_overview_skips_empty_billing_cycle_string() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        // An empty (non-None) billing_cycle string is treated as "not configured"
        insert_server_with_billing(&db, "srv-empty-bc", Some(""), Some(1), Some(1_000)).await;
        let result = TrafficService::overview(&db).await.unwrap();
        assert!(
            !result.iter().any(|o| o.server_id == "srv-empty-bc"),
            "empty billing_cycle string should be skipped"
        );
    }

    // --- overview_daily: global aggregation across servers ---

    #[tokio::test]
    async fn test_overview_daily_aggregates_across_servers() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-a").await;
        insert_server_with_billing(&db, "srv-b", None, None, None).await;
        let today = Utc::now().date_naive();
        let d = today - Duration::days(1);
        // Two servers contribute to the same date -> summed in overview_daily
        insert_daily(&db, "srv-a", d, 100, 200).await;
        insert_daily(&db, "srv-b", d, 50, 25).await;

        let rows = TrafficService::overview_daily(&db, 7).await.unwrap();
        let entry = rows
            .iter()
            .find(|r| r.date == d.format("%Y-%m-%d").to_string())
            .expect("date present");
        assert_eq!(entry.bytes_in, 150);
        assert_eq!(entry.bytes_out, 225);
    }

    #[tokio::test]
    async fn test_overview_daily_respects_cutoff() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-a").await;
        let today = Utc::now().date_naive();
        let recent = today - Duration::days(2);
        let old = today - Duration::days(40);
        insert_daily(&db, "srv-a", recent, 10, 11).await;
        insert_daily(&db, "srv-a", old, 9_999, 9_999).await;

        // Only the last 7 days are returned; the 40-day-old row is excluded
        let rows = TrafficService::overview_daily(&db, 7).await.unwrap();
        assert!(rows.iter().all(|r| r.date != old.format("%Y-%m-%d").to_string()));
        assert!(rows.iter().any(|r| r.date == recent.format("%Y-%m-%d").to_string()));
    }

    #[tokio::test]
    async fn test_overview_daily_empty() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        // No daily data -> empty vec
        let rows = TrafficService::overview_daily(&db, 30).await.unwrap();
        assert!(rows.is_empty());
    }

    // --- cycle_history with a configured cycle and real data ---

    #[tokio::test]
    async fn test_cycle_history_iterates_backwards_with_data() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-h").await;
        let today = Utc::now().date_naive();
        // Current natural-month cycle starts on the 1st of this month
        let cur_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
        // Previous cycle = the previous month
        let prev_start = cur_start - Duration::days(1); // last day of previous month
        let prev_cycle_start =
            NaiveDate::from_ymd_opt(prev_start.year(), prev_start.month(), 1).unwrap();
        // Seed data in the current cycle and the previous cycle
        insert_daily(&db, "srv-h", cur_start, 100, 200).await;
        insert_daily(&db, "srv-h", prev_cycle_start, 300, 400).await;

        let history = TrafficService::cycle_history(&db, "srv-h", "monthly", Some(1), 2)
            .await
            .unwrap();
        assert_eq!(history.len(), 2);
        // First entry = current cycle
        assert_eq!(history[0].start, cur_start.format("%Y-%m-%d").to_string());
        assert_eq!(history[0].bytes_in, 100);
        assert_eq!(history[0].bytes_out, 200);
        // Second entry = previous cycle (iterating backwards)
        assert_eq!(
            history[1].start,
            prev_cycle_start.format("%Y-%m-%d").to_string()
        );
        assert_eq!(history[1].bytes_in, 300);
        assert_eq!(history[1].bytes_out, 400);
        // Period string formatting "<start> ~ <end>"
        assert!(history[0].period.contains(" ~ "));
    }

    #[tokio::test]
    async fn test_cycle_history_zero_count_returns_empty() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-h").await;
        // count = 0 -> loop body never runs
        let history = TrafficService::cycle_history(&db, "srv-h", "monthly", Some(1), 0)
            .await
            .unwrap();
        assert!(history.is_empty());
    }

    // --- load_transfer_cache: multiple rows and empty ---

    #[tokio::test]
    async fn test_load_transfer_cache_multiple_servers() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        insert_server_with_billing(&db, "srv-2", None, None, None).await;
        TrafficService::upsert_state(&db, "srv-1", 10, 20).await.unwrap();
        TrafficService::upsert_state(&db, "srv-2", 30, 40).await.unwrap();

        let cache = TrafficService::load_transfer_cache(&db).await.unwrap();
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("srv-1"), Some(&(10i64, 20i64)));
        assert_eq!(cache.get("srv-2"), Some(&(30i64, 40i64)));
    }

    #[tokio::test]
    async fn test_load_transfer_cache_empty() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        // No traffic_state rows -> empty cache
        let cache = TrafficService::load_transfer_cache(&db).await.unwrap();
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_upsert_state_updates_existing_row() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        // Second upsert on the same server_id replaces (not accumulates) values
        TrafficService::upsert_state(&db, "srv-1", 100, 200).await.unwrap();
        TrafficService::upsert_state(&db, "srv-1", 500, 600).await.unwrap();
        let cache = TrafficService::load_transfer_cache(&db).await.unwrap();
        assert_eq!(cache.get("srv-1"), Some(&(500i64, 600i64)));
    }

    // --- aggregate_daily: full method (yesterday + today buckets) ---

    #[tokio::test]
    async fn test_aggregate_daily_full_method_processes_both_days() {
        use crate::entity::traffic_daily;
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        // Use UTC so local-date boundaries are unambiguous
        let now = Utc::now();
        let today = now.date_naive();
        let yesterday = today - Duration::days(1);
        // One hourly row for today, one for yesterday (both well inside the day in UTC)
        let h_today = today.and_hms_opt(12, 0, 0).unwrap().and_utc();
        let h_yesterday = yesterday.and_hms_opt(12, 0, 0).unwrap().and_utc();
        TrafficService::upsert_hourly(&db, "srv-1", h_today, 100, 200)
            .await
            .unwrap();
        TrafficService::upsert_hourly(&db, "srv-1", h_yesterday, 300, 400)
            .await
            .unwrap();

        let affected = TrafficService::aggregate_daily(&db, "UTC").await.unwrap();
        // Both day-buckets aggregated one server group each
        assert_eq!(affected, 2);

        let daily = traffic_daily::Entity::find().all(&db).await.unwrap();
        assert_eq!(daily.len(), 2);
        let today_row = daily.iter().find(|d| d.date == today).unwrap();
        assert_eq!(today_row.bytes_in, 100);
        assert_eq!(today_row.bytes_out, 200);
        let yesterday_row = daily.iter().find(|d| d.date == yesterday).unwrap();
        assert_eq!(yesterday_row.bytes_in, 300);
        assert_eq!(yesterday_row.bytes_out, 400);
    }

    #[tokio::test]
    async fn test_aggregate_daily_invalid_timezone_errors() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        // An unparseable timezone string must surface an error (not panic)
        let err = TrafficService::aggregate_daily(&db, "Not/AZone")
            .await
            .err()
            .expect("invalid timezone should error");
        let msg = format!("{err}");
        assert!(msg.contains("Invalid timezone"), "got: {msg}");
    }

    #[tokio::test]
    async fn test_aggregate_daily_for_date_upsert_overwrites() {
        use crate::entity::traffic_daily;
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let date = NaiveDate::from_ymd_opt(2026, 3, 17).unwrap();
        let h = date.and_hms_opt(12, 0, 0).unwrap().and_utc();
        TrafficService::upsert_hourly(&db, "srv-1", h, 100, 200)
            .await
            .unwrap();
        // First aggregation
        TrafficService::aggregate_daily_for_date(&db, date, &chrono_tz::UTC)
            .await
            .unwrap();
        // Add more hourly, then re-aggregate: ON CONFLICT replaces with the new SUM
        let h2 = date.and_hms_opt(13, 0, 0).unwrap().and_utc();
        TrafficService::upsert_hourly(&db, "srv-1", h2, 50, 60)
            .await
            .unwrap();
        TrafficService::aggregate_daily_for_date(&db, date, &chrono_tz::UTC)
            .await
            .unwrap();

        let rows = traffic_daily::Entity::find().all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        // Overwritten total = 100+50 / 200+60
        assert_eq!(rows[0].bytes_in, 150);
        assert_eq!(rows[0].bytes_out, 260);
    }

    // --- cleanup helpers ---

    #[tokio::test]
    async fn test_cleanup_hourly_removes_old_rows() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let old = (Utc::now() - Duration::days(40))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let recent = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
        TrafficService::upsert_hourly(&db, "srv-1", old, 1, 1).await.unwrap();
        TrafficService::upsert_hourly(&db, "srv-1", recent, 2, 2).await.unwrap();

        // Cleanup older than 30 days -> only the 40-day-old row is deleted
        let deleted = TrafficService::cleanup_hourly(&db, 30).await.unwrap();
        assert_eq!(deleted, 1);
        let remaining = traffic_hourly::Entity::find().all(&db).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].bytes_in, 2);
    }

    #[tokio::test]
    async fn test_cleanup_daily_removes_old_rows() {
        use crate::entity::traffic_daily;
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        let old = (Utc::now() - Duration::days(400)).date_naive();
        let recent = Utc::now().date_naive();
        insert_daily(&db, "srv-1", old, 1, 1).await;
        insert_daily(&db, "srv-1", recent, 2, 2).await;

        // Cleanup older than 365 days -> only the very old row is deleted
        let deleted = TrafficService::cleanup_daily(&db, 365).await.unwrap();
        assert_eq!(deleted, 1);
        let remaining = traffic_daily::Entity::find().all(&db).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].date, recent);
    }

    #[tokio::test]
    async fn test_cleanup_task_results_removes_old_rows() {
        use crate::entity::task_result;
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let old_finished = Utc::now() - Duration::days(100);
        let recent_finished = Utc::now();
        // Two task_results: one old, one recent (no FK on this table)
        task_result::ActiveModel {
            task_id: Set("t-1".to_string()),
            server_id: Set("srv-1".to_string()),
            output: Set("old".to_string()),
            exit_code: Set(0),
            attempt: Set(1),
            finished_at: Set(old_finished),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        task_result::ActiveModel {
            task_id: Set("t-2".to_string()),
            server_id: Set("srv-1".to_string()),
            output: Set("recent".to_string()),
            exit_code: Set(0),
            attempt: Set(1),
            finished_at: Set(recent_finished),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        // Cleanup older than 30 days -> deletes only the 100-day-old result
        let deleted = TrafficService::cleanup_task_results(&db, 30).await.unwrap();
        assert_eq!(deleted, 1);
        let remaining = task_result::Entity::find().all(&db).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].output, "recent");
    }

    #[tokio::test]
    async fn test_cleanup_returns_zero_when_nothing_to_delete() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "srv-1").await;
        // Empty tables -> all cleanups report 0 rows affected
        assert_eq!(TrafficService::cleanup_hourly(&db, 30).await.unwrap(), 0);
        assert_eq!(TrafficService::cleanup_daily(&db, 365).await.unwrap(), 0);
        assert_eq!(
            TrafficService::cleanup_task_results(&db, 30).await.unwrap(),
            0
        );
    }

    // --- merge_recovered_server_history ---

    #[tokio::test]
    async fn test_merge_recovered_server_history_moves_and_dedups() {
        use crate::entity::{traffic_daily, traffic_state};
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "target").await;
        insert_server_with_billing(&db, "source", None, None, None).await;

        let date_a = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let date_b = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        // Target already has a daily row on date_a; source has date_a (collides) + date_b
        insert_daily(&db, "target", date_a, 10, 10).await;
        insert_daily(&db, "source", date_a, 99, 99).await;
        insert_daily(&db, "source", date_b, 20, 20).await;
        // Hourly rows for source
        let h = date_a.and_hms_opt(1, 0, 0).unwrap().and_utc();
        TrafficService::upsert_hourly(&db, "source", h, 5, 5).await.unwrap();
        // State rows: both target and source exist -> target's must survive the dedup delete
        TrafficService::upsert_state(&db, "target", 1, 1).await.unwrap();
        TrafficService::upsert_state(&db, "source", 2, 2).await.unwrap();

        TrafficService::merge_recovered_server_history(&db, "target", "source")
            .await
            .unwrap();

        // Daily: target keeps its own date_a (collision was deleted from target before re-key),
        // and gains date_b from source. date_a should now exist once for target.
        let target_daily = traffic_daily::Entity::find()
            .filter(traffic_daily::Column::ServerId.eq("target"))
            .all(&db)
            .await
            .unwrap();
        // date_a (re-keyed from source after target's colliding row was deleted) + date_b
        let dates: Vec<NaiveDate> = target_daily.iter().map(|r| r.date).collect();
        assert!(dates.contains(&date_a));
        assert!(dates.contains(&date_b));
        // No daily rows remain under "source"
        let source_daily = traffic_daily::Entity::find()
            .filter(traffic_daily::Column::ServerId.eq("source"))
            .all(&db)
            .await
            .unwrap();
        assert!(source_daily.is_empty());

        // Hourly moved to target
        let target_hourly = traffic_hourly::Entity::find()
            .filter(traffic_hourly::Column::ServerId.eq("target"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(target_hourly.len(), 1);

        // State: source's state row removed (empty key columns -> 1=1 predicate deletes
        // target's, then source's is re-keyed to target). Only one target state row remains.
        let target_state = traffic_state::Entity::find()
            .filter(traffic_state::Column::ServerId.eq("target"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(target_state.len(), 1);
        // With empty key columns the source state overwrites target -> last_in/last_out from source
        assert_eq!(target_state[0].last_in, 2);
        assert_eq!(target_state[0].last_out, 2);
        let source_state = traffic_state::Entity::find()
            .filter(traffic_state::Column::ServerId.eq("source"))
            .all(&db)
            .await
            .unwrap();
        assert!(source_state.is_empty());
    }

    #[tokio::test]
    async fn test_replace_unique_key_table_no_collision_moves_all() {
        use crate::entity::traffic_daily;
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(&db, "target").await;
        insert_server_with_billing(&db, "source", None, None, None).await;
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        // Only source has data; target has none -> all source rows simply re-key to target
        insert_daily(&db, "source", d, 7, 8).await;

        TrafficService::replace_unique_key_table_server_id_on_connection(
            &db,
            "traffic_daily",
            &["date"],
            "target",
            "source",
        )
        .await
        .unwrap();

        let target_daily = traffic_daily::Entity::find()
            .filter(traffic_daily::Column::ServerId.eq("target"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(target_daily.len(), 1);
        assert_eq!(target_daily[0].bytes_in, 7);
        assert_eq!(target_daily[0].bytes_out, 8);
    }
}
