use std::collections::HashMap;

use chrono::{NaiveDate, SecondsFormat, Utc};
use sea_orm::*;
use serde::Serialize;

use crate::entity::{server, uptime_daily};
use crate::error::AppError;

/// A single day's uptime data, with gap-filling for missing dates.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UptimeDailyEntry {
    pub date: NaiveDate,
    pub total_minutes: i32,
    pub online_minutes: i32,
    pub downtime_incidents: i32,
}

pub struct UptimeService;

impl UptimeService {
    /// Get daily uptime records for a server, returning exactly `days` entries.
    ///
    /// Date range: `[today - (days-1), today]` (inclusive).
    /// Missing dates are gap-filled with zeros. Results are ordered date-ascending.
    pub async fn get_daily_filled(
        db: &DatabaseConnection,
        server_id: &str,
        days: u32,
    ) -> Result<Vec<UptimeDailyEntry>, AppError> {
        let today = Utc::now().date_naive();
        let start_date = today - chrono::Duration::days((days as i64) - 1);

        // Fetch existing records in the date range
        let records = uptime_daily::Entity::find()
            .filter(uptime_daily::Column::ServerId.eq(server_id))
            .filter(uptime_daily::Column::Date.gte(start_date))
            .filter(uptime_daily::Column::Date.lte(today))
            .order_by_asc(uptime_daily::Column::Date)
            .all(db)
            .await?;

        // Build a lookup map: date -> record
        let mut record_map: std::collections::HashMap<NaiveDate, &uptime_daily::Model> =
            std::collections::HashMap::new();
        for r in &records {
            record_map.insert(r.date, r);
        }

        // Generate exactly `days` entries, gap-filling with zeros
        let mut result = Vec::with_capacity(days as usize);
        let mut current = start_date;
        while current <= today {
            let entry = if let Some(r) = record_map.get(&current) {
                UptimeDailyEntry {
                    date: current,
                    total_minutes: r.total_minutes,
                    online_minutes: r.online_minutes,
                    downtime_incidents: r.downtime_incidents,
                }
            } else {
                UptimeDailyEntry {
                    date: current,
                    total_minutes: 0,
                    online_minutes: 0,
                    downtime_incidents: 0,
                }
            };
            result.push(entry);
            current += chrono::Duration::days(1);
        }

        Ok(result)
    }

    pub async fn get_daily_filled_batch(
        db: &DatabaseConnection,
        server_ids: &[String],
        days: u32,
    ) -> Result<HashMap<String, Vec<UptimeDailyEntry>>, AppError> {
        if server_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let today = Utc::now().date_naive();
        let start_date = today - chrono::Duration::days((days as i64) - 1);

        let records = uptime_daily::Entity::find()
            .filter(uptime_daily::Column::ServerId.is_in(server_ids.iter().cloned()))
            .filter(uptime_daily::Column::Date.gte(start_date))
            .filter(uptime_daily::Column::Date.lte(today))
            .order_by_asc(uptime_daily::Column::Date)
            .all(db)
            .await?;

        let mut records_by_server: HashMap<&str, HashMap<NaiveDate, &uptime_daily::Model>> =
            HashMap::new();
        for r in &records {
            records_by_server
                .entry(r.server_id.as_str())
                .or_default()
                .insert(r.date, r);
        }

        let mut out = HashMap::with_capacity(server_ids.len());
        for server_id in server_ids {
            let record_map = records_by_server.get(server_id.as_str());
            let mut entries = Vec::with_capacity(days as usize);
            let mut current = start_date;
            while current <= today {
                let entry = if let Some(r) = record_map.and_then(|m| m.get(&current)) {
                    UptimeDailyEntry {
                        date: current,
                        total_minutes: r.total_minutes,
                        online_minutes: r.online_minutes,
                        downtime_incidents: r.downtime_incidents,
                    }
                } else {
                    UptimeDailyEntry {
                        date: current,
                        total_minutes: 0,
                        online_minutes: 0,
                        downtime_incidents: 0,
                    }
                };
                entries.push(entry);
                current += chrono::Duration::days(1);
            }
            out.insert(server_id.clone(), entries);
        }

        Ok(out)
    }

