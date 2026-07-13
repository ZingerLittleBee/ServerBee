use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde::Deserialize;
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio::task::JoinSet;
use tokio_cron_scheduler::{Job, JobScheduler};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::entity::{server, task, task_result};
use crate::error::AppError;
use crate::service::agent_manager::AgentRequestError;
use crate::service::audit::AuditService;
use crate::service::high_risk_audit::ExecAuditContext;
use crate::state::AppState;
use serverbee_common::constants::CAP_EXEC;
use serverbee_common::protocol::{AgentMessage, ServerMessage};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateTaskRequest {
    pub command: String,
    pub server_ids: Vec<String>,
    #[serde(default)]
    pub timeout: Option<u32>,
    /// "oneshot" (default) or "scheduled"
    #[serde(default = "default_oneshot")]
    pub task_type: TaskType,
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    #[serde(default)]
    pub retry_count: Option<i32>,
    #[serde(default)]
    pub retry_interval: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    Oneshot,
    Scheduled,
}

impl TaskType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Oneshot => "oneshot",
            Self::Scheduled => "scheduled",
        }
    }
}

impl fmt::Display for TaskType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn default_oneshot() -> TaskType {
    TaskType::Oneshot
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub server_ids: Option<Vec<String>>,
    pub cron_expression: Option<String>,
    pub enabled: Option<bool>,
    pub timeout: Option<i32>,
    pub retry_count: Option<i32>,
    pub retry_interval: Option<i32>,
}

struct ActiveRun {
    run_id: String,
    cancellation: CancellationToken,
    completed: CancellationToken,
}

type ActiveRuns = DashMap<String, ActiveRun>;

fn clear_active_run_if_current(active_runs: &ActiveRuns, task_id: &str, run_id: &str) -> bool {
    active_runs
        .remove_if(task_id, |_, active_run| active_run.run_id == run_id)
        .is_some()
}

struct ActiveRunGuard {
    active_runs: Arc<ActiveRuns>,
    task_id: String,
    state: Arc<AppState>,
    run_id: String,
    completed: CancellationToken,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        clear_active_run_if_current(&self.active_runs, &self.task_id, &self.run_id);
        self.state.exec_audit_contexts.remove(&self.run_id);
        self.completed.cancel();
    }
}

pub struct TaskScheduler {
    scheduler: JobScheduler,
    job_map: DashMap<String, uuid::Uuid>,
    /// Active task executions. Arc so cleanup guards share the real map.
    active_runs: Arc<ActiveRuns>,
    lifecycle_lock: Arc<Mutex<()>>,
    timezone: chrono_tz::Tz,
}

impl TaskScheduler {
    pub async fn new(timezone: &str) -> Result<Self, AppError> {
        let timezone = timezone
            .parse()
            .map_err(|_| AppError::Internal(format!("Invalid timezone: {timezone}")))?;
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create scheduler: {e}")))?;
        Ok(Self {
            scheduler,
            job_map: DashMap::new(),
            active_runs: Arc::new(DashMap::new()),
            lifecycle_lock: Arc::new(Mutex::new(())),
            timezone,
        })
    }

    #[cfg(test)]
    fn is_running(&self, task_id: &str) -> bool {
        self.active_runs.contains_key(task_id)
    }

    fn cancel_active_run(&self, task_id: &str) -> Option<CancellationToken> {
        if let Some(active_run) = self.active_runs.get(task_id) {
            active_run.cancellation.cancel();
            return Some(active_run.completed.clone());
        }
        None
    }

    async fn cancel_and_wait_active_run(&self, task_id: &str) {
        if let Some(completed) = self.cancel_active_run(task_id) {
            completed.cancelled().await;
        }
    }

    fn tz(&self) -> chrono_tz::Tz {
        self.timezone
    }

    fn next_run_at(&self, cron_expr: &str) -> Option<DateTime<Utc>> {
        let mut job = Job::new_tz(cron_expr, self.tz(), |_uuid, _lock| {}).ok()?;
        job.job_data().ok()?.next_tick_utc()
    }

    fn is_current_job(&self, task_id: &str, job_id: uuid::Uuid) -> bool {
        self.job_map
            .get(task_id)
            .is_some_and(|current_job_id| *current_job_id == job_id)
    }

    async fn start(&self) -> Result<(), AppError> {
        self.scheduler
            .start()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to start scheduler: {e}")))?;
        Ok(())
    }

