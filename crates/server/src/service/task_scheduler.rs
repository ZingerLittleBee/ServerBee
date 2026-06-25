use std::sync::Arc;

use dashmap::DashMap;
use tokio_cron_scheduler::{Job, JobScheduler};
use tokio_util::sync::CancellationToken;

use crate::error::AppError;

pub struct TaskScheduler {
    scheduler: JobScheduler,
    job_map: DashMap<String, uuid::Uuid>,
    /// task_id -> (run_id, cancellation token). Arc so the cleanup guard shares the real map.
    pub(crate) active_runs: Arc<DashMap<String, (String, CancellationToken)>>,
    timezone: String,
}

impl TaskScheduler {
    pub async fn new(timezone: &str) -> Result<Self, AppError> {
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create scheduler: {e}")))?;
        Ok(Self {
            scheduler,
            job_map: DashMap::new(),
            active_runs: Arc::new(DashMap::new()),
            timezone: timezone.to_string(),
        })
    }

    pub fn is_running(&self, task_id: &str) -> bool {
        self.active_runs.contains_key(task_id)
    }

    pub fn cancel_active_run(&self, task_id: &str) {
        if let Some((_, (_, token))) = self.active_runs.remove(task_id) {
            token.cancel();
        }
    }

    pub fn timezone(&self) -> &str {
        &self.timezone
    }

    pub async fn start(&self) -> Result<(), AppError> {
        self.scheduler
            .start()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to start scheduler: {e}")))?;
        Ok(())
    }

    pub async fn add_job(
        &self,
        task_model: &crate::entity::task::Model,
        state: std::sync::Arc<crate::state::AppState>,
    ) -> Result<(), AppError> {
        let cron = task_model
            .cron_expression
            .as_deref()
            .ok_or_else(|| AppError::BadRequest("Missing cron_expression".into()))?;
        let task_id = task_model.id.clone();

        let tz: chrono_tz::Tz = self
            .timezone
            .parse()
            .map_err(|_| AppError::Internal(format!("Invalid timezone: {}", self.timezone)))?;

        let job = Job::new_async_tz(cron, tz, move |_uuid, _lock| {
            let state = state.clone();
            let task_id = task_id.clone();
            Box::pin(async move {
                crate::task::task_scheduler::execute_scheduled_task(&state, &task_id, false, None)
                    .await;
            })
        })
        .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {e}")))?;

        let job_id = job.guid();
        self.scheduler
            .add(job)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to add job: {e}")))?;
        self.job_map.insert(task_model.id.clone(), job_id);
        Ok(())
    }

    pub async fn remove_job(&self, task_id: &str) -> Result<(), AppError> {
        self.cancel_active_run(task_id);
        if let Some((_, job_id)) = self.job_map.remove(task_id) {
            self.scheduler
                .remove(&job_id)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to remove job: {e}")))?;
        }
        Ok(())
    }

    pub async fn update_job(
        &self,
        task_model: &crate::entity::task::Model,
        state: std::sync::Arc<crate::state::AppState>,
    ) -> Result<(), AppError> {
        self.remove_job(&task_model.id).await?;
        if task_model.enabled {
            self.add_job(task_model, state).await?;
        }
        Ok(())
    }

    pub async fn disable_job(&self, task_id: &str) -> Result<(), AppError> {
        self.remove_job(task_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_scheduler() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        assert!(!scheduler.is_running("nonexistent"));
    }

    #[tokio::test]
    async fn test_overlap_detection() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let token = CancellationToken::new();
        scheduler
            .active_runs
            .insert("task-1".to_string(), ("run-1".to_string(), token));
        assert!(scheduler.is_running("task-1"));
        assert!(!scheduler.is_running("task-2"));
    }

