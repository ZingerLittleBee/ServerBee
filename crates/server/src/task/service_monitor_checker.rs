use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::Semaphore;

use crate::service::monitor_check::CheckOutcome;
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
        let due_monitors = {
            let mut sched = schedule.lock().await;
            select_due_monitors(&monitors, &mut sched, now, Utc::now())
        };

        // Dispatch checks with bounded concurrency
        for monitor in due_monitors {
            let state = state.clone();
            let semaphore = semaphore.clone();

            tokio::spawn(async move {
                let _permit = match semaphore.acquire().await {
                    Ok(p) => p,
                    Err(_) => return,
                };

                match state
                    .monitor_check_runner
                    .run_check(&state.db, &state.config, &monitor.id)
                    .await
                {
                    Ok(CheckOutcome::Completed(_)) => {}
                    Ok(CheckOutcome::AlreadyRunning) => tracing::debug!(
                        "Skipping check for '{}': previous check still running",
                        monitor.name
                    ),
                    Err(e) => {
                        tracing::error!("Service monitor check failed for {}: {e}", monitor.id);
                    }
                }
            });
        }
    }
}

/// Update the per-monitor `schedule` against the current monitor set and return the
/// monitors that are due for a check at `now`.
///
/// This is the deterministic core of one scheduler tick, extracted from [`run`] so it
/// can be unit-tested without any timing dependency. Behavior is unchanged: new monitors
/// are bootstrapped from their `last_checked_at`, stale schedule entries (for monitors
/// that no longer exist) are pruned, and each due monitor's next check is rescheduled
/// `interval` seconds out. `now` is the monotonic clock instant; `wall_now` is the wall
/// clock used to compute how overdue a previously-checked monitor is.
pub(crate) fn select_due_monitors(
    monitors: &[crate::entity::service_monitor::Model],
    schedule: &mut HashMap<String, Instant>,
    now: Instant,
    wall_now: chrono::DateTime<Utc>,
) -> Vec<crate::entity::service_monitor::Model> {
    // Bootstrap schedule for new monitors based on last_checked_at
    for monitor in monitors {
        schedule.entry(monitor.id.clone()).or_insert_with(|| {
            if let Some(last_checked) = monitor.last_checked_at {
                let elapsed_since_check = wall_now
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
    schedule.retain(|id, _| active_ids.contains(id.as_str()));

    // Collect monitors that are due for a check
    let mut due_monitors = Vec::new();
    for monitor in monitors {
        if let Some(next_at) = schedule.get(&monitor.id)
            && now >= *next_at
        {
            due_monitors.push(monitor.clone());
            // Schedule next check
            let interval_secs = monitor.interval.max(1) as u64;
            schedule.insert(
                monitor.id.clone(),
                now + std::time::Duration::from_secs(interval_secs),
            );
        }
    }

    due_monitors
}

#[cfg(test)]
mod tests {
    // `super::*` already brings `HashMap`, `Instant`, and `Utc` into scope
    // from the parent module.
    use super::*;
    use crate::entity::service_monitor;
    use chrono::TimeZone;

    /// Fixed wall-clock instant used so scheduling math is deterministic.
    fn fixed_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 25, 12, 0, 0).unwrap()
    }

    /// Build a bare `service_monitor::Model` for pure scheduling tests (no DB).
    fn make_monitor(
        id: &str,
        interval: i32,
        last_checked_at: Option<chrono::DateTime<Utc>>,
    ) -> service_monitor::Model {
        service_monitor::Model {
            id: id.to_string(),
            name: format!("monitor-{id}"),
            monitor_type: "tcp".to_string(),
            target: "example.com:443".to_string(),
            interval,
            config_json: "{}".to_string(),
            notification_group_id: None,
            retry_count: 1,
            server_ids_json: None,
            enabled: true,
            last_status: None,
            consecutive_failures: 0,
            last_checked_at,
            created_at: fixed_now(),
            updated_at: fixed_now(),
        }
    }

    // ---- select_due_monitors (pure scheduler core) ----

    #[test]
    fn select_due_empty_input_is_noop() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        let due = select_due_monitors(&[], &mut schedule, now, fixed_now());
        assert!(due.is_empty());
        assert!(schedule.is_empty());
    }

    #[test]
    fn select_due_never_checked_is_due_immediately() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        let monitors = vec![make_monitor("m1", 60, None)];

        let due = select_due_monitors(&monitors, &mut schedule, now, fixed_now());

        // Never-checked monitor is scheduled at `now`, so it is immediately due.
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "m1");
        // After dispatch, its next check is pushed out by the interval (60s).
        let next = *schedule.get("m1").expect("schedule entry must exist");
        assert!(next >= now + std::time::Duration::from_secs(60));
    }

    #[test]
    fn select_due_recently_checked_is_skipped() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        // Checked 5s ago with a 3600s interval => not yet due.
        let last = fixed_now() - chrono::Duration::seconds(5);
        let monitors = vec![make_monitor("m1", 3600, Some(last))];

        let due = select_due_monitors(&monitors, &mut schedule, now, fixed_now());

        assert!(due.is_empty(), "recently-checked monitor must not be due");
        // It is still scheduled, just for a future instant (~3595s out).
        let next = *schedule.get("m1").expect("schedule entry must exist");
        assert!(next > now);
    }

    #[test]
    fn select_due_overdue_monitor_is_due() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        // Checked 1 hour ago with a 60s interval => overdue.
        let last = fixed_now() - chrono::Duration::seconds(3600);
        let monitors = vec![make_monitor("m1", 60, Some(last))];

        let due = select_due_monitors(&monitors, &mut schedule, now, fixed_now());

        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "m1");
    }

    #[test]
    fn select_due_prunes_stale_schedule_entries() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        // A leftover entry for a monitor no longer in the active set.
        schedule.insert("gone".to_string(), now);
        let monitors = vec![make_monitor("m1", 60, None)];

        let due = select_due_monitors(&monitors, &mut schedule, now, fixed_now());

        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "m1");
        // Stale entry pruned, only the active monitor remains scheduled.
        assert!(!schedule.contains_key("gone"));
        assert!(schedule.contains_key("m1"));
    }

    #[test]
    fn select_due_already_scheduled_future_is_not_re_added() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        // Pre-seed a future schedule (e.g. set on a previous tick): not yet due.
        schedule.insert("m1".to_string(), now + std::time::Duration::from_secs(30));
        let monitors = vec![make_monitor("m1", 60, None)];

        let due = select_due_monitors(&monitors, &mut schedule, now, fixed_now());

        // The existing future entry is preserved (or_insert does not overwrite),
        // so the monitor is not due on this tick.
        assert!(due.is_empty());
    }

    // The full check transition (record/state writes, overlap guard,
    // notifications) is owned and tested by `service::monitor_check`.
}
