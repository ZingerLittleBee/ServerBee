use chrono::{NaiveDate, Utc};
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

    /// Aggregate uptime data for the current day.
    /// Checks record existence in the `records` table to determine online minutes.
    /// Each record represents ~1 minute of online time (record_writer writes every 60s).
    /// Uses INSERT ... ON CONFLICT DO UPDATE (upsert) on (server_id, date).
    pub async fn aggregate_daily(db: &DatabaseConnection) -> Result<u32, AppError> {
        let now = Utc::now();
        let today = now.date_naive();
        let day_start = today
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let day_end = now.format("%Y-%m-%d %H:%M:%S").to_string();

        // Minutes elapsed in the current day so far
        let total_minutes = {
            let elapsed = now - today.and_hms_opt(0, 0, 0).unwrap().and_utc();
            elapsed.num_minutes().max(1) as i32
        };

        // Get all server IDs
        let servers = server::Entity::find().all(db).await?;
        if servers.is_empty() {
            return Ok(0);
        }

        let date_str = today.format("%Y-%m-%d").to_string();
        let mut upserted = 0u32;

        for srv in &servers {
            // Count records for this server today (each record = ~1 minute online)
            let count_result = db
                .query_one(Statement::from_sql_and_values(
                    db.get_database_backend(),
                    "SELECT COUNT(*) FROM records WHERE server_id = $1 AND time >= $2 AND time < $3",
                    [srv.id.clone().into(), day_start.clone().into(), day_end.clone().into()],
                ))
                .await?;

            let online_minutes: i32 = match count_result {
                Some(row) => row.try_get_by_index(0).unwrap_or(0),
                None => 0,
            };

            // Count downtime incidents: gaps > 2 minutes between consecutive records
            let downtime_incidents = Self::count_downtime_incidents(
                db,
                &srv.id,
                &day_start,
                &day_end,
            )
            .await?;

            // Upsert into uptime_daily
            db.execute(Statement::from_sql_and_values(
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

        Ok(upserted)
    }

    /// Count distinct downtime incidents for a server in a time range.
    /// A downtime incident is a gap of more than 2 minutes between consecutive records.
    async fn count_downtime_incidents(
        db: &DatabaseConnection,
        server_id: &str,
        start: &str,
        end: &str,
    ) -> Result<i32, AppError> {
        let rows = db
            .query_all(Statement::from_sql_and_values(
                db.get_database_backend(),
                "SELECT time FROM records WHERE server_id = $1 AND time >= $2 AND time < $3 ORDER BY time",
                [server_id.into(), start.into(), end.into()],
            ))
            .await?;

        if rows.len() < 2 {
            return Ok(0);
        }

        let mut incidents = 0i32;
        let mut prev_time: Option<chrono::DateTime<Utc>> = None;

        for row in &rows {
            let time: chrono::DateTime<Utc> = row.try_get_by_index(0).map_err(|e| {
                AppError::Internal(format!("Failed to read record time: {e}"))
            })?;

            if let Some(prev) = prev_time {
                let gap = (time - prev).num_seconds();
                if gap > 120 {
                    incidents += 1;
                }
            }

            prev_time = Some(time);
        }

        Ok(incidents)
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
}
