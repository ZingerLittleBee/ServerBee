use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sea_orm::*;

use crate::entity::session;
use crate::state::AppState;

/// Periodically deletes expired sessions from the database (every 3600 seconds).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;

        let now = Utc::now();
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
    }
}
