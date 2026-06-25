use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::service::upgrade_tracker::UpgradeJobTracker;
use crate::state::AppState;

/// Periodically marks stuck upgrade jobs as timed out and removes expired history.
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        run_tick(&state.upgrade_tracker, Utc::now());
    }
}

/// Performs the work of a single `run` iteration: sweep timed-out upgrade jobs and
/// drop expired terminal history. Extracted so it can be unit-tested deterministically
/// with a fixed `now`.
pub(crate) fn run_tick(tracker: &UpgradeJobTracker, now: DateTime<Utc>) {
    let timed_out = tracker.sweep_timeouts(now);
    let removed = tracker.cleanup_old(now);

    if !timed_out.is_empty() {
        tracing::warn!("Timed out {} upgrade job(s)", timed_out.len());
    }
    if removed > 0 {
        tracing::debug!("Removed {removed} expired upgrade job(s)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::upgrade_tracker::{UPGRADE_RETENTION_HOURS, UPGRADE_TIMEOUT_SECS, UpgradeJob};
    use chrono::{Duration as ChronoDuration, TimeZone};
    use serverbee_common::protocol::{BrowserMessage, UpgradeStage, UpgradeStatus};
    use tokio::sync::broadcast;

    /// Fixed reference instant so timeout/retention cutoffs are deterministic.
    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 25, 12, 0, 0).unwrap()
    }

    fn make_tracker() -> (UpgradeJobTracker, broadcast::Receiver<BrowserMessage>) {
        let (tx, rx) = broadcast::channel(16);
        (UpgradeJobTracker::new(tx), rx)
    }

    fn running_job(server_id: &str, started_at: DateTime<Utc>) -> UpgradeJob {
        UpgradeJob {
            server_id: server_id.to_string(),
            job_id: format!("job-{server_id}"),
            target_version: "1.0.0".to_string(),
            stage: UpgradeStage::Downloading,
            status: UpgradeStatus::Running,
            error: None,
            backup_path: None,
            started_at,
            finished_at: None,
        }
    }

    fn terminal_job(
        server_id: &str,
        status: UpgradeStatus,
        finished_at: DateTime<Utc>,
    ) -> UpgradeJob {
        UpgradeJob {
            server_id: server_id.to_string(),
            job_id: format!("job-{server_id}"),
            target_version: "1.0.0".to_string(),
            stage: UpgradeStage::Installing,
            status,
            error: Some("done".to_string()),
            backup_path: None,
            started_at: finished_at,
            finished_at: Some(finished_at),
        }
    }

    // Empty tracker: a tick is a no-op and broadcasts nothing.
    #[test]
    fn run_tick_on_empty_tracker_is_noop() {
        let (tracker, mut rx) = make_tracker();

        run_tick(&tracker, fixed_now());

        assert_eq!(tracker.snapshot().len(), 0);
        assert!(rx.try_recv().is_err(), "empty tick should not broadcast");
    }

    // Happy path: a stale running job flips to Timeout while a recently started one stays Running.
    #[test]
    fn run_tick_times_out_stale_running_jobs_only() {
        let (tracker, mut rx) = make_tracker();
        let now = fixed_now();

        // Started well past the timeout window -> should be swept.
        let stale_started = now - ChronoDuration::seconds(UPGRADE_TIMEOUT_SECS + 30);
        tracker
            .jobs
            .insert("stale".into(), running_job("stale", stale_started));

        // Started just inside the timeout window -> should be left running.
        let recent_started = now - ChronoDuration::seconds(UPGRADE_TIMEOUT_SECS - 10);
        tracker
            .jobs
            .insert("recent".into(), running_job("recent", recent_started));

        run_tick(&tracker, now);

        let stale = tracker.get("stale").expect("stale job should remain in map");
        assert_eq!(stale.status, UpgradeStatus::Timeout);
        assert_eq!(stale.finished_at, Some(now));
        assert!(
            stale
                .error
                .as_deref()
                .is_some_and(|e| e.contains("timed out")),
            "timed out job should carry a timeout error message"
        );

        let recent = tracker
            .get("recent")
            .expect("recent job should remain in map");
        assert_eq!(recent.status, UpgradeStatus::Running);
        assert_eq!(recent.finished_at, None);

        // Exactly one timeout result should have been broadcast (for the stale job).
        let msg = rx.try_recv().expect("timeout should broadcast a result");
        assert!(matches!(
            msg,
            BrowserMessage::UpgradeResult {
                server_id,
                status: UpgradeStatus::Timeout,
                ..
            } if server_id == "stale"
        ));
        assert!(
            rx.try_recv().is_err(),
            "only the stale job should broadcast a result"
        );
    }

    // Boundary: the skip guard is `started_at > cutoff`, so a job started EXACTLY
    // UPGRADE_TIMEOUT_SECS ago sits at the cutoff, is not skipped, and IS timed out
    // (the timeout boundary is inclusive).
    #[test]
    fn run_tick_times_out_job_exactly_at_timeout_boundary() {
        let (tracker, _rx) = make_tracker();
        let now = fixed_now();

        let boundary_started = now - ChronoDuration::seconds(UPGRADE_TIMEOUT_SECS);
        tracker
            .jobs
            .insert("boundary".into(), running_job("boundary", boundary_started));

        run_tick(&tracker, now);

        let boundary = tracker.get("boundary").expect("boundary job should remain");
        assert_eq!(boundary.status, UpgradeStatus::Timeout);
        assert_eq!(boundary.finished_at, Some(now));
    }

    // Retention cleanup: expired terminal jobs are removed, fresh terminal and running jobs are kept.
    #[test]
    fn run_tick_removes_expired_terminal_history() {
        let (tracker, _rx) = make_tracker();
        let now = fixed_now();

        let expired_finished = now - ChronoDuration::hours(UPGRADE_RETENTION_HOURS + 1);
        let fresh_finished = now - ChronoDuration::hours(1);

        tracker.jobs.insert(
            "expired".into(),
            terminal_job("expired", UpgradeStatus::Succeeded, expired_finished),
        );
        tracker.jobs.insert(
            "fresh".into(),
            terminal_job("fresh", UpgradeStatus::Failed, fresh_finished),
        );
        // A long-running job that has not timed out yet must survive cleanup.
        let recent_started = now - ChronoDuration::seconds(UPGRADE_TIMEOUT_SECS - 5);
        tracker
            .jobs
            .insert("running".into(), running_job("running", recent_started));

        run_tick(&tracker, now);

        assert!(
            tracker.get("expired").is_none(),
            "expired terminal job should be cleaned up"
        );
        assert!(
            tracker.get("fresh").is_some(),
            "fresh terminal job should be retained"
        );
        assert!(
            tracker.get("running").is_some(),
            "running job should be retained"
        );
    }

    // Combined path within one tick: a stale running job is timed out, then (since its finished_at is
    // `now`, well inside retention) it is NOT cleaned up in the same tick, while a separately expired
    // terminal job is removed.
    #[test]
    fn run_tick_sweeps_then_cleans_in_one_iteration() {
        let (tracker, mut rx) = make_tracker();
        let now = fixed_now();

        let stale_started = now - ChronoDuration::seconds(UPGRADE_TIMEOUT_SECS + 60);
        tracker
            .jobs
            .insert("stale".into(), running_job("stale", stale_started));

        let expired_finished = now - ChronoDuration::hours(UPGRADE_RETENTION_HOURS + 5);
        tracker.jobs.insert(
            "expired".into(),
            terminal_job("expired", UpgradeStatus::Succeeded, expired_finished),
        );

        run_tick(&tracker, now);

        // The just-timed-out job stays (finished_at == now is fresh).
        let stale = tracker.get("stale").expect("freshly timed-out job stays");
        assert_eq!(stale.status, UpgradeStatus::Timeout);
        // The pre-existing expired terminal job is gone.
        assert!(tracker.get("expired").is_none());

        // Exactly one broadcast: the timeout result for "stale".
        let msg = rx.try_recv().expect("timeout should broadcast");
        assert!(matches!(
            msg,
            BrowserMessage::UpgradeResult {
                server_id,
                status: UpgradeStatus::Timeout,
                ..
            } if server_id == "stale"
        ));
        assert!(rx.try_recv().is_err());
    }
}
