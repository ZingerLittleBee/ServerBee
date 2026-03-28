use std::sync::Arc;

use chrono::Utc;
use sea_orm::*;

use crate::entity::mobile_session;
use crate::state::AppState;

pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(86400));
    loop {
        interval.tick().await;
        let cutoff = Utc::now();
        // Delete expired sessions
        if let Err(e) = mobile_session::Entity::delete_many()
            .filter(mobile_session::Column::ExpiresAt.lt(cutoff))
            .exec(&state.db)
            .await
        {
            tracing::error!("Mobile session cleanup error: {e}");
        }
        // Delete revoked sessions older than 7 days
        let revoked_cutoff = cutoff - chrono::Duration::days(7);
        if let Err(e) = mobile_session::Entity::delete_many()
            .filter(mobile_session::Column::RevokedAt.is_not_null())
            .filter(mobile_session::Column::UpdatedAt.lt(revoked_cutoff))
            .exec(&state.db)
            .await
        {
            tracing::error!("Revoked session cleanup error: {e}");
        }
    }
}