    pub async fn aggregate_daily(db: &DatabaseConnection) -> Result<u32, AppError> {
        let now = Utc::now();
        let today = now.date_naive();
        let day_start = today
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339_opts(SecondsFormat::AutoSi, false);
        let day_end = now.to_rfc3339_opts(SecondsFormat::AutoSi, false);

        let total_minutes = {
            let elapsed = now - today.and_hms_opt(0, 0, 0).unwrap().and_utc();
            elapsed.num_minutes().max(1) as i32
        };

        let servers = server::Entity::find().all(db).await?;
        if servers.is_empty() {
            return Ok(0);
        }

        let rows = db
            .query_all(Statement::from_sql_and_values(
                db.get_database_backend(),
                "SELECT server_id, time FROM records WHERE time >= $1 AND time < $2 ORDER BY server_id, time",
                [day_start.into(), day_end.into()],
            ))
            .await?;

        let mut stats: HashMap<String, (i32, i32, chrono::DateTime<Utc>)> = HashMap::new();
        for row in &rows {
            let server_id: String = row
                .try_get_by_index(0)
                .map_err(|e| AppError::Internal(format!("Failed to read server_id: {e}")))?;
            let time: chrono::DateTime<Utc> = row
                .try_get_by_index(1)
                .map_err(|e| AppError::Internal(format!("Failed to read record time: {e}")))?;

            match stats.get_mut(&server_id) {
                Some((count, incidents, prev_time)) => {
                    *count += 1;
                    if (time - *prev_time).num_seconds() > 120 {
                        *incidents += 1;
                    }
                    *prev_time = time;
                }
                None => {
                    stats.insert(server_id, (1, 0, time));
                }
            }
        }

        let date_str = today.format("%Y-%m-%d").to_string();
        let txn = db.begin().await?;
        let mut upserted = 0u32;

        for srv in &servers {
            let (online_minutes, downtime_incidents) = stats
                .get(&srv.id)
                .map(|&(count, incidents, _)| (count, incidents))
                .unwrap_or((0, 0));

            txn.execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                "INSERT INTO uptime_daily (server_id, date, total_minutes, online_minutes, downtime_incidents) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT(server_id, date) DO UPDATE SET \
                     total_minutes = excluded.total_minutes, \
                     online_minutes = excluded.online_minutes, \
                     downtime_incidents = excluded.downtime_incidents",
                [
                    srv.id.clone().into(),
                    date_str.clone().into(),
                    total_minutes.into(),
                    online_minutes.into(),
                    downtime_incidents.into(),
                ],
            ))
            .await?;

