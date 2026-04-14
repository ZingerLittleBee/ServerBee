use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serverbee_common::constants::CapabilityDeniedReason;
use serverbee_common::protocol::{BrowserMessage, UpgradeJobDto, UpgradeStage, UpgradeStatus};
use tokio::sync::broadcast;
use uuid::Uuid;

pub const UPGRADE_TIMEOUT_SECS: i64 = 120;
pub const UPGRADE_RETENTION_HOURS: i64 = 24;

#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeJob {
    pub server_id: String,
    pub job_id: String,
    pub target_version: String,
    pub stage: UpgradeStage,
    pub status: UpgradeStatus,
    pub error: Option<String>,
    pub backup_path: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl UpgradeJob {
    fn to_dto(&self) -> UpgradeJobDto {
        UpgradeJobDto {
            server_id: self.server_id.clone(),
            job_id: self.job_id.clone(),
            target_version: self.target_version.clone(),
            stage: self.stage,
            status: self.status,
            error: self.error.clone(),
            backup_path: self.backup_path.clone(),
            started_at: self.started_at,
            finished_at: self.finished_at,
        }
    }

    fn is_terminal(&self) -> bool {
        self.status != UpgradeStatus::Running
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeLookup {
    pub server_id: String,
    pub job_id: Option<String>,
    pub target_version: String,
}

impl UpgradeLookup {
    pub fn new(
        server_id: impl Into<String>,
        job_id: Option<String>,
        target_version: impl Into<String>,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            job_id,
            target_version: target_version.into(),
        }
    }

    pub fn from_job(job: &UpgradeJob) -> Self {
        Self {
            server_id: job.server_id.clone(),
            job_id: Some(job.job_id.clone()),
            target_version: job.target_version.clone(),
        }
    }

    fn matches(&self, job: &UpgradeJob) -> bool {
        if job.server_id != self.server_id {
            return false;
        }

        if let Some(job_id) = &self.job_id {
            job.job_id == *job_id
        } else {
            job.target_version == self.target_version
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StartUpgradeJobError {
    Conflict(UpgradeJob),
}

pub struct UpgradeJobTracker {
    pub(crate) jobs: DashMap<String, UpgradeJob>,
    browser_tx: broadcast::Sender<BrowserMessage>,
}

impl UpgradeJobTracker {
    pub fn new(browser_tx: broadcast::Sender<BrowserMessage>) -> Self {
        Self {
            jobs: DashMap::new(),
            browser_tx,
        }
    }

    pub fn start_job(
        &self,
        server_id: impl Into<String>,
        target_version: impl Into<String>,
    ) -> Result<UpgradeJob, StartUpgradeJobError> {
        let server_id = server_id.into();
        let target_version = target_version.into();

        if let Some(existing) = self.jobs.get(&server_id)
            && existing.status == UpgradeStatus::Running
        {
            return Err(StartUpgradeJobError::Conflict(existing.clone()));
        }

        let job = UpgradeJob {
            server_id: server_id.clone(),
            job_id: Uuid::new_v4().to_string(),
            target_version,
            stage: UpgradeStage::Downloading,
            status: UpgradeStatus::Running,
            error: None,
            backup_path: None,
            started_at: Utc::now(),
            finished_at: None,
        };

        self.jobs.insert(server_id, job.clone());
        self.broadcast_progress(&job);

        Ok(job)
    }

    pub fn update_stage(&self, lookup: UpgradeLookup, stage: UpgradeStage) -> Option<UpgradeJob> {
        let mut job = self.jobs.get_mut(&lookup.server_id)?;
        if job.status != UpgradeStatus::Running || !lookup.matches(&job) {
            return None;
        }
        if job.stage == stage {
            return Some(job.clone());
        }

        job.stage = stage;
        let updated = job.clone();
        drop(job);
        self.broadcast_progress(&updated);

        Some(updated)
    }

    pub fn mark_failed(
        &self,
        lookup: UpgradeLookup,
        stage: UpgradeStage,
        error: String,
        backup_path: Option<String>,
    ) -> Option<UpgradeJob> {
        self.finish_job(
            lookup,
            UpgradeStatus::Failed,
            Some(stage),
            Some(error),
            backup_path,
        )
    }

    pub fn mark_failed_by_capability_denied(
        &self,
        lookup: UpgradeLookup,
        reason: CapabilityDeniedReason,
    ) -> Option<UpgradeJob> {
        self.finish_job(
            lookup,
            UpgradeStatus::Failed,
            None,
            Some(format!("Upgrade capability denied: {reason:?}")),
            None,
        )
    }

    pub fn mark_succeeded(
        &self,
        lookup: UpgradeLookup,
        backup_path: Option<String>,
    ) -> Option<UpgradeJob> {
        self.finish_job(lookup, UpgradeStatus::Succeeded, None, None, backup_path)
    }

    pub fn sweep_timeouts(&self, now: DateTime<Utc>) -> Vec<UpgradeJob> {
        let mut timed_out = Vec::new();
        let timeout_cutoff = now - Duration::seconds(UPGRADE_TIMEOUT_SECS);

        for mut entry in self.jobs.iter_mut() {
            if entry.status != UpgradeStatus::Running || entry.started_at > timeout_cutoff {
                continue;
            }

            entry.status = UpgradeStatus::Timeout;
            entry.error = Some(format!("Upgrade timed out after {UPGRADE_TIMEOUT_SECS}s"));
            entry.finished_at = Some(now);

            let job = entry.clone();
            timed_out.push(job.clone());
            drop(entry);
            self.broadcast_result(&job);
        }

        timed_out
    }

    pub fn cleanup_old(&self, now: DateTime<Utc>) -> usize {
        let retention_cutoff = now - Duration::hours(UPGRADE_RETENTION_HOURS);
        let before = self.jobs.len();

        self.jobs.retain(|_, job| {
            !(job.is_terminal()
                && job
                    .finished_at
                    .is_some_and(|finished_at| finished_at <= retention_cutoff))
        });

        before.saturating_sub(self.jobs.len())
    }

    pub fn get(&self, server_id: &str) -> Option<UpgradeJob> {
        self.jobs.get(server_id).map(|job| job.clone())
    }

    pub fn snapshot(&self) -> Vec<UpgradeJobDto> {
        self.jobs.iter().map(|job| job.to_dto()).collect::<Vec<_>>()
    }

    fn finish_job(
        &self,
        lookup: UpgradeLookup,
        status: UpgradeStatus,
        stage: Option<UpgradeStage>,
        error: Option<String>,
        backup_path: Option<String>,
    ) -> Option<UpgradeJob> {
        let mut job = self.jobs.get_mut(&lookup.server_id)?;
        if job.status != UpgradeStatus::Running || !lookup.matches(&job) {
            return None;
        }

        if let Some(stage) = stage {
            job.stage = stage;
        }
        job.status = status;
        job.error = error;
        job.backup_path = backup_path;
        job.finished_at = Some(Utc::now());

        let updated = job.clone();
        drop(job);
        self.broadcast_result(&updated);

        Some(updated)
    }

    fn broadcast_progress(&self, job: &UpgradeJob) {
        let _ = self.browser_tx.send(BrowserMessage::UpgradeProgress {
            server_id: job.server_id.clone(),
            job_id: job.job_id.clone(),
            target_version: job.target_version.clone(),
            stage: job.stage,
        });
    }

    fn broadcast_result(&self, job: &UpgradeJob) {
        let _ = self.browser_tx.send(BrowserMessage::UpgradeResult {
            server_id: job.server_id.clone(),
            job_id: job.job_id.clone(),
            target_version: job.target_version.clone(),
            status: job.status,
            stage: Some(job.stage),
            error: job.error.clone(),
            backup_path: job.backup_path.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use serverbee_common::constants::CapabilityDeniedReason;
    use serverbee_common::protocol::{BrowserMessage, UpgradeJobDto, UpgradeStage, UpgradeStatus};
    use tokio::sync::broadcast;

    use super::*;

    fn make_tracker() -> (UpgradeJobTracker, broadcast::Receiver<BrowserMessage>) {
        let (tx, rx) = broadcast::channel(16);
        (UpgradeJobTracker::new(tx), rx)
    }

    fn assert_progress(
        msg: BrowserMessage,
        server_id: &str,
        job_id: &str,
        target_version: &str,
        stage: UpgradeStage,
    ) {
        assert!(matches!(
            msg,
            BrowserMessage::UpgradeProgress {
                server_id: ref actual_server_id,
                job_id: ref actual_job_id,
                target_version: ref actual_target_version,
                stage: actual_stage,
            } if actual_server_id == server_id
                && actual_job_id == job_id
                && actual_target_version == target_version
                && actual_stage == stage
        ));
    }

    fn assert_result(
        msg: BrowserMessage,
        server_id: &str,
        job_id: &str,
        target_version: &str,
        status: UpgradeStatus,
        stage: Option<UpgradeStage>,
    ) {
        assert!(matches!(
            msg,
            BrowserMessage::UpgradeResult {
                server_id: ref actual_server_id,
                job_id: ref actual_job_id,
                target_version: ref actual_target_version,
                status: actual_status,
                stage: actual_stage,
                ..
            } if actual_server_id == server_id
                && actual_job_id == job_id
                && actual_target_version == target_version
                && actual_status == status
                && actual_stage == stage
        ));
    }

    #[test]
    fn start_job_rejects_a_second_running_job() {
        let (tracker, mut rx) = make_tracker();

        let first = tracker
            .start_job("server-1", "1.2.3")
            .expect("first job should start");
        assert_progress(
            rx.try_recv().expect("start should broadcast progress"),
            "server-1",
            &first.job_id,
            "1.2.3",
            UpgradeStage::Downloading,
        );

        let conflict = tracker
            .start_job("server-1", "1.2.4")
            .expect_err("second running job should be rejected");

        match conflict {
            StartUpgradeJobError::Conflict(existing) => {
                assert_eq!(existing.job_id, first.job_id);
                assert_eq!(existing.target_version, "1.2.3");
                assert_eq!(existing.status, UpgradeStatus::Running);
            }
        }

        assert!(rx.try_recv().is_err(), "conflict should not broadcast");
    }

    #[test]
    fn update_stage_prefers_job_id_and_ignores_stale_messages() {
        let (tracker, mut rx) = make_tracker();

        let stale = tracker
            .start_job("server-1", "1.2.3")
            .expect("stale job should start");
        rx.try_recv().expect("start broadcast");
        tracker.mark_failed(
            UpgradeLookup::from_job(&stale),
            UpgradeStage::Installing,
            "boom".into(),
            None,
        );
        rx.try_recv().expect("failure broadcast");

        let active = tracker
            .start_job("server-1", "1.2.3")
            .expect("replacement job should start");
        rx.try_recv().expect("replacement start broadcast");

        tracker.update_stage(
            UpgradeLookup {
                server_id: "server-1".into(),
                job_id: Some(stale.job_id.clone()),
                target_version: "1.2.3".into(),
            },
            UpgradeStage::Verifying,
        );

        assert!(rx.try_recv().is_err(), "stale update should be ignored");

        let current = tracker.get("server-1").expect("active job should remain");
        assert_eq!(current.job_id, active.job_id);
        assert_eq!(current.stage, UpgradeStage::Downloading);

        tracker.update_stage(UpgradeLookup::from_job(&active), UpgradeStage::Verifying);

        assert_progress(
            rx.try_recv().expect("active update should broadcast"),
            "server-1",
            &active.job_id,
            "1.2.3",
            UpgradeStage::Verifying,
        );
    }

    #[test]
    fn mark_succeeded_does_not_overwrite_timeout() {
        let (tracker, mut rx) = make_tracker();

        let job = tracker
            .start_job("server-1", "1.2.3")
            .expect("job should start");
        rx.try_recv().expect("start broadcast");

        let timed_out =
            tracker.sweep_timeouts(Utc::now() + Duration::seconds(UPGRADE_TIMEOUT_SECS + 1));
        assert_eq!(timed_out.len(), 1);

        let timeout_msg = rx.try_recv().expect("timeout broadcast");
        assert_result(
            timeout_msg,
            "server-1",
            &job.job_id,
            "1.2.3",
            UpgradeStatus::Timeout,
            Some(UpgradeStage::Downloading),
        );

        tracker.mark_succeeded(UpgradeLookup::from_job(&job), Some("/tmp/backup".into()));

        assert!(rx.try_recv().is_err(), "late success should not broadcast");

        let current = tracker.get("server-1").expect("job should still exist");
        assert_eq!(current.status, UpgradeStatus::Timeout);
        assert_eq!(current.backup_path.as_deref(), None);
        assert!(current.finished_at.is_some());
    }

    #[test]
    fn cleanup_old_removes_only_expired_terminal_jobs() {
        let (tracker, _rx) = make_tracker();
        let now = Utc::now();
        let expired = now - Duration::hours(UPGRADE_RETENTION_HOURS + 1);
        let fresh = now - Duration::hours(1);

        tracker.jobs.insert(
            "expired-succeeded".into(),
            UpgradeJob {
                server_id: "expired-succeeded".into(),
                job_id: "job-expired-succeeded".into(),
                target_version: "1.0.0".into(),
                stage: UpgradeStage::Restarting,
                status: UpgradeStatus::Succeeded,
                error: None,
                backup_path: None,
                started_at: expired,
                finished_at: Some(expired),
            },
        );
        tracker.jobs.insert(
            "expired-timeout".into(),
            UpgradeJob {
                server_id: "expired-timeout".into(),
                job_id: "job-expired-timeout".into(),
                target_version: "1.0.0".into(),
                stage: UpgradeStage::Installing,
                status: UpgradeStatus::Timeout,
                error: Some("timed out".into()),
                backup_path: None,
                started_at: expired,
                finished_at: Some(expired),
            },
        );
        tracker.jobs.insert(
            "fresh-failed".into(),
            UpgradeJob {
                server_id: "fresh-failed".into(),
                job_id: "job-fresh-failed".into(),
                target_version: "1.0.0".into(),
                stage: UpgradeStage::Installing,
                status: UpgradeStatus::Failed,
                error: Some("failed".into()),
                backup_path: None,
                started_at: fresh,
                finished_at: Some(fresh),
            },
        );
        tracker.jobs.insert(
            "still-running".into(),
            UpgradeJob {
                server_id: "still-running".into(),
                job_id: "job-still-running".into(),
                target_version: "1.0.0".into(),
                stage: UpgradeStage::Installing,
                status: UpgradeStatus::Running,
                error: None,
                backup_path: None,
                started_at: expired,
                finished_at: None,
            },
        );

        let removed = tracker.cleanup_old(now);
        assert_eq!(removed, 2);

        assert!(tracker.get("expired-succeeded").is_none());
        assert!(tracker.get("expired-timeout").is_none());
        assert!(tracker.get("fresh-failed").is_some());
        assert!(tracker.get("still-running").is_some());
    }

    #[test]
    fn mark_failed_by_capability_denied_sets_failed_result() {
        let (tracker, mut rx) = make_tracker();

        let job = tracker
            .start_job("server-1", "1.2.3")
            .expect("job should start");
        rx.try_recv().expect("start broadcast");

        tracker.mark_failed_by_capability_denied(
            UpgradeLookup::from_job(&job),
            CapabilityDeniedReason::AgentCapabilityDisabled,
        );

        let current = tracker.get("server-1").expect("job should remain");
        assert_eq!(current.status, UpgradeStatus::Failed);
        assert!(
            current
                .error
                .expect("failure error should be recorded")
                .contains("AgentCapabilityDisabled")
        );
        assert_result(
            rx.try_recv()
                .expect("capability denied should broadcast result"),
            "server-1",
            &job.job_id,
            "1.2.3",
            UpgradeStatus::Failed,
            Some(UpgradeStage::Downloading),
        );
    }

    #[test]
    fn snapshot_returns_upgrade_job_dtos() {
        let (tracker, _rx) = make_tracker();

        let job = tracker
            .start_job("server-1", "1.2.3")
            .expect("job should start");

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(
            snapshot[0],
            UpgradeJobDto {
                server_id: "server-1".into(),
                job_id: job.job_id,
                target_version: "1.2.3".into(),
                stage: UpgradeStage::Downloading,
                status: UpgradeStatus::Running,
                error: None,
                backup_path: None,
                started_at: snapshot[0].started_at,
                finished_at: None,
            }
        );
    }
}
