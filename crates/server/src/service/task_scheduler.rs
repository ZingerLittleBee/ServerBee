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
                crate::task::task_scheduler::execute_scheduled_task(&state, &task_id, false).await;
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
}