    async fn add_job(
        &self,
        task_model: &task::Model,
        state: Arc<AppState>,
    ) -> Result<(), AppError> {
        if self.job_map.contains_key(&task_model.id) {
            return Err(AppError::Internal(format!(
                "Scheduled task {} is already registered",
                task_model.id
            )));
        }
        let cron = task_model
            .cron_expression
            .as_deref()
            .ok_or_else(|| AppError::BadRequest("Missing cron_expression".into()))?;
        let task_id = task_model.id.clone();

        let job = Job::new_async_tz(cron, self.tz(), move |job_id, _lock| {
            let state = state.clone();
            let task_id = task_id.clone();
            Box::pin(async move {
                let _lifecycle_guard = state.task_scheduler.lifecycle_lock.lock().await;
                if !state.task_scheduler.is_current_job(&task_id, job_id) {
                    tracing::debug!(
                        task_id,
                        %job_id,
                        "Ignoring stale scheduled task callback"
                    );
                    return;
                }
                match execute_scheduled_task(&state, &task_id, false, None).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::warn!("Task {task_id} still running, skipping cron trigger");
                    }
                    Err(error) => {
                        tracing::error!("Failed to start scheduled task {task_id}: {error}");
                    }
                }
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

    async fn remove_job(&self, task_id: &str) -> Result<(), AppError> {
        let job_id = self
            .job_map
            .get(task_id)
            .map(|entry| entry.value().to_owned());
        let remove_result = if let Some(job_id) = job_id {
            self.scheduler
                .remove(&job_id)
                .await
                .map_err(|error| AppError::Internal(format!("Failed to remove job: {error}")))
        } else {
            Ok(())
        };
        self.cancel_and_wait_active_run(task_id).await;
        remove_result?;
        self.job_map.remove(task_id);
        Ok(())
    }

    async fn update_job(
        &self,
        task_model: &task::Model,
        state: Arc<AppState>,
    ) -> Result<(), AppError> {
        self.remove_job(&task_model.id).await?;
        if task_model.enabled {
            self.add_job(task_model, state).await?;
        }
        Ok(())
    }

    async fn restore_job(
        &self,
        task_model: &task::Model,
        state: Arc<AppState>,
    ) -> Result<(), AppError> {
        if task_model.task_type == "scheduled"
            && task_model.enabled
            && !self.job_map.contains_key(&task_model.id)
        {
            self.add_job(task_model, state).await?;
        }
        Ok(())
    }
}

pub fn validate_cron(expr: &str) -> Result<(), AppError> {
    Job::new_tz(expr, chrono_tz::UTC, |_uuid, _lock| {})
        .map(|_| ())
        .map_err(|error| AppError::Validation(format!("Invalid cron expression: {error}")))
}

pub async fn create_task(
    state: &Arc<AppState>,
    input: CreateTaskRequest,
    created_by: &str,
    ip: &str,
) -> Result<task::Model, AppError> {
    let CreateTaskRequest {
        command,
        server_ids,
        timeout,
        task_type,
        name,
        cron_expression,
        retry_count,
        retry_interval,
    } = input;

    if server_ids.is_empty() {
        return Err(AppError::Validation(
            "server_ids cannot be empty".to_string(),
        ));
    }
    if command.trim().is_empty() {
        return Err(AppError::Validation("command cannot be empty".to_string()));
    }

    let is_scheduled = task_type == TaskType::Scheduled;
    if is_scheduled {
        let cron = cron_expression.as_deref().ok_or_else(|| {
            AppError::Validation("cron_expression is required for scheduled tasks".into())
        })?;
        validate_cron(cron)?;
    }

    let timeout_i32 = match timeout {
        Some(0) => return Err(AppError::Validation("timeout must be > 0".into())),
        Some(value) => Some(
            i32::try_from(value)
                .map_err(|_| AppError::Validation("timeout exceeds supported range".into()))?,
        ),
        None => None,
    };
    if let Some(count) = retry_count
        && !(0..=10).contains(&count)
    {
        return Err(AppError::Validation(
            "retry_count must be between 0 and 10".into(),
        ));
    }
    if matches!(retry_interval, Some(interval) if interval < 1) {
        return Err(AppError::Validation("retry_interval must be >= 1".into()));
    }

    let _lifecycle_guard = if is_scheduled {
        Some(state.task_scheduler.lifecycle_lock.lock().await)
    } else {
        None
    };
    let task_id = Uuid::new_v4().to_string();
    let server_ids_json = serde_json::to_string(&server_ids)
        .map_err(|error| AppError::Internal(format!("Serialization error: {error}")))?;
    let next_run_at = cron_expression
        .as_deref()
        .and_then(|expr| state.task_scheduler.next_run_at(expr));
    let task_model = task::ActiveModel {
        id: Set(task_id.clone()),
        command: Set(command.clone()),
        server_ids_json: Set(server_ids_json),
        created_by: Set(created_by.to_string()),
        task_type: Set(task_type.as_str().to_string()),
        name: Set(name),
        cron_expression: Set(cron_expression),
        enabled: Set(true),
        timeout: Set(timeout_i32),
        retry_count: Set(retry_count.unwrap_or(0)),
        retry_interval: Set(retry_interval.unwrap_or(60)),
        last_run_at: NotSet,
        next_run_at: Set(next_run_at),
        created_at: Set(Utc::now()),
    }
    .insert(&state.db)
    .await?;

    if is_scheduled {
        // The row is persisted before the scheduler's asynchronous mutation.
        // Keep both sides in sync if registration fails after validation.
        if let Err(error) = state
            .task_scheduler
            .add_job(&task_model, state.clone())
            .await
        {
            if let Err(rollback_error) = task::Entity::delete_by_id(&task_id).exec(&state.db).await
            {
                tracing::error!(
                    task_id,
                    "Failed to remove task row after scheduler registration error: {rollback_error}"
                );
            }
            return Err(error);
        }
        tracing::info!("Scheduled task {} registered", task_id);
    } else {
        dispatch_oneshot(
            state,
            &task_id,
            &command,
            &server_ids,
            timeout,
            created_by,
            ip,
        )
        .await?;
    }

    Ok(task_model)
}

pub async fn update_task(
    state: &Arc<AppState>,
    id: &str,
    input: UpdateTaskRequest,
) -> Result<task::Model, AppError> {
    let _lifecycle_guard = state.task_scheduler.lifecycle_lock.lock().await;
    let existing = task::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {id} not found")))?;
    let existing_backup = existing.clone();
    let mut model: task::ActiveModel = existing.into();

    let UpdateTaskRequest {
        name,
        command,
        server_ids,
        cron_expression,
        enabled,
        timeout,
        retry_count,
        retry_interval,
    } = input;

    if let Some(name) = name {
        model.name = Set(Some(name));
    }
    if let Some(command) = command {
        model.command = Set(command);
    }
    if let Some(server_ids) = server_ids {
        let json = serde_json::to_string(&server_ids)
            .map_err(|error| AppError::Internal(format!("Serialization error: {error}")))?;
        model.server_ids_json = Set(json);
    }
    if let Some(cron) = cron_expression.as_deref() {
        validate_cron(cron)?;
        model.cron_expression = Set(Some(cron.to_string()));
        model.next_run_at = Set(state.task_scheduler.next_run_at(cron));
    }
    if let Some(enabled) = enabled {
        model.enabled = Set(enabled);
        if enabled {
            let cron = cron_expression
                .as_deref()
                .or(existing_backup.cron_expression.as_deref());
            if let Some(cron) = cron {
                model.next_run_at = Set(state.task_scheduler.next_run_at(cron));
            }
        }
    }
    if let Some(timeout) = timeout {
        if timeout < 1 {
            return Err(AppError::Validation("timeout must be >= 1".into()));
        }
        model.timeout = Set(Some(timeout));
    }
    if let Some(retry_count) = retry_count {
        if !(0..=10).contains(&retry_count) {
            return Err(AppError::Validation(
                "retry_count must be between 0 and 10".into(),
            ));
        }
        model.retry_count = Set(retry_count);
    }
    if let Some(retry_interval) = retry_interval {
        if retry_interval < 1 {
            return Err(AppError::Validation("retry_interval must be >= 1".into()));
        }
        model.retry_interval = Set(retry_interval);
    }

    let updated = model.update(&state.db).await?;
    if updated.task_type == "scheduled"
        && let Err(error) = state
            .task_scheduler
            .update_job(&updated, state.clone())
            .await
    {
        let rollback: task::ActiveModel = existing_backup.clone().into();
        match rollback.update(&state.db).await {
            Ok(restored) => {
                if let Err(restore_error) = state
                    .task_scheduler
                    .restore_job(&restored, state.clone())
                    .await
                {
                    tracing::error!(
                        task_id = id,
                        "Failed to restore scheduled job after update error: {restore_error}"
                    );
                }
            }
            Err(rollback_error) => {
                tracing::error!(
                    task_id = id,
                    "Failed to restore task row after scheduler update error: {rollback_error}"
                );
            }
        }
        return Err(error);
    }

    Ok(updated)
}

pub async fn delete_task(state: &Arc<AppState>, id: &str) -> Result<(), AppError> {
    let _lifecycle_guard = state.task_scheduler.lifecycle_lock.lock().await;
    let existing = task::Entity::find_by_id(id).one(&state.db).await?;
    state.task_scheduler.remove_job(id).await?;

    let transaction = match state.db.begin().await {
        Ok(transaction) => transaction,
        Err(error) => {
            if let Some(task_model) = existing.as_ref()
                && let Err(restore_error) = state
                    .task_scheduler
                    .restore_job(task_model, state.clone())
                    .await
            {
                tracing::error!(
                    task_id = id,
                    "Failed to restore scheduled job after transaction error: {restore_error}"
                );
            }
            return Err(error.into());
        }
    };
    let delete_result: Result<(), DbErr> = async {
        task_result::Entity::delete_many()
            .filter(task_result::Column::TaskId.eq(id))
            .exec(&transaction)
            .await?;
        task::Entity::delete_by_id(id).exec(&transaction).await?;
        transaction.commit().await?;
        Ok(())
    }
    .await;

    if let Err(error) = delete_result {
        if let Some(task_model) = existing.as_ref()
            && let Err(restore_error) = state
                .task_scheduler
                .restore_job(task_model, state.clone())
                .await
        {
            tracing::error!(
                task_id = id,
                "Failed to restore scheduled job after delete error: {restore_error}"
            );
        }
        return Err(error.into());
    }

    Ok(())
}

#[must_use = "the scheduler cleanup must run after the surrounding transaction commits"]
pub(crate) struct TaskServerCleanup {
    changed_task_ids: Vec<String>,
    deleted_task_ids: Vec<String>,
    orphan_server_ids: Vec<String>,
    _lifecycle_guard: OwnedMutexGuard<()>,
}

impl TaskServerCleanup {
    pub(crate) async fn remove_server_references(
        &mut self,
        transaction: &DatabaseTransaction,
        orphan_ids: &[String],
    ) -> Result<(), AppError> {
        self.orphan_server_ids = orphan_ids.to_vec();
        for task_model in task::Entity::find().all(transaction).await? {
            let mut server_ids: Vec<String> = serde_json::from_str(&task_model.server_ids_json)
                .map_err(|error| {
                    AppError::Internal(format!(
                        "Task {} has invalid server_ids_json: {error}",
                        task_model.id
                    ))
                })?;
            let previous_len = server_ids.len();
            server_ids.retain(|server_id| !orphan_ids.contains(server_id));
            if server_ids.len() == previous_len {
                continue;
            }
            self.changed_task_ids.push(task_model.id.clone());

            if server_ids.is_empty() {
                task_result::Entity::delete_many()
                    .filter(task_result::Column::TaskId.eq(&task_model.id))
                    .exec(transaction)
                    .await?;
                task::Entity::delete_by_id(&task_model.id)
                    .exec(transaction)
                    .await?;
                self.deleted_task_ids.push(task_model.id);
            } else {
                let mut active: task::ActiveModel = task_model.into();
                active.server_ids_json =
                    Set(serde_json::to_string(&server_ids).map_err(|error| {
                        AppError::Internal(format!("Failed to serialize task server IDs: {error}"))
                    })?);
                active.update(transaction).await?;
            }
        }

        Ok(())
    }

    pub(crate) async fn apply_after_commit(self, state: &Arc<AppState>) {
        let deleted_task_ids: std::collections::HashSet<&str> =
            self.deleted_task_ids.iter().map(String::as_str).collect();
        for task_id in &self.changed_task_ids {
            if deleted_task_ids.contains(task_id.as_str()) {
                if let Err(error) = state.task_scheduler.remove_job(task_id).await {
                    tracing::error!(
                        task_id,
                        "Failed to remove cleaned-up task from scheduler: {error}"
                    );
                }
            } else {
                state
                    .task_scheduler
                    .cancel_and_wait_active_run(task_id)
                    .await;
            }
        }

        if !self.deleted_task_ids.is_empty()
            && let Err(error) = task_result::Entity::delete_many()
                .filter(task_result::Column::TaskId.is_in(self.deleted_task_ids.clone()))
                .exec(&state.db)
                .await
        {
            tracing::error!("Failed to remove late results for deleted tasks: {error}");
        }
        if !self.orphan_server_ids.is_empty()
            && let Err(error) = task_result::Entity::delete_many()
                .filter(task_result::Column::ServerId.is_in(self.orphan_server_ids.clone()))
                .exec(&state.db)
                .await
        {
            tracing::error!("Failed to remove late results for cleaned-up servers: {error}");
        }
    }
}

pub(crate) async fn begin_server_cleanup(state: &Arc<AppState>) -> TaskServerCleanup {
    let lifecycle_guard = state
        .task_scheduler
        .lifecycle_lock
        .clone()
        .lock_owned()
        .await;
    TaskServerCleanup {
        changed_task_ids: Vec::new(),
        deleted_task_ids: Vec::new(),
        orphan_server_ids: Vec::new(),
        _lifecycle_guard: lifecycle_guard,
    }
}

pub async fn run_now(
    state: &Arc<AppState>,
    id: &str,
    audit_context: Option<ExecAuditContext>,
) -> Result<task::Model, AppError> {
    let _lifecycle_guard = state.task_scheduler.lifecycle_lock.lock().await;
    let task_model = task::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {id} not found")))?;
    if task_model.task_type != "scheduled" {
        return Err(AppError::BadRequest(
            "Only scheduled tasks can be manually triggered".into(),
        ));
    }

    if !execute_scheduled_task(state, id, true, audit_context).await? {
        return Err(AppError::Conflict(
            "Task is currently running, try again later".into(),
        ));
    }

    task::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Task not found".into()))
}

/// Restore enabled scheduled tasks from the database, then start the scheduler.
pub async fn restore_and_start(state: Arc<AppState>) {
    let _lifecycle_guard = state.task_scheduler.lifecycle_lock.lock().await;
    let tasks = task::Entity::find()
        .filter(task::Column::TaskType.eq("scheduled"))
        .filter(task::Column::Enabled.eq(true))
        .all(&state.db)
        .await;

    match tasks {
        Ok(tasks) => {
            for task_model in &tasks {
                if let Err(error) = state
                    .task_scheduler
                    .add_job(task_model, state.clone())
                    .await
                {
                    tracing::error!(
                        "Failed to register scheduled task {}: {error}",
                        task_model.id
                    );
                    continue;
                }
                let next_run_at = task_model
                    .cron_expression
                    .as_deref()
                    .and_then(|expr| state.task_scheduler.next_run_at(expr));
                if let Err(error) = task::Entity::update_many()
                    .filter(task::Column::Id.eq(&task_model.id))
                    .col_expr(task::Column::NextRunAt, Expr::value(next_run_at))
                    .exec(&state.db)
                    .await
                {
                    tracing::error!(
                        task_id = task_model.id,
                        "Failed to refresh next run during scheduler restore: {error}"
                    );
                }
            }
            tracing::info!("Loaded {} scheduled tasks", tasks.len());
        }
        Err(error) => {
            tracing::error!("Failed to load scheduled tasks: {error}");
        }
    }

    if let Err(error) = state.task_scheduler.start().await {
        tracing::error!("Failed to start task scheduler: {error}");
    }
}

/// Build correlation ID: {task_id}:{run_id}:{server_id}:{attempt}
fn build_correlation_id(task_id: &str, run_id: &str, server_id: &str, attempt: i32) -> String {
    format!("{task_id}:{run_id}:{server_id}:{attempt}")
}

/// Called by a cron trigger or the manual run interface.
/// Returns true if execution was started, false if an overlapping run won.
async fn execute_scheduled_task(
    state: &Arc<AppState>,
    task_id: &str,
    skip_retry: bool,
    audit_context: Option<ExecAuditContext>,
) -> Result<bool, AppError> {
    let scheduler = &state.task_scheduler;
    let run_id = Uuid::new_v4().to_string();
    let token = CancellationToken::new();
    let completed = CancellationToken::new();
    match scheduler.active_runs.entry(task_id.to_string()) {
        Entry::Occupied(_) => {
            tracing::warn!("Task {task_id} still running, skipping trigger");
            return Ok(false);
        }
        Entry::Vacant(entry) => {
            entry.insert(ActiveRun {
                run_id: run_id.clone(),
                cancellation: token.clone(),
                completed: completed.clone(),
            });
        }
    }

    let active_run_guard = ActiveRunGuard {
        active_runs: Arc::clone(&scheduler.active_runs),
        task_id: task_id.to_string(),
        state: state.clone(),
        run_id: run_id.clone(),
        completed,
    };
    let task_model = task::Entity::find_by_id(task_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Task {task_id} not found")))?;

    let server_ids: Vec<String> = serde_json::from_str(&task_model.server_ids_json)
        .map_err(|error| AppError::Internal(format!("Invalid task server_ids_json: {error}")))?;
    let timeout_secs = task_model.timeout.unwrap_or(300).max(1) as u64;
    let retry_count = if skip_retry {
        0
    } else {
        task_model.retry_count.max(0)
    };
    let retry_interval = task_model.retry_interval.max(1) as u64;
    let command = task_model.command.clone();
    let audit_context_ref = audit_context.as_ref();
    if let Some(context) = audit_context.clone() {
        state.exec_audit_contexts.insert(run_id.clone(), context);
    }

    let next_run_at = task_model
        .cron_expression
        .as_deref()
        .and_then(|expr| scheduler.next_run_at(expr));
    let mut update = task::Entity::update_many()
        .filter(task::Column::Id.eq(task_id))
        .col_expr(task::Column::LastRunAt, Expr::value(Utc::now()));
    if let Some(next_run_at) = next_run_at {
        update = update.col_expr(task::Column::NextRunAt, Expr::value(next_run_at));
    }
    update.exec(&state.db).await?;

    let target_servers = server::Entity::find()
        .filter(server::Column::Id.is_in(server_ids.clone()))
        .all(&state.db)
        .await?;
    let server_capabilities: HashMap<String, i32> = target_servers
        .iter()
        .map(|server| (server.id.clone(), server.capabilities))
        .collect();
    let mut join_set = JoinSet::new();

    for server_id in &server_ids {
        let configured_capabilities = server_capabilities.get(server_id).copied().unwrap_or(0);
        if let Some(reason) = state.agent_manager.capability_denied_reason(
            server_id,
            configured_capabilities as u32,
            CAP_EXEC,
        ) {
            write_synthetic_result(
                &state.db,
                task_id,
                &run_id,
                server_id,
                -2,
                exec_capability_denied_output(reason),
            )
            .await?;
            if let Some(context) = audit_context_ref {
                let detail = serde_json::json!({
                    "server_id": server_id,
                    "task_id": task_id,
                    "command": command,
                    "deny_reason": reason,
                })
                .to_string();
                let _ = AuditService::log(
                    &state.db,
                    &context.user_id,
                    "exec_denied",
                    Some(&detail),
                    &context.ip,
                )
                .await;
            }
            continue;
        }

        if let Some(context) = audit_context_ref {
            let detail = serde_json::json!({
                "server_id": server_id,
                "task_id": task_id,
                "command": command,
                "timeout": Some(timeout_secs as u32),
            })
            .to_string();
            let _ = AuditService::log(
                &state.db,
                &context.user_id,
                "exec_started",
                Some(&detail),
                &context.ip,
            )
            .await;
        }

        let state = state.clone();
        let task_id = task_id.to_string();
        let run_id = run_id.clone();
        let server_id = server_id.clone();
        let command = command.clone();
        let token = token.clone();
        join_set.spawn(async move {
            execute_for_server(
                &state,
                &task_id,
                &run_id,
                &server_id,
                &command,
                timeout_secs,
                retry_count,
                retry_interval,
                token,
            )
            .await
        });
    }

    tokio::spawn(async move {
        let _active_run_guard = active_run_guard;
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::error!("Failed to persist scheduled task result: {error}");
                }
                Err(error) => {
                    tracing::error!("Scheduled task executor failed to join: {error}");
                }
            }
        }
    });

    Ok(true)
}

#[allow(clippy::too_many_arguments)]
async fn execute_for_server(
    state: &Arc<AppState>,
    task_id: &str,
    run_id: &str,
    server_id: &str,
    command: &str,
    timeout_secs: u64,
    retry_count: i32,
    retry_interval: u64,
    token: CancellationToken,
) -> Result<(), DbErr> {
    let max_attempts = retry_count + 1;
    let timeout_duration = std::time::Duration::from_secs(timeout_secs + 10);

    for attempt in 1..=max_attempts {
        if token.is_cancelled() {
            break;
        }
        if attempt > 1 {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(retry_interval)) => {}
                _ = token.cancelled() => { break; }
            }
        }