    #[tokio::test]
    async fn test_cancel_active_run() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let token = CancellationToken::new();
        let token_clone = token.clone();
        scheduler
            .active_runs
            .insert("task-1".to_string(), ("run-1".to_string(), token));
        scheduler.cancel_active_run("task-1");
        assert!(token_clone.is_cancelled());
        assert!(!scheduler.is_running("task-1"));
    }

    // ---- Helpers -------------------------------------------------------

    /// A cron expression that is structurally valid for the scheduler's
    /// `croner` parser (6 fields with seconds required: sec min hour
    /// day-of-month month day-of-week) and will not fire during a test run:
    /// midnight on Jan 1. Registering it is enough to exercise the bookkeeping
    /// paths; the job body never runs within a sub-second test.
    const FAR_FUTURE_CRON: &str = "0 0 0 1 1 *";

    /// Build an `Arc<AppState>` backed by a fresh migrated test DB. The
    /// returned `TempDir` guards must be kept alive for the test duration:
    /// one owns the SQLite file, the other the scheduler/data dir.
    async fn build_test_state() -> (
        std::sync::Arc<crate::state::AppState>,
        tempfile::TempDir,
        tempfile::TempDir,
    ) {
        let (db, db_guard) = crate::test_utils::setup_test_db().await;
        let data_dir = tempfile::TempDir::new().unwrap();
        let mut config = crate::config::AppConfig::default();
        // Redirect data_dir to a tempdir so GeoIP/ASN/file-transfer paths
        // never touch the real `./data` working directory.
        config.server.data_dir = data_dir.path().to_str().unwrap().to_string();
        let state = crate::state::AppState::new(db, config).await.unwrap();
        (state, db_guard, data_dir)
    }

    /// Construct a minimal scheduled-task model. The caller controls the
    /// fields that drive scheduling branches (cron, enabled).
    fn make_task(id: &str, cron: Option<&str>, enabled: bool) -> crate::entity::task::Model {
        crate::entity::task::Model {
            id: id.to_string(),
            command: "echo hi".to_string(),
            server_ids_json: "[]".to_string(),
            created_by: "tester".to_string(),
            task_type: "scheduled".to_string(),
            name: Some("test task".to_string()),
            cron_expression: cron.map(|c| c.to_string()),
            enabled,
            timeout: None,
            retry_count: 0,
            retry_interval: 0,
            last_run_at: None,
            next_run_at: None,
            created_at: chrono::Utc::now(),
        }
    }

    // ---- accessors / basic state --------------------------------------

    #[tokio::test]
    async fn test_timezone_accessor() {
        let scheduler = TaskScheduler::new("Asia/Shanghai").await.unwrap();
        assert_eq!(scheduler.timezone(), "Asia/Shanghai");
    }

    #[tokio::test]
    async fn test_start_succeeds() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        // Starting a freshly-created scheduler must succeed.
        scheduler.start().await.unwrap();
    }

    #[tokio::test]
    async fn test_cancel_active_run_missing_is_noop() {
        // Cancelling a task that is not running must not panic and leaves
        // the map untouched.
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        scheduler.cancel_active_run("does-not-exist");
        assert!(!scheduler.is_running("does-not-exist"));
    }

    // ---- add_job error branches (do not require a live AppState path) --

    #[tokio::test]
    async fn test_add_job_missing_cron_expression() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let task = make_task("t-no-cron", None, true);
        let err = scheduler.add_job(&task, state).await.unwrap_err();
        match err {
            AppError::BadRequest(msg) => assert!(msg.contains("Missing cron_expression"), "{msg}"),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_add_job_invalid_timezone() {
        let (state, _db, _dir) = build_test_state().await;
        // Construction does not validate the tz; add_job does.
        let scheduler = TaskScheduler::new("Not/AZone").await.unwrap();
        let task = make_task("t-bad-tz", Some(FAR_FUTURE_CRON), true);
        let err = scheduler.add_job(&task, state).await.unwrap_err();
        match err {
            AppError::Internal(msg) => assert!(msg.contains("Invalid timezone"), "{msg}"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_add_job_invalid_cron_expression() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let task = make_task("t-bad-cron", Some("this is not cron"), true);
        let err = scheduler.add_job(&task, state).await.unwrap_err();
        match err {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("Invalid cron expression"), "{msg}")
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // ---- add_job success + job_map bookkeeping -------------------------

    #[tokio::test]
    async fn test_add_job_success_registers_in_map() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let task = make_task("t-ok", Some(FAR_FUTURE_CRON), true);
        scheduler.add_job(&task, state).await.unwrap();
        // The job must now be tracked in the internal job map.
        assert!(scheduler.job_map.contains_key("t-ok"));
    }

    // ---- remove_job ----------------------------------------------------

    #[tokio::test]
    async fn test_remove_job_unknown_is_ok() {
        // Removing a task that was never added is a no-op success.
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        scheduler.remove_job("never-added").await.unwrap();
        assert!(!scheduler.job_map.contains_key("never-added"));
    }

    #[tokio::test]
    async fn test_remove_job_cancels_active_run_and_clears_map() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let task = make_task("t-remove", Some(FAR_FUTURE_CRON), true);
        scheduler.add_job(&task, state).await.unwrap();
        assert!(scheduler.job_map.contains_key("t-remove"));

        // Simulate an in-flight run to exercise the cancel_active_run path.
        let token = CancellationToken::new();
        let token_clone = token.clone();
        scheduler
            .active_runs
            .insert("t-remove".to_string(), ("run-x".to_string(), token));

        scheduler.remove_job("t-remove").await.unwrap();

        assert!(token_clone.is_cancelled(), "active run should be cancelled");
        assert!(!scheduler.is_running("t-remove"));
        assert!(!scheduler.job_map.contains_key("t-remove"));
    }

    // ---- update_job (enable/disable branches) -------------------------

    #[tokio::test]
    async fn test_update_job_enabled_reregisters() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let task = make_task("t-update", Some(FAR_FUTURE_CRON), true);
        // First registration.
        scheduler.add_job(&task, state.clone()).await.unwrap();
        // update_job removes then re-adds because enabled == true.
        scheduler.update_job(&task, state).await.unwrap();
        assert!(scheduler.job_map.contains_key("t-update"));
    }

    #[tokio::test]
    async fn test_update_job_disabled_removes_without_readd() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let enabled = make_task("t-toggle", Some(FAR_FUTURE_CRON), true);
        scheduler.add_job(&enabled, state.clone()).await.unwrap();
        assert!(scheduler.job_map.contains_key("t-toggle"));

        // Same task id but now disabled: update_job must remove and skip add.
        let disabled = make_task("t-toggle", Some(FAR_FUTURE_CRON), false);
        scheduler.update_job(&disabled, state).await.unwrap();
        assert!(!scheduler.job_map.contains_key("t-toggle"));
    }

    #[tokio::test]
    async fn test_update_job_disabled_when_never_registered() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        // Updating a disabled task that was never registered is a clean no-op.
        let task = make_task("t-fresh-disabled", Some(FAR_FUTURE_CRON), false);
        scheduler.update_job(&task, state).await.unwrap();
        assert!(!scheduler.job_map.contains_key("t-fresh-disabled"));
    }

    #[tokio::test]
    async fn test_update_job_enabled_propagates_invalid_cron_error() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        // enabled == true forces the add_job path, whose cron validation fails.
        let task = make_task("t-update-bad", Some("nonsense cron"), true);
        let err = scheduler.update_job(&task, state).await.unwrap_err();
        match err {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("Invalid cron expression"), "{msg}")
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // ---- disable_job ---------------------------------------------------

    #[tokio::test]
    async fn test_disable_job_removes_registered_job() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let task = make_task("t-disable", Some(FAR_FUTURE_CRON), true);
        scheduler.add_job(&task, state).await.unwrap();
        assert!(scheduler.job_map.contains_key("t-disable"));

        scheduler.disable_job("t-disable").await.unwrap();
        assert!(!scheduler.job_map.contains_key("t-disable"));
    }

    #[tokio::test]
    async fn test_disable_job_unknown_is_ok() {
        // Disabling an unknown task delegates to remove_job and must succeed.
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        scheduler.disable_job("nope").await.unwrap();
        assert!(!scheduler.is_running("nope"));
    }
}
