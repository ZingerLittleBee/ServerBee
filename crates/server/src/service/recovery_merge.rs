use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::entity::recovery_job;
use crate::error::AppError;
use crate::service::recovery_job::RecoveryJobService;
use crate::state::AppState;

pub const RECOVERY_STAGE_VALIDATING: &str = "validating";
pub const RECOVERY_STAGE_REBINDING: &str = "rebinding";
pub const RECOVERY_STAGE_AWAITING_TARGET_ONLINE: &str = "awaiting_target_online";

pub struct RecoveryMergeService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryFailurePhase {
    PreRebind,
    PostRebind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryRetryStrategy {
    StartNewJob,
    ResumeSameJob,
}

impl RecoveryMergeService {
    pub async fn start(
        state: &Arc<AppState>,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        Self::start_on_db(&state.db, target_server_id, source_server_id).await
    }

    pub async fn handle_rebind_ack(
        state: &Arc<AppState>,
        job_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        Self::handle_rebind_ack_on_db(&state.db, job_id).await
    }

    pub async fn start_on_db(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        let running_target = RecoveryJobService::running_for_target(db, target_server_id).await?;
        let running_source = RecoveryJobService::running_for_source(db, source_server_id).await?;

        if let Some(job) = running_target {
            if let Some(source_job) = &running_source
                && source_job.job_id != job.job_id
            {
                return Err(AppError::Conflict(
                    "A running recovery job already exists for this target or source".to_string(),
                ));
            }

            if !is_pre_rebind_stage(job.stage.as_str()) {
                return Err(AppError::Conflict(
                    "Recovery job has already advanced past the rebind step".to_string(),
                ));
            }

            return RecoveryJobService::update_stage(
                db,
                &job.job_id,
                RECOVERY_STAGE_REBINDING,
                None,
                None,
            )
            .await;
        }

        if running_source.is_some() {
            return Err(AppError::Conflict(
                "A running recovery job already exists for this target or source".to_string(),
            ));
        }

        let job = RecoveryJobService::create_job(db, target_server_id, source_server_id).await?;
        RecoveryJobService::update_stage(db, &job.job_id, RECOVERY_STAGE_REBINDING, None, None)
            .await
    }

    pub async fn handle_rebind_ack_on_db(
        db: &DatabaseConnection,
        job_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        let job = RecoveryJobService::get_job(db, job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;

        if job.status != "running" {
            return Ok(job);
        }

        if job.stage == RECOVERY_STAGE_AWAITING_TARGET_ONLINE {
            return Ok(job);
        }

        RecoveryJobService::update_stage(
            db,
            job_id,
            RECOVERY_STAGE_AWAITING_TARGET_ONLINE,
            None,
            None,
        )
        .await
    }
}

pub fn recovery_phase_for_stage(stage: &str) -> RecoveryFailurePhase {
    match stage {
        RECOVERY_STAGE_VALIDATING | RECOVERY_STAGE_REBINDING => RecoveryFailurePhase::PreRebind,
        _ => RecoveryFailurePhase::PostRebind,
    }
}

pub fn is_pre_rebind_stage(stage: &str) -> bool {
    matches!(
        recovery_phase_for_stage(stage),
        RecoveryFailurePhase::PreRebind
    )
}

pub fn retry_strategy_for_phase(phase: RecoveryFailurePhase) -> RecoveryRetryStrategy {
    match phase {
        RecoveryFailurePhase::PreRebind => RecoveryRetryStrategy::StartNewJob,
        RecoveryFailurePhase::PostRebind => RecoveryRetryStrategy::ResumeSameJob,
    }
}

pub fn retry_strategy_for_stage(stage: &str) -> RecoveryRetryStrategy {
    retry_strategy_for_phase(recovery_phase_for_stage(stage))
}

#[cfg(test)]
mod tests {
    use super::{
        RECOVERY_STAGE_AWAITING_TARGET_ONLINE, RECOVERY_STAGE_REBINDING, RecoveryFailurePhase,
        RecoveryMergeService, RecoveryRetryStrategy, is_pre_rebind_stage, recovery_phase_for_stage,
        retry_strategy_for_phase, retry_strategy_for_stage,
    };
    use crate::service::recovery_job::RecoveryJobService;
    use crate::test_utils::setup_test_db;

    #[test]
    fn pre_rebind_phase_requires_new_job() {
        assert_eq!(
            retry_strategy_for_phase(RecoveryFailurePhase::PreRebind),
            RecoveryRetryStrategy::StartNewJob
        );
        assert_eq!(
            retry_strategy_for_stage(RECOVERY_STAGE_REBINDING),
            RecoveryRetryStrategy::StartNewJob
        );
        assert_eq!(
            recovery_phase_for_stage(RECOVERY_STAGE_REBINDING),
            RecoveryFailurePhase::PreRebind
        );
    }

    #[test]
    fn post_rebind_phase_resumes_same_job() {
        assert_eq!(
            retry_strategy_for_phase(RecoveryFailurePhase::PostRebind),
            RecoveryRetryStrategy::ResumeSameJob
        );
        assert_eq!(
            retry_strategy_for_stage(RECOVERY_STAGE_AWAITING_TARGET_ONLINE),
            RecoveryRetryStrategy::ResumeSameJob
        );
        assert_eq!(
            recovery_phase_for_stage(RECOVERY_STAGE_AWAITING_TARGET_ONLINE),
            RecoveryFailurePhase::PostRebind
        );
    }

    #[tokio::test]
    async fn start_persists_job_and_advances_to_rebinding() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        assert_eq!(job.target_server_id, "target-1");
        assert_eq!(job.source_server_id, "source-1");
        assert_eq!(job.status, "running");
        assert_eq!(job.stage, RECOVERY_STAGE_REBINDING);
        assert!(job.last_heartbeat_at.is_some());

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, RECOVERY_STAGE_REBINDING);
        assert_eq!(loaded.status, "running");
    }

    #[tokio::test]
    async fn start_reuses_existing_pre_rebind_job() {
        let (db, _tmp) = setup_test_db().await;

        let first = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();
        let second = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        assert_eq!(second.job_id, first.job_id);
        assert_eq!(second.stage, RECOVERY_STAGE_REBINDING);
        assert!(is_pre_rebind_stage(second.stage.as_str()));
    }

    #[tokio::test]
    async fn rebind_ack_advances_to_waiting_for_target_online() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        let updated = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id)
            .await
            .unwrap();

        assert_eq!(updated.job_id, job.job_id);
        assert_eq!(updated.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
        assert_eq!(updated.status, "running");

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
    }

    #[tokio::test]
    async fn rebind_ack_is_idempotent_once_advanced() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();
        let _ = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id)
            .await
            .unwrap();

        let updated = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id)
            .await
            .unwrap();

        assert_eq!(updated.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
        assert_eq!(updated.status, "running");
    }
}
