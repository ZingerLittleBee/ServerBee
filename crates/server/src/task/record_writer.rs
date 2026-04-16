use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Timelike, Utc};

use crate::service::record::RecordService;
use crate::service::traffic::{TrafficService, compute_delta};
use crate::state::AppState;

/// Periodically writes cached agent reports to the database (every 60 seconds).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));

    // Initialize transfer cache from traffic_state table
    let mut transfer_cache: HashMap<String, (i64, i64)> =
        match TrafficService::load_transfer_cache(&state.db).await {
            Ok(cache) => {
                tracing::info!("Loaded {} transfer cache entries", cache.len());
                cache
            }
            Err(e) => {
                tracing::error!("Failed to load transfer cache: {e}");
                HashMap::new()
            }
        };

    loop {
        interval.tick().await;

        let reports = state.agent_manager.all_latest_reports();
        if reports.is_empty() {
            continue;
        }

        let now = Utc::now();
        // Truncate to hour boundary for hourly bucketing
        let hour = now
            .date_naive()
            .and_hms_opt(now.time().hour(), 0, 0)
            .unwrap()
            .and_utc();

        let mut count = 0;
        for (server_id, report) in &reports {
            let writes_allowed = state.recovery_lock.writes_allowed_for(server_id);

            // Save metrics record
            if writes_allowed {
                if let Err(e) = RecordService::save_report(&state.db, server_id, report).await {
                    tracing::error!("Failed to save record for {server_id}: {e}");
                } else {
                    count += 1;
                }
            } else {
                tracing::info!("Skipping recovery-frozen record write for {server_id}");
            }

            // Compute traffic delta
            let curr_in = report.net_in_transfer;
            let curr_out = report.net_out_transfer;

            if curr_in == 0 && curr_out == 0 {
                // No traffic data available, skip
                continue;
            }

            let (delta_in, delta_out) =
                if let Some(&(prev_in, prev_out)) = transfer_cache.get(server_id) {
                    compute_delta(prev_in, prev_out, curr_in, curr_out)
                } else {
                    // First observation: no previous state, skip delta (just record state)
                    transfer_cache.insert(server_id.clone(), (curr_in, curr_out));
                    if writes_allowed {
                        if let Err(e) = TrafficService::upsert_state(
                            &state.db,
                            server_id,
                            curr_in,
                            curr_out,
                        )
                        .await
                        {
                            tracing::error!("Failed to upsert traffic state for {server_id}: {e}");
                        }
                    } else {
                        tracing::info!("Skipping recovery-frozen traffic state write for {server_id}");
                    }
                    continue;
                };

            // Update cache
            transfer_cache.insert(server_id.clone(), (curr_in, curr_out));

            // Only write if there's actual traffic
            if delta_in > 0 || delta_out > 0 {
                if writes_allowed {
                    if let Err(e) = TrafficService::upsert_hourly(
                        &state.db,
                        server_id,
                        hour,
                        delta_in,
                        delta_out,
                    )
                    .await
                    {
                        tracing::error!("Failed to upsert traffic hourly for {server_id}: {e}");
                    }
                } else {
                    tracing::info!("Skipping recovery-frozen traffic hourly write for {server_id}");
                }
            }

            // Always update state
            if writes_allowed {
                if let Err(e) =
                    TrafficService::upsert_state(&state.db, server_id, curr_in, curr_out).await
                {
                    tracing::error!("Failed to upsert traffic state for {server_id}: {e}");
                }
            } else {
                tracing::info!("Skipping recovery-frozen traffic state write for {server_id}");
            }
        }

        tracing::debug!("Wrote {count} metric records");
    }
}
