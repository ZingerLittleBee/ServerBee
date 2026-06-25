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

                execute_check(&state, &monitor).await;
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

/// Execute a single monitor check: run the checker, insert the record, update state,
/// and send notifications if needed.
pub(crate) async fn execute_check(
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
        tracing::error!("Failed to update check state for {}: {e}", monitor.id);
        return;
    }

    // Determine if we need to send notifications
    let was_failing = monitor.last_status == Some(false);

    // Skip notifications if any associated server is in maintenance
    let in_maintenance = if let Some(ref server_ids_json) = monitor.server_ids_json {
        let server_ids: Vec<String> = serde_json::from_str(server_ids_json).unwrap_or_default();
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

        if let Err(e) =
            NotificationService::send_group(&state.db, &state.config, group_id, &ctx).await
        {
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

        if let Err(e) =
            NotificationService::send_group(&state.db, &state.config, group_id, &ctx).await
        {
            tracing::error!(
                "Failed to send recovery notification for {}: {e}",
                monitor.name
            );
        }
    }
}

#[cfg(test)]
mod tests {
    // `super::*` already brings `ServiceMonitorService`, `HashMap`, `Instant`,
    // `Utc`, and `AppState` into scope from the parent module.
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::service_monitor;
    use crate::test_utils::setup_test_db;
    use chrono::TimeZone;
    use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

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

    /// Insert a monitor row so `execute_check` can write a record and update state.
    async fn insert_monitor(
        db: &DatabaseConnection,
        id: &str,
        monitor_type: &str,
        target: &str,
        retry_count: i32,
        last_status: Option<bool>,
        consecutive_failures: i32,
    ) -> service_monitor::Model {
        service_monitor::ActiveModel {
            id: Set(id.to_string()),
            name: Set(format!("monitor-{id}")),
            monitor_type: Set(monitor_type.to_string()),
            target: Set(target.to_string()),
            interval: Set(60),
            config_json: Set("{}".to_string()),
            notification_group_id: Set(None),
            retry_count: Set(retry_count),
            server_ids_json: Set(None),
            enabled: Set(true),
            last_status: Set(last_status),
            consecutive_failures: Set(consecutive_failures),
            last_checked_at: Set(None),
            created_at: Set(fixed_now()),
            updated_at: Set(fixed_now()),
        }
        .insert(db)
        .await
        .expect("insert monitor should succeed")
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

    // ---- execute_check (full DB side-effect path) ----

    #[tokio::test]
    async fn execute_check_writes_failure_record_for_offline_target() {
        let (db, _tmp) = setup_test_db().await;
        // Invalid TCP target ("no-port") fails fast and offline (no network).
        insert_monitor(&db, "mon-fail", "tcp", "no-port-here", 1, None, 0).await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();

        let monitor = ServiceMonitorService::get(&state.db, "mon-fail")
            .await
            .unwrap();
        execute_check(&state, &monitor).await;

        // A failure record was written.
        let records = ServiceMonitorService::get_records(&state.db, "mon-fail", None, None, None)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert!(!records[0].success);
        assert!(
            records[0].error.is_some(),
            "offline check should record an error message"
        );

        // Monitor state advanced: last_status=false, consecutive_failures bumped to 1.
        let updated = ServiceMonitorService::get(&state.db, "mon-fail")
            .await
            .unwrap();
        assert_eq!(updated.last_status, Some(false));
        assert_eq!(updated.consecutive_failures, 1);
        assert!(updated.last_checked_at.is_some());
    }

    #[tokio::test]
    async fn execute_check_accumulates_consecutive_failures() {
        let (db, _tmp) = setup_test_db().await;
        // Seed a monitor that has already failed once.
        insert_monitor(&db, "mon-fail2", "tcp", "no-port-here", 1, Some(false), 1).await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();

        let monitor = ServiceMonitorService::get(&state.db, "mon-fail2")
            .await
            .unwrap();
        execute_check(&state, &monitor).await;

        // The failure count increments from the monitor's prior value (1 -> 2).
        let updated = ServiceMonitorService::get(&state.db, "mon-fail2")
            .await
            .unwrap();
        assert_eq!(updated.consecutive_failures, 2);
        assert_eq!(updated.last_status, Some(false));
    }

    #[tokio::test]
    async fn execute_check_handles_unknown_monitor_type() {
        let (db, _tmp) = setup_test_db().await;
        // Unknown type returns a deterministic failure without any network call.
        insert_monitor(&db, "mon-unknown", "bogus", "whatever", 1, None, 0).await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();

        let monitor = ServiceMonitorService::get(&state.db, "mon-unknown")
            .await
            .unwrap();
        execute_check(&state, &monitor).await;

        let records = ServiceMonitorService::get_records(&state.db, "mon-unknown", None, None, None)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert!(!records[0].success);
        assert!(
            records[0]
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("Unknown monitor type")
        );
    }
}