        let correlation_id = build_correlation_id(task_id, run_id, server_id, attempt);
        let started_at = Utc::now();
        let result = tokio::select! {
            result = state.agent_manager.request_with_id(
                server_id,
                correlation_id,
                timeout_duration,
                |msg_id| ServerMessage::Exec {
                    task_id: msg_id,
                    command: command.to_string(),
                    timeout: Some(timeout_secs as u32),
                },
            ) => result,
            _ = token.cancelled() => {
                break;
            }
        };

        if token.is_cancelled() {
            break;
        }

        match result {
            Ok(AgentMessage::TaskResult { result, .. }) => {
                write_result(
                    &state.db,
                    task_id,
                    run_id,
                    server_id,
                    attempt,
                    started_at,
                    result.exit_code,
                    &result.output,
                )
                .await?;
                if result.exit_code == 0 {
                    break;
                }
            }
            Err(AgentRequestError::Offline) => {
                write_result(
                    &state.db,
                    task_id,
                    run_id,
                    server_id,
                    attempt,
                    started_at,
                    -3,
                    "Server offline",
                )
                .await?;
            }
            Err(AgentRequestError::SendFailed) => {
                write_result(
                    &state.db,
                    task_id,
                    run_id,
                    server_id,
                    attempt,
                    started_at,
                    -3,
                    "Dispatch failed",
                )
                .await?;
            }
            _ => {
                write_result(
                    &state.db,
                    task_id,
                    run_id,
                    server_id,
                    attempt,
                    started_at,
                    -4,
                    &format!("No response within {timeout_secs}s"),
                )
                .await?;
            }
        }

