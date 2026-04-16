use std::sync::Arc;

use crate::service::alert::AlertService;
use crate::state::AppState;

/// Runs every 60 seconds to evaluate all enabled alert rules.
pub async fn run(state: Arc<AppState>) {
    tracing::info!("Alert evaluator started");

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;

        if let Err(e) = AlertService::evaluate_all(
            &state.db,
            &state.config,
            &state.agent_manager,
            &state.alert_state_manager,
        )
        .await
        {
            tracing::error!("Alert evaluation error: {e}");
        }
    }
}
