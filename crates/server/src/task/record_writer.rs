use std::sync::Arc;
use std::time::Duration;

use crate::service::record::RecordService;
use crate::state::AppState;

/// Periodically writes cached agent reports to the database (every 60 seconds).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));

    loop {
        interval.tick().await;

        let reports = state.agent_manager.all_latest_reports();
        if reports.is_empty() {
            continue;
        }

        let mut count = 0;
        for (server_id, report) in &reports {
            if let Err(e) = RecordService::save_report(&state.db, server_id, report).await {
                tracing::error!("Failed to save record for {server_id}: {e}");
            } else {
                count += 1;
            }
        }

        tracing::debug!("Wrote {count} metric records");
    }
}