        if attempt == max_attempts {
            break;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn write_result(
    db: &DatabaseConnection,
    task_id: &str,
    run_id: &str,
    server_id: &str,
    attempt: i32,
    started_at: DateTime<Utc>,
    exit_code: i32,
    output: &str,
) -> Result<(), DbErr> {
    task_result::ActiveModel {
        id: NotSet,
        task_id: Set(task_id.to_string()),
        server_id: Set(server_id.to_string()),
        output: Set(output.to_string()),
        exit_code: Set(exit_code),
        finished_at: Set(Utc::now()),
        run_id: Set(Some(run_id.to_string())),
        attempt: Set(attempt),
        started_at: Set(Some(started_at)),
    }
    .insert(db)
    .await?;
    Ok(())
}

async fn write_synthetic_result(
    db: &DatabaseConnection,
    task_id: &str,
    run_id: &str,
    server_id: &str,
    exit_code: i32,
    output: &str,
) -> Result<(), DbErr> {
    write_result(
        db,
        task_id,
        run_id,
        server_id,
        1,
        Utc::now(),
        exit_code,
        output,
    )
    .await
}

async fn dispatch_oneshot(
    state: &Arc<AppState>,
    task_id: &str,
    command: &str,
    server_ids: &[String],
    timeout: Option<u32>,
    user_id: &str,
    ip: &str,
) -> Result<(), AppError> {
    let servers = server::Entity::find()
        .filter(server::Column::Id.is_in(server_ids.iter().cloned()))
        .all(&state.db)
        .await?;
    let mut capable = Vec::new();
    let mut disabled = Vec::new();

    for server_id in server_ids {
        let configured_capabilities = servers
            .iter()
            .find(|server| server.id == *server_id)
            .map(|server| server.capabilities as u32)
            .unwrap_or(0);
        if let Some(reason) = state.agent_manager.capability_denied_reason(
            server_id,
            configured_capabilities,
            CAP_EXEC,
        ) {
            disabled.push((
                server_id.as_str(),
                exec_capability_denied_output(reason),
                reason,
            ));
        } else {
            capable.push(server_id);
        }
    }

    let now = Utc::now();
    for (server_id, output, deny_reason) in &disabled {
        task_result::ActiveModel {
            id: NotSet,
            task_id: Set(task_id.to_string()),
            server_id: Set((*server_id).to_string()),
            output: Set((*output).to_string()),
            exit_code: Set(-2),
            run_id: Set(None),
            attempt: Set(1),
            started_at: Set(None),
            finished_at: Set(now),
        }
        .insert(&state.db)
        .await?;
        let detail = serde_json::json!({
            "server_id": server_id,
            "task_id": task_id,
            "command": command,
            "deny_reason": deny_reason,
        })
        .to_string();
        let _ = AuditService::log(&state.db, user_id, "exec_denied", Some(&detail), ip).await;
    }

    let mut dispatched = 0;
    for server_id in &capable {
        if let Some(sender) = state.agent_manager.get_sender(server_id) {
            let message = ServerMessage::Exec {
                task_id: task_id.to_string(),
                command: command.to_string(),
                timeout,
            };
            if sender.send(message).await.is_ok() {
                dispatched += 1;
                let detail = serde_json::json!({
                    "server_id": server_id,
                    "task_id": task_id,
                    "command": command,
                    "timeout": timeout,
                })
                .to_string();
                let _ =
                    AuditService::log(&state.db, user_id, "exec_started", Some(&detail), ip).await;
            }
        }
    }

    tracing::info!(
        "Task {} dispatched to {}/{} agents",
        task_id,
        dispatched,
        server_ids.len()
    );
    Ok(())
}

fn exec_capability_denied_output(reason: &str) -> &'static str {
    match reason {
        "agent_capability_disabled" => {
            "Capability denied: exec is disabled in the agent's config (capabilities are agent-owned)"
        }
        _ => "Capability denied: exec disabled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Weekday};
    use serverbee_common::constants::CAP_DEFAULT;

    fn test_active_run(run_id: &str) -> ActiveRun {
        ActiveRun {
            run_id: run_id.to_string(),
            cancellation: CancellationToken::new(),
            completed: CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn test_new_scheduler() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        assert!(!scheduler.is_running("nonexistent"));
    }

    #[tokio::test]
    async fn test_overlap_detection() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        scheduler
            .active_runs
            .insert("task-1".to_string(), test_active_run("run-1"));
        assert!(scheduler.is_running("task-1"));
        assert!(!scheduler.is_running("task-2"));
    }

    #[tokio::test]
    async fn test_cancel_active_run() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let active_run = test_active_run("run-1");
        let token = active_run.cancellation.clone();
        scheduler
            .active_runs
            .insert("task-1".to_string(), active_run);
        scheduler.cancel_active_run("task-1");
        assert!(token.is_cancelled());
        assert!(
            scheduler.is_running("task-1"),
            "the slot must remain occupied until the cancelled run drains"
        );
    }

    #[tokio::test]
    async fn test_stale_cleanup_cannot_remove_a_newer_run() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        scheduler
            .active_runs
            .insert("task-1".to_string(), test_active_run("run-a"));
        scheduler.cancel_active_run("task-1");

        assert!(matches!(
            scheduler.active_runs.entry("task-1".to_string()),
            Entry::Occupied(_)
        ));
        assert!(clear_active_run_if_current(
            &scheduler.active_runs,
            "task-1",
            "run-a"
        ));

        scheduler
            .active_runs
            .insert("task-1".to_string(), test_active_run("run-b"));
        assert!(!clear_active_run_if_current(
            &scheduler.active_runs,
            "task-1",
            "run-a"
        ));
        let active = scheduler.active_runs.get("task-1").unwrap();
        assert_eq!(active.run_id, "run-b");
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

    fn create_scheduled_request() -> CreateTaskRequest {
        CreateTaskRequest {
            command: "echo hi".to_string(),
            server_ids: vec!["server-1".to_string()],
            timeout: Some(5),
            task_type: TaskType::Scheduled,
            name: Some("test task".to_string()),
            cron_expression: Some(FAR_FUTURE_CRON.to_string()),
            retry_count: Some(0),
            retry_interval: Some(1),
        }
    }

    // ---- accessors / basic state --------------------------------------

    #[tokio::test]
    async fn test_timezone_accessor() {
        let scheduler = TaskScheduler::new("Asia/Shanghai").await.unwrap();
        assert_eq!(scheduler.tz(), chrono_tz::Asia::Shanghai);
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
    async fn test_new_scheduler_rejects_invalid_timezone() {
        let err = match TaskScheduler::new("Not/AZone").await {
            Ok(_) => panic!("invalid timezone should fail"),
            Err(error) => error,
        };
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
        let current_job_id = *scheduler.job_map.get("t-ok").unwrap();
        assert!(scheduler.is_current_job("t-ok", current_job_id));
        assert!(!scheduler.is_current_job("t-ok", uuid::Uuid::nil()));
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
    async fn test_remove_job_cancels_active_run_and_clears_job_map() {
        let (state, _db, _dir) = build_test_state().await;
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let task = make_task("t-remove", Some(FAR_FUTURE_CRON), true);
        scheduler.add_job(&task, state).await.unwrap();
        assert!(scheduler.job_map.contains_key("t-remove"));

        // Simulate the executor guard draining after it observes cancellation.
        let active_run = test_active_run("run-x");
        let cancellation = active_run.cancellation.clone();
        let completed = active_run.completed.clone();
        let active_runs = Arc::clone(&scheduler.active_runs);
        scheduler
            .active_runs
            .insert("t-remove".to_string(), active_run);
        let cancelled = cancellation.clone();
        tokio::spawn(async move {
            cancellation.cancelled().await;
            clear_active_run_if_current(&active_runs, "t-remove", "run-x");
            completed.cancel();
        });

        scheduler.remove_job("t-remove").await.unwrap();

        assert!(cancelled.is_cancelled(), "active run should be cancelled");
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

    #[test]
    fn test_validate_cron_uses_scheduler_syntax() {
        assert!(validate_cron(FAR_FUTURE_CRON).is_ok());
        let error = validate_cron("0 0 0 1 1 * 2027").unwrap_err();
        assert!(matches!(error, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn test_next_run_uses_scheduler_weekday_semantics() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let next = scheduler
            .next_run_at("0 0 0 * * 1")
            .expect("weekday schedule should have a next run");
        assert_eq!(next.weekday(), Weekday::Mon);
    }

    #[tokio::test]
    async fn test_create_scheduled_task_owns_row_and_job() {
        let (state, _db, _dir) = build_test_state().await;
        let created = create_task(&state, create_scheduled_request(), "admin", "127.0.0.1")
            .await
            .unwrap();

        assert!(created.next_run_at.is_some());
        assert!(state.task_scheduler.job_map.contains_key(&created.id));
        assert!(
            task::Entity::find_by_id(&created.id)
                .one(&state.db)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_update_scheduled_task_disables_and_restores_job() {
        let (state, _db, _dir) = build_test_state().await;
        let created = create_task(&state, create_scheduled_request(), "admin", "127.0.0.1")
            .await
            .unwrap();
        let update = |enabled| UpdateTaskRequest {
            name: None,
            command: None,
            server_ids: None,
            cron_expression: None,
            enabled: Some(enabled),
            timeout: None,
            retry_count: None,
            retry_interval: None,
        };

        let disabled = update_task(&state, &created.id, update(false))
            .await
            .unwrap();
        assert!(!disabled.enabled);
        assert!(!state.task_scheduler.job_map.contains_key(&created.id));

        let enabled = update_task(&state, &created.id, update(true))
            .await
            .unwrap();
        assert!(enabled.enabled);
        assert!(enabled.next_run_at.is_some());
        assert!(state.task_scheduler.job_map.contains_key(&created.id));
    }

    #[tokio::test]
    async fn test_delete_task_removes_row_results_job_and_active_run() {
        let (state, _db, _dir) = build_test_state().await;
        let created = create_task(&state, create_scheduled_request(), "admin", "127.0.0.1")
            .await
            .unwrap();
        write_synthetic_result(
            &state.db,
            &created.id,
            "run-delete",
            "server-1",
            -2,
            "denied",
        )
        .await
        .unwrap();
        let active_run = test_active_run("run-delete");
        let cancellation = active_run.cancellation.clone();
        let completed = active_run.completed.clone();
        let active_runs = Arc::clone(&state.task_scheduler.active_runs);
        let db = state.db.clone();
        let task_id = created.id.clone();
        state
            .task_scheduler
            .active_runs
            .insert(created.id.clone(), active_run);
        let cancelled = cancellation.clone();
        tokio::spawn(async move {
            cancellation.cancelled().await;
            write_synthetic_result(&db, &task_id, "run-delete", "server-1", -3, "late result")
                .await
                .unwrap();
            clear_active_run_if_current(&active_runs, &task_id, "run-delete");
            completed.cancel();
        });

        delete_task(&state, &created.id).await.unwrap();

        assert!(cancelled.is_cancelled());
        assert!(!state.task_scheduler.job_map.contains_key(&created.id));
        assert!(!state.task_scheduler.active_runs.contains_key(&created.id));
        assert_eq!(result_count(&state.db, &created.id).await, 0);
        assert!(
            task::Entity::find_by_id(&created.id)
                .one(&state.db)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_server_cleanup_updates_or_removes_tasks_and_scheduler_jobs() {
        let (state, _db, _dir) = build_test_state().await;
        let mut deleted_request = create_scheduled_request();
        deleted_request.server_ids = vec!["orphan".to_string()];
        let deleted = create_task(&state, deleted_request, "admin", "127.0.0.1")
            .await
            .unwrap();
        let mut retained_request = create_scheduled_request();
        retained_request.server_ids = vec!["orphan".to_string(), "keep".to_string()];
        let retained = create_task(&state, retained_request, "admin", "127.0.0.1")
            .await
            .unwrap();

        let active_run = test_active_run("cleanup-run");
        let cancellation = active_run.cancellation.clone();
        let completed = active_run.completed.clone();
        let active_runs = Arc::clone(&state.task_scheduler.active_runs);
        let db = state.db.clone();
        let retained_task_id = retained.id.clone();
        state
            .task_scheduler
            .active_runs
            .insert(retained.id.clone(), active_run);
        let cancelled = cancellation.clone();
        tokio::spawn(async move {
            cancellation.cancelled().await;
            write_synthetic_result(
                &db,
                &retained_task_id,
                "cleanup-run",
                "orphan",
                -3,
                "late orphan result",
            )
            .await
            .unwrap();
            clear_active_run_if_current(&active_runs, &retained_task_id, "cleanup-run");
            completed.cancel();
        });

        let mut cleanup = begin_server_cleanup(&state).await;
        let transaction = state.db.begin().await.unwrap();
        cleanup
            .remove_server_references(&transaction, &["orphan".to_string()])
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        cleanup.apply_after_commit(&state).await;

        assert!(cancelled.is_cancelled());
        assert!(
            task::Entity::find_by_id(&deleted.id)
                .one(&state.db)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!state.task_scheduler.job_map.contains_key(&deleted.id));

        let retained = task::Entity::find_by_id(&retained.id)
            .one(&state.db)
            .await
            .unwrap()
            .expect("task with a remaining server should survive");
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&retained.server_ids_json).unwrap(),
            vec!["keep"]
        );
        assert!(state.task_scheduler.job_map.contains_key(&retained.id));
        assert_eq!(
            task_result::Entity::find()
                .filter(task_result::Column::ServerId.eq("orphan"))
                .count(&state.db)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn test_restore_and_start_registers_only_enabled_scheduled_tasks() {
        let (state, _db, _dir) = build_test_state().await;
        seed_task(&state.db, "enabled-task", &["server-1"]).await;
        seed_task(&state.db, "disabled-task", &["server-1"]).await;
        task::Entity::update_many()
            .filter(task::Column::Id.eq("disabled-task"))
            .col_expr(task::Column::Enabled, Expr::value(false))
            .exec(&state.db)
            .await
            .unwrap();

        restore_and_start(state.clone()).await;

        assert!(state.task_scheduler.job_map.contains_key("enabled-task"));
        assert!(!state.task_scheduler.job_map.contains_key("disabled-task"));
        let enabled = task::Entity::find_by_id("enabled-task")
            .one(&state.db)
            .await
            .unwrap()
            .unwrap();
        assert!(enabled.next_run_at.is_some());
    }

    #[tokio::test]
    async fn test_run_now_reports_overlap_as_conflict() {
        let (state, _db, _dir) = build_test_state().await;
        let created = create_task(&state, create_scheduled_request(), "admin", "127.0.0.1")
            .await
            .unwrap();
        state
            .task_scheduler
            .active_runs
            .insert(created.id.clone(), test_active_run("existing-run"));

        let error = run_now(&state, &created.id, None).await.unwrap_err();
        assert!(matches!(error, AppError::Conflict(_)));
    }

    #[test]
    fn test_correlation_id_format() {
        let cid = build_correlation_id("task-1", "run-abc", "srv-1", 1);
        assert_eq!(cid, "task-1:run-abc:srv-1:1");
    }

    #[test]
    fn test_correlation_id_uniqueness() {
        let a = build_correlation_id("t1", "r1", "s1", 1);
        let b = build_correlation_id("t1", "r1", "s2", 1);
        let c = build_correlation_id("t1", "r1", "s1", 2);
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    // build_correlation_id must not collapse empty segments — the colon-joined
    // shape is preserved even when every component is the empty string.
    #[test]
    fn test_correlation_id_empty_segments_keep_delimiters() {
        assert_eq!(build_correlation_id("", "", "", 0), ":::0");
    }

    // Negative and large attempt numbers are formatted verbatim (no clamping).
    #[test]
    fn test_correlation_id_attempt_boundaries() {
        assert_eq!(build_correlation_id("t", "r", "s", -1), "t:r:s:-1");
        assert_eq!(
            build_correlation_id("t", "r", "s", i32::MAX),
            format!("t:r:s:{}", i32::MAX)
        );
    }

    // ---- Helpers -------------------------------------------------------

    /// Insert a server row with the given id and persisted capability mirror.
    /// The server is NOT registered with the agent_manager, so it is offline.
    async fn seed_server(db: &DatabaseConnection, id: &str, capabilities: i32) {
        use crate::entity::server;
        let now = chrono::Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some("hash".to_string())),
            token_prefix: Set(Some("sb_pref".to_string())),
            name: Set(format!("server-{id}")),
            cpu_name: Set(None),
            cpu_cores: Set(None),
            cpu_arch: Set(None),
            os: Set(None),
            kernel_version: Set(None),
            mem_total: Set(None),
            swap_total: Set(None),
            disk_total: Set(None),
            ipv4: Set(None),
            ipv6: Set(None),
            region: Set(None),
            country_code: Set(None),
            geo_manual: Set(false),
            virtualization: Set(None),
            agent_version: Set(None),
            group_id: Set(None),
            weight: Set(0),
            hidden: Set(false),
            remark: Set(None),
            public_remark: Set(None),
            price: Set(None),
            billing_cycle: Set(None),
            currency: Set(None),
            expired_at: Set(None),
            traffic_limit: Set(None),
            traffic_limit_type: Set(None),
            billing_start_day: Set(None),
            capabilities: Set(capabilities),
            protocol_version: Set(1),
            features: Set("[]".to_string()),
            last_remote_addr: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed server");
    }

    /// Insert a scheduled task row targeting `server_ids`.
    async fn seed_task(db: &DatabaseConnection, id: &str, server_ids: &[&str]) {
        let now = chrono::Utc::now();
        task::ActiveModel {
            id: Set(id.to_string()),
            command: Set("echo hi".to_string()),
            server_ids_json: Set(serde_json::to_string(server_ids).unwrap()),
            created_by: Set("tester".to_string()),
            task_type: Set("scheduled".to_string()),
            name: Set(Some("test task".to_string())),
            cron_expression: Set(Some("0 0 0 1 1 *".to_string())),
            enabled: Set(true),
            timeout: Set(Some(5)),
            retry_count: Set(0),
            retry_interval: Set(1),
            last_run_at: Set(None),
            next_run_at: Set(None),
            created_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed task");
    }

    /// Count persisted task_result rows for a given task id.
    async fn result_count(db: &DatabaseConnection, task_id: &str) -> u64 {
        task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq(task_id))
            .count(db)
            .await
            .expect("count task results")
    }

    /// Poll until at least `expected` task_result rows exist for `task_id` (the
    /// offline executor path runs inside a detached tokio task spawned by
    /// `execute_scheduled_task`).
    async fn wait_for_results(db: &DatabaseConnection, task_id: &str, expected: u64) -> u64 {
        for _ in 0..100 {
            let n = result_count(db, task_id).await;
            if n >= expected {
                return n;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        result_count(db, task_id).await
    }

    // ---- write_result (pure DB write) ---------------------------------

    // write_result persists a row with every field set verbatim, including a
    // negative synthetic exit code and the supplied run_id/attempt/timestamps.
    #[tokio::test]
    async fn test_write_result_persists_all_fields() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let started = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        write_result(
            &db,
            "task-w",
            "run-w",
            "srv-w",
            2,
            started,
            -3,
            "Server offline",
        )
        .await
        .expect("write_result should insert");

        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-w"))
            .one(&db)
            .await
            .unwrap()
            .expect("row present");
        assert_eq!(row.server_id, "srv-w");
        assert_eq!(row.output, "Server offline");
        assert_eq!(row.exit_code, -3);
        assert_eq!(row.run_id.as_deref(), Some("run-w"));
        assert_eq!(row.attempt, 2);
        assert_eq!(row.started_at, Some(started));
    }

    // ---- write_synthetic_result --------------------------------------

    // write_synthetic_result delegates to write_result with a fixed attempt of
    // 1 and stamps started_at itself (so it is Some, not the caller's value).
    #[tokio::test]
    async fn test_write_synthetic_result_uses_attempt_one() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        write_synthetic_result(&db, "task-s", "run-s", "srv-s", -2, "capability denied")
            .await
            .expect("synthetic write should insert");

        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-s"))
            .one(&db)
            .await
            .unwrap()
            .expect("row present");
        assert_eq!(row.attempt, 1);
        assert_eq!(row.exit_code, -2);
        assert_eq!(row.output, "capability denied");
        assert!(row.started_at.is_some());
    }

    // ---- execute_for_server (offline agent) ---------------------------

    // With no connected agent, execute_for_server falls into the get_sender
    // None branch and writes exactly one "Server offline" result (exit -3) when
    // retries are exhausted (retry_count == 0 → single attempt).
    #[tokio::test]
    async fn test_execute_for_server_offline_writes_single_result() {
        let (state, _db, _dir) = build_test_state().await;
        let token = CancellationToken::new();
        execute_for_server(
            &state, "task-off", "run-off", "srv-off", "echo hi", 1, 0, 1, token,
        )
        .await
        .unwrap();

        assert_eq!(result_count(&state.db, "task-off").await, 1);
        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-off"))
            .one(&state.db)
            .await
            .unwrap()
            .expect("offline result present");
        assert_eq!(row.exit_code, -3);
        assert_eq!(row.output, "Server offline");
        assert_eq!(row.attempt, 1);
    }

    // An already-cancelled token short-circuits before the attempt loop body,
    // so no result row is written at all.
    #[tokio::test]
    async fn test_execute_for_server_cancelled_writes_nothing() {
        let (state, _db, _dir) = build_test_state().await;
        let token = CancellationToken::new();
        token.cancel();
        execute_for_server(
            &state,
            "task-cancel",
            "run-c",
            "srv-c",
            "echo hi",
            1,
            2,
            1,
            token,
        )
        .await
        .unwrap();

        assert_eq!(result_count(&state.db, "task-cancel").await, 0);
    }

    // ---- execute_scheduled_task: missing task -------------------------

    // A task id that is not in the DB returns a real not-found error and clears
    // its claimed active_runs entry (no result rows written).
    #[tokio::test]
    async fn test_execute_scheduled_task_missing_task_returns_not_found() {
        let (state, _db, _dir) = build_test_state().await;
        let error = execute_scheduled_task(&state, "ghost-task", true, None)
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::NotFound(_)));
        assert!(!state.task_scheduler.is_running("ghost-task"));
        assert_eq!(result_count(&state.db, "ghost-task").await, 0);
    }

    // ---- execute_scheduled_task: overlap guard ------------------------

    // When an active run already occupies the active_runs slot, a second
    // trigger is skipped (returns false) without touching the existing entry.
    #[tokio::test]
    async fn test_execute_scheduled_task_overlap_is_skipped() {
        let (state, _db, _dir) = build_test_state().await;
        seed_task(&state.db, "task-overlap", &[]).await;
        state
            .task_scheduler
            .active_runs
            .insert("task-overlap".to_string(), test_active_run("prior-run"));

        let started = execute_scheduled_task(&state, "task-overlap", true, None)
            .await
            .unwrap();
        assert!(!started);
        // The pre-existing run_id must remain untouched.
        let entry = state
            .task_scheduler
            .active_runs
            .get("task-overlap")
            .expect("entry retained");
        assert_eq!(entry.run_id, "prior-run");
    }

    // ---- execute_scheduled_task: capability denied --------------------

    // A target server whose mirror lacks CAP_EXEC produces a synthetic
    // capability-denied result (exit -2) written synchronously before any
    // server execution is spawned.
    #[tokio::test]
    async fn test_execute_scheduled_task_cap_exec_denied_writes_synthetic() {
        let (state, _db, _dir) = build_test_state().await;
        // CAP_DEFAULT intentionally excludes CAP_EXEC.
        seed_server(&state.db, "srv-nocap", CAP_DEFAULT as i32).await;
        seed_task(&state.db, "task-nocap", &["srv-nocap"]).await;

        let started = execute_scheduled_task(&state, "task-nocap", true, None)
            .await
            .unwrap();
        assert!(started);

        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-nocap"))
            .one(&state.db)
            .await
            .unwrap()
            .expect("synthetic denied result present");
        assert_eq!(row.exit_code, -2);
        assert_eq!(row.server_id, "srv-nocap");
        assert!(row.output.contains("Capability denied"));
    }

    // With an audit context supplied, the capability-denied branch also records
    // an `exec_denied` audit log entry alongside the synthetic result.
    #[tokio::test]
    async fn test_execute_scheduled_task_cap_denied_logs_audit() {
        use crate::entity::audit_log;
        let (state, _db, _dir) = build_test_state().await;
        seed_server(&state.db, "srv-audit", CAP_DEFAULT as i32).await;
        seed_task(&state.db, "task-audit", &["srv-audit"]).await;

        let ctx = ExecAuditContext {
            user_id: "admin".to_string(),
            ip: "127.0.0.1".to_string(),
        };
        let started = execute_scheduled_task(&state, "task-audit", true, Some(ctx))
            .await
            .unwrap();
        assert!(started);

        let denied = audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq("exec_denied"))
            .count(&state.db)
            .await
            .unwrap();
        assert_eq!(denied, 1);
    }

    // ---- execute_scheduled_task: offline agent dispatch ---------------

    // A server that HAS CAP_EXEC but is not connected reaches execute_for_server
    // via the spawned join_set, which writes a "Server offline" result (exit
    // -3). retry_count is forced to 0 here because skip_retry == true.
    #[tokio::test]
    async fn test_execute_scheduled_task_offline_agent_writes_offline_result() {
        let (state, _db, _dir) = build_test_state().await;
        seed_server(&state.db, "srv-online-cap", (CAP_DEFAULT | CAP_EXEC) as i32).await;
        seed_task(&state.db, "task-dispatch", &["srv-online-cap"]).await;

        let started = execute_scheduled_task(&state, "task-dispatch", true, None)
            .await
            .unwrap();
        assert!(started);

        let n = wait_for_results(&state.db, "task-dispatch", 1).await;
        assert_eq!(n, 1);
        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-dispatch"))
            .one(&state.db)
            .await
            .unwrap()
            .expect("offline dispatch result present");
        assert_eq!(row.exit_code, -3);
        assert_eq!(row.output, "Server offline");
    }

    // An empty server list produces no work and no result rows, but the trigger
    // is still considered "started" (returns true) and updates last_run_at.
    #[tokio::test]
    async fn test_execute_scheduled_task_empty_server_list_starts_with_no_results() {
        let (state, _db, _dir) = build_test_state().await;
        seed_task(&state.db, "task-empty", &[]).await;

        let started = execute_scheduled_task(&state, "task-empty", true, None)
            .await
            .unwrap();
        assert!(started);

        // Give any (non-existent) spawned work a chance to run, then assert none.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(result_count(&state.db, "task-empty").await, 0);

        let updated = task::Entity::find_by_id("task-empty")
            .one(&state.db)
            .await
            .unwrap()
            .expect("task present");
        assert!(
            updated.last_run_at.is_some(),
            "last_run_at should be stamped"
        );
    }
}
