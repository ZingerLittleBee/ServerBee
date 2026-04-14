use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;

use crate::state::AppState;

/// Periodically marks stuck upgrade jobs as timed out and removes expired history.
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        let now = Utc::now();
        let timed_out = state.upgrade_tracker.sweep_timeouts(now);
        let removed = state.upgrade_tracker.cleanup_old(now);

        if !timed_out.is_empty() {
            tracing::warn!("Timed out {} upgrade job(s)", timed_out.len());
        }
        if removed > 0 {
            tracing::debug!("Removed {removed} expired upgrade job(s)");
        }
    }
}
