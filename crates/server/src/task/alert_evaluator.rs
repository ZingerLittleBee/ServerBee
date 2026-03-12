use std::sync::Arc;

use crate::service::alert::{AlertService, AlertStateManager};
use crate::state::AppState;

/// Runs every 60 seconds to evaluate all enabled alert rules.
pub async fn run(state: Arc<AppState>) {
    // Load existing triggered states from DB
    let state_manager = loop {
        match AlertStateManager::load_from_db(&state.db).await {
            Ok(sm) => break sm,
            Err(e) => {
                tracing::error!("Failed to load alert states, retrying in 10s: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    };

    tracing::info!("Alert evaluator started");

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;

        if let Err(e) =
            AlertService::evaluate_all(&state.db, &state.agent_manager, &state_manager).await
        {
            tracing::error!("Alert evaluation error: {e}");
        }
    }
}
