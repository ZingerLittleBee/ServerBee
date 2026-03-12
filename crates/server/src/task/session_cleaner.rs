use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sea_orm::*;

use crate::entity::session;
use crate::state::AppState;

/// Periodically deletes expired sessions and cleans up in-memory caches (every 3600 seconds).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;

        let now = Utc::now();

        // Clean expired sessions from database
        match session::Entity::delete_many()
            .filter(session::Column::ExpiresAt.lt(now))
            .exec(&state.db)
            .await
        {
            Ok(result) if result.rows_affected > 0 => {
                tracing::info!(
                    "Cleaned up {} expired sessions",
                    result.rows_affected
                );
            }
            Err(e) => tracing::error!("Failed to clean up expired sessions: {e}"),
            _ => {}
        }

        // Clean expired pending TOTP secrets (older than 10 minutes)
        let totp_cutoff = now - chrono::Duration::minutes(10);
        state
            .pending_totp
            .retain(|_, v| v.created_at > totp_cutoff);

        // Clean expired login rate limit entries (older than 15 minutes)
        let rate_cutoff = now - chrono::Duration::minutes(15);
        state
            .login_rate_limit
            .retain(|_, v| v.window_start > rate_cutoff);
    }
}
