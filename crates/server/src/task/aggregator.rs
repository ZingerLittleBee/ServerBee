use std::sync::Arc;
use std::time::Duration;

use crate::service::record::RecordService;
use crate::state::AppState;

/// Periodically aggregates raw records into hourly summaries (every 3600 seconds).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;

        match RecordService::aggregate_hourly(&state.db).await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!("Aggregated hourly records for {count} servers");
                }
            }
            Err(e) => {
                tracing::error!("Failed to aggregate hourly records: {e}");
            }
        }
    }
}
