use std::sync::Arc;
use std::time::Duration;

use crate::state::AppState;

/// Periodically checks for agents that have stopped reporting and marks them offline.
/// Runs every 10 seconds with a 30-second threshold.
/// Also cleans up expired pending requests (older than 60s).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        let offline_ids = state.agent_manager.check_offline(30);
        for id in &offline_ids {
            tracing::info!("Agent {id} marked offline (no report for 30s)");
        }

        // Clean up expired pending requests (HTTP→WS relay) based on per-entry TTL
        state.agent_manager.cleanup_expired_requests();

        // Clean up expired traceroute results (older than 120s)
        state.agent_manager.cleanup_traceroute_results();
    }
}
