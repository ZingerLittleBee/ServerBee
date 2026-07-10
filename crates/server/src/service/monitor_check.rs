//! The complete service-monitor check transition.
//!
//! Both the background scheduler ([`crate::task::service_monitor_checker`])
//! and the manual HTTP trigger execute checks through
//! [`MonitorCheckRunner::run_check`], which owns the entire transition:
//! per-monitor overlap guard, checker dispatch, transactional record + state
//! write, maintenance gating, and failure/recovery notifications. Callers
//! only choose *when* a check happens; *what a check means* lives here.

use chrono::Utc;
use dashmap::DashMap;
use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::config::AppConfig;
use crate::entity::{service_monitor, service_monitor_record};
use crate::error::AppError;
use crate::service::checker::{self, CheckResult};
use crate::service::maintenance::MaintenanceService;
use crate::service::notification::{NotificationService, NotifyContext};
use crate::service::service_monitor::ServiceMonitorService;

/// Outcome of a check request.
pub enum CheckOutcome {
    /// The check ran to completion; the inserted record is returned.
    Completed(service_monitor_record::Model),
    /// A check for this monitor is still in flight; nothing was run.
    AlreadyRunning,
}

/// Owns the check transition for all callers and guarantees at most one
/// in-flight check per monitor.
pub struct MonitorCheckRunner {
    in_flight: DashMap<String, ()>,
}

impl Default for MonitorCheckRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorCheckRunner {
    pub fn new() -> Self {
        Self {
            in_flight: DashMap::new(),
        }
    }

    /// Run one complete check transition for the monitor with `monitor_id`.
    ///
    /// Returns [`CheckOutcome::AlreadyRunning`] without touching the database
    /// when a check for the same monitor has not finished yet, so slow checks
    /// can never interleave their record/state writes. Notification failures
    /// are logged, not returned: the check itself succeeded.
    pub async fn run_check(
        &self,
        db: &DatabaseConnection,
        config: &AppConfig,
        monitor_id: &str,
    ) -> Result<CheckOutcome, AppError> {
        let Some(_guard) = self.try_acquire(monitor_id) else {
            return Ok(CheckOutcome::AlreadyRunning);
        };

        // Load fresh state so a run dispatched from a stale model (the
        // scheduler fetches monitors up to a tick earlier) still sees the
        // current failure count and last status.
        let monitor = ServiceMonitorService::get(db, monitor_id).await?;

        let checker_config: serde_json::Value =
            serde_json::from_str(&monitor.config_json).unwrap_or_default();
        let result = checker::run_check(&monitor.monitor_type, &monitor.target, &checker_config).await;

        let consecutive_failures = if result.success {
            0
        } else {
            monitor.consecutive_failures + 1
        };

        // The record and the monitor state describe the same transition —
        // commit them together so readers never see one without the other.
        let txn = db.begin().await?;
        let record = ServiceMonitorService::insert_record(
            &txn,
            &monitor.id,
            result.success,
            result.latency,
            result.detail.clone(),
            result.error.clone(),
        )
        .await?;
        ServiceMonitorService::update_check_state(
            &txn,
            &monitor.id,
            result.success,
            consecutive_failures,
        )
        .await?;
        txn.commit().await?;

        self.notify(db, config, &monitor, &result, consecutive_failures)
            .await;

        Ok(CheckOutcome::Completed(record))
    }

    /// Reserve the in-flight slot for `monitor_id`, or return `None` if a
    /// check is already running. The slot is released when the guard drops,
    /// including on panic or early return.
    fn try_acquire(&self, monitor_id: &str) -> Option<InFlightGuard<'_>> {
        match self.in_flight.entry(monitor_id.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(_) => None,
            dashmap::mapref::entry::Entry::Vacant(v) => {
                v.insert(());
                Some(InFlightGuard {
                    map: &self.in_flight,
                    id: monitor_id.to_string(),
                })
            }
        }
    }

    /// Maintenance gate plus failure/recovery notifications.
    ///
    /// `monitor` carries the *pre-check* state (`last_status`,
    /// `consecutive_failures`), which decides whether this transition is a
    /// new failure past the retry threshold or a recovery.
    async fn notify(
        &self,
        db: &DatabaseConnection,
        config: &AppConfig,
        monitor: &service_monitor::Model,
        result: &CheckResult,
        consecutive_failures: i32,
    ) {
        let Some(ref group_id) = monitor.notification_group_id else {
            return;
        };

        let failure_crossed_threshold =
            !result.success && consecutive_failures > monitor.retry_count;
        let recovered = result.success && monitor.last_status == Some(false);
        if !failure_crossed_threshold && !recovered {
            return;
        }

        if self.any_server_in_maintenance(db, monitor).await {
            tracing::debug!(
                "Skipping notification for service monitor '{}': associated server in maintenance",
                monitor.name
            );
            return;
        }

        let ctx = if failure_crossed_threshold {
            let error_msg = result.error.as_deref().unwrap_or("Unknown error");
            NotifyContext {
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
            }
        } else {
            NotifyContext {
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
            }
        };

        if let Err(e) = NotificationService::send_group(db, config, group_id, &ctx).await {
            tracing::error!(
                "Failed to send {} notification for {}: {e}",
                ctx.event,
                monitor.name
            );
        }
    }

    async fn any_server_in_maintenance(
        &self,
        db: &DatabaseConnection,
        monitor: &service_monitor::Model,
    ) -> bool {
        let Some(ref server_ids_json) = monitor.server_ids_json else {
            return false;
        };
        let server_ids: Vec<String> = serde_json::from_str(server_ids_json).unwrap_or_default();
        for sid in &server_ids {
            if MaintenanceService::is_in_maintenance(db, sid)
                .await
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }
}

