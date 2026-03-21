use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::Semaphore;

use crate::service::checker;
use crate::service::maintenance::MaintenanceService;
use crate::service::notification::{NotificationService, NotifyContext};
use crate::service::service_monitor::ServiceMonitorService;
use crate::state::AppState;

/// Maximum number of concurrent service monitor checks.
const MAX_CONCURRENT_CHECKS: usize = 20;

/// Background task that periodically checks enabled service monitors.
///
/// Ticks every 10 seconds, queries enabled monitors from the database,
/// and dispatches checks using a semaphore-bounded concurrency pool.
pub async fn run(state: Arc<AppState>) {
    // Wait a bit before starting to let migrations and other init complete
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    tracing::info!("Service monitor checker started");

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS));
    let schedule: Arc<tokio::sync::Mutex<HashMap<String, Instant>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));

    loop {
        interval.tick().await;

        let monitors = match ServiceMonitorService::list_enabled(&state.db).await {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("Failed to list enabled service monitors: {e}");
                continue;
            }
        };

        if monitors.is_empty() {
            continue;
        }

        let now = Instant::now();
        let mut sched = schedule.lock().await;

        // Bootstrap schedule for new monitors based on last_checked_at
        for monitor in &monitors {
            sched.entry(monitor.id.clone()).or_insert_with(|| {
                if let Some(last_checked) = monitor.last_checked_at {
                    let elapsed_since_check = Utc::now()
                        .signed_duration_since(last_checked)
                        .num_seconds()
                        .max(0) as u64;
                    let interval_secs = monitor.interval.max(1) as u64;
                    if elapsed_since_check >= interval_secs {
                        // Overdue: schedule immediately
                        now
                    } else {
                        // Not yet due: schedule for remaining time
                        let remaining = interval_secs - elapsed_since_check;
                        now + std::time::Duration::from_secs(remaining)
                    }
                } else {
                    // Never checked: run immediately
                    now
                }
            });
        }

        // Clean up schedule entries for monitors that no longer exist
        let active_ids: std::collections::HashSet<&str> =
            monitors.iter().map(|m| m.id.as_str()).collect();
        sched.retain(|id, _| active_ids.contains(id.as_str()));

        // Collect monitors that are due for a check
        let mut due_monitors = Vec::new();
        for monitor in &monitors {
            if let Some(next_at) = sched.get(&monitor.id)
                && now >= *next_at
            {
                due_monitors.push(monitor.clone());
                // Schedule next check
                let interval_secs = monitor.interval.max(1) as u64;
                sched.insert(
                    monitor.id.clone(),
                    now + std::time::Duration::from_secs(interval_secs),
                );
            }
        }

        drop(sched);

        // Dispatch checks with bounded concurrency
        for monitor in due_monitors {
            let state = state.clone();
            let semaphore = semaphore.clone();

            tokio::spawn(async move {
                let _permit = match semaphore.acquire().await {
                    Ok(p) => p,
                    Err(_) => return,
                };

                execute_check(&state, &monitor).await;
            });
        }
    }
}

/// Execute a single monitor check: run the checker, insert the record, update state,
/// and send notifications if needed.
async fn execute_check(
    state: &AppState,
    monitor: &crate::entity::service_monitor::Model,
) {
    let config: serde_json::Value = serde_json::from_str(&monitor.config_json).unwrap_or_default();

    let result = checker::run_check(&monitor.monitor_type, &monitor.target, &config).await;

    // Insert the record
    if let Err(e) = ServiceMonitorService::insert_record(
        &state.db,
        &monitor.id,
        result.success,
        result.latency,
        result.detail.clone(),
        result.error.clone(),
    )
    .await
    {
        tracing::error!(
            "Failed to insert service monitor record for {}: {e}",
            monitor.id
        );
        return;
    }

    // Calculate new consecutive failure count
    let consecutive_failures = if result.success {
        0
    } else {
        monitor.consecutive_failures + 1
    };

    // Update monitor state
    if let Err(e) = ServiceMonitorService::update_check_state(
        &state.db,
        &monitor.id,
        result.success,
        consecutive_failures,
    )
    .await
    {
        tracing::error!(
            "Failed to update check state for {}: {e}",
            monitor.id
        );
        return;
    }

    // Determine if we need to send notifications
    let was_failing = monitor.last_status == Some(false);

    // Skip notifications if any associated server is in maintenance
    let in_maintenance = if let Some(ref server_ids_json) = monitor.server_ids_json {
        let server_ids: Vec<String> =
            serde_json::from_str(server_ids_json).unwrap_or_default();
        let mut any_in_maintenance = false;
        for sid in &server_ids {
            if MaintenanceService::is_in_maintenance(&state.db, sid)
                .await
                .unwrap_or(false)
            {
                any_in_maintenance = true;
                break;
            }
        }
        any_in_maintenance
    } else {
        false
    };

    if in_maintenance {
        tracing::debug!(
            "Skipping notification for service monitor '{}': associated server in maintenance",
            monitor.name
        );
        return;
    }

    // Failure notification: consecutive failures exceeded retry_count
    if !result.success
        && consecutive_failures > monitor.retry_count
        && let Some(ref group_id) = monitor.notification_group_id
    {
        let error_msg = result.error.as_deref().unwrap_or("Unknown error");
        let ctx = NotifyContext {
            server_name: monitor.name.clone(),
            server_id: monitor.id.clone(),
            rule_name: format!("{} ({})", monitor.name, monitor.monitor_type),
            event: "triggered".to_string(),
            message: format!(
                "Service monitor '{}' failed after {} consecutive failures: {}",
                monitor.name, consecutive_failures, error_msg
            ),
            time: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            ..Default::default()
        };

        if let Err(e) = NotificationService::send_group(&state.db, group_id, &ctx).await {
            tracing::error!(
                "Failed to send failure notification for {}: {e}",
                monitor.name
            );
        }
    }

    // Recovery notification: was failing, now succeeded
    if result.success
        && was_failing
        && let Some(ref group_id) = monitor.notification_group_id
    {
        let ctx = NotifyContext {
            server_name: monitor.name.clone(),
            server_id: monitor.id.clone(),
            rule_name: format!("{} ({})", monitor.name, monitor.monitor_type),
            event: "recovered".to_string(),
            message: format!(
                "Service monitor '{}' has recovered after {} consecutive failures",
                monitor.name, monitor.consecutive_failures
            ),
            time: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            ..Default::default()
        };

        if let Err(e) = NotificationService::send_group(&state.db, group_id, &ctx).await {
            tracing::error!(
                "Failed to send recovery notification for {}: {e}",
                monitor.name
            );
        }
    }
}
