use chrono::Utc;
use sea_orm::*;

use crate::entity::{server, uptime_daily};
use crate::error::AppError;

pub struct UptimeService;

impl UptimeService {
    /// Get daily uptime records for a server over the last N days.
    pub async fn get_daily(
        db: &DatabaseConnection,
        server_id: &str,
        days: u32,
    ) -> Result<Vec<uptime_daily::Model>, AppError> {
        let cutoff = Utc::now().date_naive() - chrono::Duration::days(days as i64);

        Ok(uptime_daily::Entity::find()
            .filter(uptime_daily::Column::ServerId.eq(server_id))
            .filter(uptime_daily::Column::Date.gte(cutoff))
            .order_by_asc(uptime_daily::Column::Date)
            .all(db)
            .await?)
    }

    /// Calculate availability percentage for a server over the last N days.
    /// Returns 100.0 if no records exist (assume online).
    pub async fn get_availability(
        db: &DatabaseConnection,
        server_id: &str,
        days: u32,
    ) -> Result<f64, AppError> {
        let records = Self::get_daily(db, server_id, days).await?;

        if records.is_empty() {
            return Ok(100.0);
        }

        let total_minutes: i64 = records.iter().map(|r| r.total_minutes as i64).sum();
        let online_minutes: i64 = records.iter().map(|r| r.online_minutes as i64).sum();

        if total_minutes == 0 {
            return Ok(100.0);
        }

        Ok((online_minutes as f64 / total_minutes as f64) * 100.0)
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