/// Releases the per-monitor in-flight slot on drop.
struct InFlightGuard<'a> {
    map: &'a DashMap<String, ()>,
    id: String,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.map.remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::service_monitor;
    use crate::test_utils::setup_test_db;
    use chrono::TimeZone;
    use sea_orm::{ActiveModelTrait, Set};

    fn fixed_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 25, 12, 0, 0).unwrap()
    }

    /// Insert a monitor row so `run_check` can write a record and update state.
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

    #[tokio::test]
    async fn run_check_writes_failure_record_for_offline_target() {
        let (db, _tmp) = setup_test_db().await;
        // Invalid TCP target ("no-port") fails fast and offline (no network).
        insert_monitor(&db, "mon-fail", "tcp", "no-port-here", 1, None, 0).await;
        let runner = MonitorCheckRunner::new();

        let outcome = runner
            .run_check(&db, &AppConfig::default(), "mon-fail")
            .await
            .unwrap();

        // The outcome carries the failure record that was written.
        let CheckOutcome::Completed(record) = outcome else {
            panic!("expected Completed outcome");
        };
        assert!(!record.success);
        assert!(
            record.error.is_some(),
            "offline check should record an error message"
        );

        let records = ServiceMonitorService::get_records(&db, "mon-fail", None, None, None)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);

        // Monitor state advanced: last_status=false, consecutive_failures bumped to 1.
        let updated = ServiceMonitorService::get(&db, "mon-fail").await.unwrap();
        assert_eq!(updated.last_status, Some(false));
        assert_eq!(updated.consecutive_failures, 1);
        assert!(updated.last_checked_at.is_some());
    }

    #[tokio::test]
    async fn run_check_accumulates_consecutive_failures() {
        let (db, _tmp) = setup_test_db().await;
        // Seed a monitor that has already failed once.
        insert_monitor(&db, "mon-fail2", "tcp", "no-port-here", 1, Some(false), 1).await;
        let runner = MonitorCheckRunner::new();

        runner
            .run_check(&db, &AppConfig::default(), "mon-fail2")
            .await
            .unwrap();

        // The failure count increments from the monitor's prior value (1 -> 2).
        let updated = ServiceMonitorService::get(&db, "mon-fail2").await.unwrap();
        assert_eq!(updated.consecutive_failures, 2);
        assert_eq!(updated.last_status, Some(false));
    }

    #[tokio::test]
    async fn run_check_handles_unknown_monitor_type() {
        let (db, _tmp) = setup_test_db().await;
        // Unknown type returns a deterministic failure without any network call.
        insert_monitor(&db, "mon-unknown", "bogus", "whatever", 1, None, 0).await;
        let runner = MonitorCheckRunner::new();

        runner
            .run_check(&db, &AppConfig::default(), "mon-unknown")
            .await
            .unwrap();

        let records = ServiceMonitorService::get_records(&db, "mon-unknown", None, None, None)
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

    #[tokio::test]
    async fn run_check_unknown_monitor_id_is_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let runner = MonitorCheckRunner::new();

        let result = runner
            .run_check(&db, &AppConfig::default(), "missing")
            .await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn run_check_skips_when_already_in_flight() {
        let (db, _tmp) = setup_test_db().await;
        insert_monitor(&db, "mon-busy", "tcp", "no-port-here", 1, None, 0).await;
        let runner = MonitorCheckRunner::new();

        // Simulate a check that has not finished yet.
        let guard = runner.try_acquire("mon-busy").expect("slot must be free");

        let outcome = runner
            .run_check(&db, &AppConfig::default(), "mon-busy")
            .await
            .unwrap();
        assert!(matches!(outcome, CheckOutcome::AlreadyRunning));

        // No record was written and state did not advance.
        let records = ServiceMonitorService::get_records(&db, "mon-busy", None, None, None)
            .await
            .unwrap();
        assert!(records.is_empty());

        // Once the in-flight check finishes, the next run proceeds.
        drop(guard);
        let outcome = runner
            .run_check(&db, &AppConfig::default(), "mon-busy")
            .await
            .unwrap();
        assert!(matches!(outcome, CheckOutcome::Completed(_)));
    }

    #[tokio::test]
    async fn in_flight_slot_is_released_after_completion() {
        let (db, _tmp) = setup_test_db().await;
        insert_monitor(&db, "mon-seq", "tcp", "no-port-here", 1, None, 0).await;
        let runner = MonitorCheckRunner::new();

        for _ in 0..2 {
            let outcome = runner
                .run_check(&db, &AppConfig::default(), "mon-seq")
                .await
                .unwrap();
            assert!(matches!(outcome, CheckOutcome::Completed(_)));
        }

        let records = ServiceMonitorService::get_records(&db, "mon-seq", None, None, None)
            .await
            .unwrap();
        assert_eq!(records.len(), 2);
    }
}