            upserted += 1;
        }

        txn.commit().await?;
        Ok(upserted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;
    use sea_orm::ConnectionTrait;

    #[tokio::test]
    async fn test_get_daily_filled_empty_returns_zeros() {
        let (db, _tmp) = setup_test_db().await;

        let entries = UptimeService::get_daily_filled(&db, "nonexistent-server", 7)
            .await
            .unwrap();

        assert_eq!(entries.len(), 7, "Should return exactly 7 entries");
        for entry in &entries {
            assert_eq!(entry.total_minutes, 0);
            assert_eq!(entry.online_minutes, 0);
            assert_eq!(entry.downtime_incidents, 0);
        }

        // Verify date ordering: ascending
        for i in 1..entries.len() {
            assert!(
                entries[i].date > entries[i - 1].date,
                "Dates should be in ascending order"
            );
        }
    }

    #[tokio::test]
    async fn test_get_daily_filled_date_range() {
        let (db, _tmp) = setup_test_db().await;

        let today = Utc::now().date_naive();
        let entries = UptimeService::get_daily_filled(&db, "server-1", 3)
            .await
            .unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].date, today - chrono::Duration::days(2));
        assert_eq!(entries[1].date, today - chrono::Duration::days(1));
        assert_eq!(entries[2].date, today);
    }

    #[tokio::test]
    async fn test_get_daily_filled_with_data() {
        let (db, _tmp) = setup_test_db().await;

        let today = Utc::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);

        // Insert uptime data for yesterday only
        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            "INSERT INTO uptime_daily (server_id, date, total_minutes, online_minutes, downtime_incidents) VALUES ($1, $2, $3, $4, $5)",
            [
                "server-1".into(),
                yesterday.format("%Y-%m-%d").to_string().into(),
                1440i32.into(),
                1400i32.into(),
                2i32.into(),
            ],
        ))
        .await
        .unwrap();

        let entries = UptimeService::get_daily_filled(&db, "server-1", 3)
            .await
            .unwrap();

        assert_eq!(entries.len(), 3);

        // Day before yesterday: gap-filled with zeros
        assert_eq!(entries[0].total_minutes, 0);
        assert_eq!(entries[0].online_minutes, 0);

        // Yesterday: should have actual data
        assert_eq!(entries[1].date, yesterday);
        assert_eq!(entries[1].total_minutes, 1440);
        assert_eq!(entries[1].online_minutes, 1400);
        assert_eq!(entries[1].downtime_incidents, 2);

        // Today: gap-filled with zeros
        assert_eq!(entries[2].total_minutes, 0);
        assert_eq!(entries[2].online_minutes, 0);
    }

    #[tokio::test]
    async fn test_get_daily_filled_single_day() {
        let (db, _tmp) = setup_test_db().await;

        let today = Utc::now().date_naive();
        let entries = UptimeService::get_daily_filled(&db, "server-1", 1)
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, today);
    }

    #[tokio::test]
    async fn test_get_daily_filled_90_days() {
        let (db, _tmp) = setup_test_db().await;

        let entries = UptimeService::get_daily_filled(&db, "server-1", 90)
            .await
            .unwrap();

        assert_eq!(entries.len(), 90, "Should return exactly 90 entries");

        let today = Utc::now().date_naive();
        assert_eq!(entries[0].date, today - chrono::Duration::days(89));
        assert_eq!(entries[89].date, today);
    }

    #[tokio::test]
    async fn test_aggregate_daily_counts_rfc3339_records() {
        let (db, _tmp) = setup_test_db().await;

        let now = Utc::now();
        let server_id = "test-server-uptime";

        // Insert a server
        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            "INSERT INTO servers (id, token_hash, token_prefix, name, weight, hidden, capabilities, protocol_version, features, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, 0, 0, 0, 1, '[]', $5, $5)",
            [
                server_id.into(),
                "hash".into(),
                "prefix".into(),
                "Test Server".into(),
                now.to_rfc3339_opts(SecondsFormat::AutoSi, false).into(),
            ],
        ))
        .await
        .unwrap();

        // Insert 5 records with RFC3339 timestamps (as sea-orm stores DateTimeUtc)
        for i in 0..5 {
            let time = now - chrono::Duration::minutes(i * 2);
            let time_str = time.to_rfc3339_opts(SecondsFormat::AutoSi, false);
            db.execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                "INSERT INTO records (server_id, time, cpu, mem_used, swap_used, disk_used, \
                 net_in_speed, net_out_speed, net_in_transfer, net_out_transfer, \
                 load1, load5, load15, tcp_conn, udp_conn, process_count) \
                 VALUES ($1, $2, 0.5, 100, 0, 200, 0, 0, 0, 0, 0.1, 0.1, 0.1, 10, 5, 50)",
                [server_id.into(), time_str.into()],
            ))
            .await
            .unwrap();
        }

        // Run aggregation
        let count = UptimeService::aggregate_daily(&db).await.unwrap();
        assert_eq!(count, 1, "Should aggregate for 1 server");

        // Verify the uptime_daily record was created with correct online_minutes
        let entries = UptimeService::get_daily_filled(&db, server_id, 1)
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].online_minutes, 5,
            "Should count 5 records as 5 online minutes"
        );
        assert!(
            entries[0].total_minutes > 0,
            "total_minutes should be > 0 for today"
        );
    }
}
