use chrono::Utc;
use sea_orm::*;

use crate::entity::uptime_daily;
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
}
