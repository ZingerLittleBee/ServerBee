use std::sync::Arc;

use chrono::Utc;
use sea_orm::prelude::Expr;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::{recovery_job, server};
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
        Self::validate_start_request(state, target_server_id, source_server_id).await?;
        Self::start_on_db(&state.db, target_server_id, source_server_id).await
    }

    pub async fn handle_rebind_ack(
        state: &Arc<AppState>,
        job_id: &str,
        acking_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        Self::handle_rebind_ack_on_db(&state.db, job_id, acking_server_id).await
    }

    async fn validate_start_request(
        state: &Arc<AppState>,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError> {
        if source_server_id == target_server_id {
            return Err(AppError::Validation(
                "source_server_id must be different from target_id".to_string(),
            ));
        }

        let target = server::Entity::find_by_id(target_server_id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;
        let source = server::Entity::find_by_id(source_server_id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;

        if state.agent_manager.is_online(&target.id) {
            return Err(AppError::Conflict(
                "Target server must be offline before starting recovery".to_string(),
            ));
        }

        if !state.agent_manager.is_online(&source.id) {
            return Err(AppError::Conflict(
                "Source server must be online before starting recovery".to_string(),
            ));
        }

        Ok(())
    }

    async fn start_on_db(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        if let Some(existing) =
            Self::find_reusable_start_job(db, target_server_id, source_server_id).await?
        {
            return Self::advance_job_to_rebinding(db, existing).await;
        }

        match RecoveryJobService::create_job(db, target_server_id, source_server_id).await {
            Ok(job) => Self::advance_job_to_rebinding(db, job).await,
            Err(AppError::Conflict(_)) => {
                Self::recover_duplicate_start(db, target_server_id, source_server_id).await
            }
            Err(err) => Err(err),
        }
    }

    async fn find_reusable_start_job(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<Option<recovery_job::Model>, AppError> {
        let running_target = RecoveryJobService::running_for_target(db, target_server_id).await?;
        let running_source = RecoveryJobService::running_for_source(db, source_server_id).await?;

        if let Some(job) = running_target {
            if job.source_server_id != source_server_id {
                return Err(AppError::Conflict(
                    "A running recovery job already exists for this target or source".to_string(),
                ));
            }

            if let Some(source_job) = &running_source
                && source_job.job_id != job.job_id
            {
                return Err(AppError::Conflict(
                    "A running recovery job already exists for this target or source".to_string(),
                ));
            }

            return Ok(Some(job));
        }

        if running_source.is_some() {
            return Err(AppError::Conflict(
                "A running recovery job already exists for this target or source".to_string(),
            ));
        }

        Ok(None)
    }

    async fn recover_duplicate_start(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        match Self::find_reusable_start_job(db, target_server_id, source_server_id).await? {
            Some(job) => Self::advance_job_to_rebinding(db, job).await,
            None => Err(AppError::Conflict(
                "A running recovery job already exists for this target or source".to_string(),
            )),
        }
    }

    async fn advance_job_to_rebinding(
        db: &DatabaseConnection,
        job: recovery_job::Model,
    ) -> Result<recovery_job::Model, AppError> {
        let now = Utc::now();
        let result = recovery_job::Entity::update_many()
            .col_expr(
                recovery_job::Column::Stage,
                Expr::value(RECOVERY_STAGE_REBINDING),
            )
            .col_expr(recovery_job::Column::CheckpointJson, Expr::value(None::<String>))
            .col_expr(recovery_job::Column::Error, Expr::value(None::<String>))
            .col_expr(recovery_job::Column::UpdatedAt, Expr::value(now))
            .col_expr(recovery_job::Column::LastHeartbeatAt, Expr::value(Some(now)))
            .filter(recovery_job::Column::JobId.eq(&job.job_id))
            .filter(recovery_job::Column::Stage.is_in([
                RECOVERY_STAGE_VALIDATING,
                RECOVERY_STAGE_REBINDING,
            ]))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return RecoveryJobService::get_job(db, &job.job_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()));
        }

        RecoveryJobService::get_job(db, &job.job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))
    }

    async fn handle_rebind_ack_on_db(
        db: &DatabaseConnection,
        job_id: &str,
        acking_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        let job = RecoveryJobService::get_job(db, job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;

        if job.source_server_id != acking_server_id {
            return Ok(job);
        }

        if job.status != "running" {
            return Ok(job);
        }

        if job.stage != RECOVERY_STAGE_REBINDING {
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

pub fn recovery_phase_for_stage(stage: &str) -> Option<RecoveryFailurePhase> {
    match stage {
        RECOVERY_STAGE_VALIDATING | RECOVERY_STAGE_REBINDING => {
            Some(RecoveryFailurePhase::PreRebind)
        }
        RECOVERY_STAGE_AWAITING_TARGET_ONLINE => Some(RecoveryFailurePhase::PostRebind),
        _ => None,
    }
}

pub fn is_pre_rebind_stage(stage: &str) -> bool {
    matches!(
        recovery_phase_for_stage(stage),
        Some(RecoveryFailurePhase::PreRebind)
    )
}

pub fn retry_strategy_for_phase(phase: RecoveryFailurePhase) -> RecoveryRetryStrategy {
    match phase {
        RecoveryFailurePhase::PreRebind => RecoveryRetryStrategy::StartNewJob,
        RecoveryFailurePhase::PostRebind => RecoveryRetryStrategy::ResumeSameJob,
    }
}

pub fn retry_strategy_for_stage(stage: &str) -> Option<RecoveryRetryStrategy> {
    recovery_phase_for_stage(stage).map(retry_strategy_for_phase)
}

#[cfg(test)]
mod tests {
    use super::{
        RECOVERY_STAGE_AWAITING_TARGET_ONLINE, RECOVERY_STAGE_REBINDING, RecoveryFailurePhase,
        RecoveryMergeService, RecoveryRetryStrategy, is_pre_rebind_stage, recovery_phase_for_stage,
        retry_strategy_for_phase, retry_strategy_for_stage,
    };
    use crate::config::AppConfig;
    use crate::entity::server;
    use crate::error::AppError;
    use crate::service::auth::AuthService;
    use crate::service::recovery_job::RecoveryJobService;
    use crate::state::AppState;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
    use serverbee_common::constants::CAP_DEFAULT;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    async fn insert_test_server(db: &DatabaseConnection, id: &str, name: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash_password should succeed");
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(token_hash),
            token_prefix: Set("serverbee_test".to_string()),
            name: Set(name.to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server should succeed");
    }

    async fn test_state_with_servers() -> (Arc<AppState>, TempDir) {
        let (db, tmp) = setup_test_db().await;
        insert_test_server(&db, "target-1", "Target").await;
        insert_test_server(&db, "source-1", "Source").await;
        insert_test_server(&db, "source-2", "Source 2").await;
        let state = AppState::new(db, AppConfig::default())
            .await
            .expect("app state should initialize");
        (state, tmp)
    }

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9527)
    }

    fn mark_online(state: &Arc<AppState>, server_id: &str) {
        let (tx, _) = mpsc::channel(1);
        state.agent_manager.add_connection(
            server_id.to_string(),
            server_id.to_string(),
            tx,
            test_addr(),
        );
    }

    #[test]
    fn pre_rebind_phase_requires_new_job() {
        assert_eq!(
            retry_strategy_for_phase(RecoveryFailurePhase::PreRebind),
            RecoveryRetryStrategy::StartNewJob
        );
        assert_eq!(
            retry_strategy_for_stage(RECOVERY_STAGE_REBINDING),
            Some(RecoveryRetryStrategy::StartNewJob)
        );
        assert_eq!(
            recovery_phase_for_stage(RECOVERY_STAGE_REBINDING),
            Some(RecoveryFailurePhase::PreRebind)
        );
        assert_eq!(retry_strategy_for_stage("unknown"), None);
        assert_eq!(recovery_phase_for_stage("unknown"), None);
    }

    #[test]
    fn post_rebind_phase_resumes_same_job() {
        assert_eq!(
            retry_strategy_for_phase(RecoveryFailurePhase::PostRebind),
            RecoveryRetryStrategy::ResumeSameJob
        );
        assert_eq!(
            retry_strategy_for_stage(RECOVERY_STAGE_AWAITING_TARGET_ONLINE),
            Some(RecoveryRetryStrategy::ResumeSameJob)
        );
        assert_eq!(
            recovery_phase_for_stage(RECOVERY_STAGE_AWAITING_TARGET_ONLINE),
            Some(RecoveryFailurePhase::PostRebind)
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
    async fn start_rejects_existing_target_job_for_different_source() {
        let (db, _tmp) = setup_test_db().await;

        let first = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        let result = RecoveryMergeService::start_on_db(&db, "target-1", "source-2").await;
        assert!(matches!(result, Err(AppError::Conflict(_))));

        let loaded = RecoveryJobService::get_job(&db, &first.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.source_server_id, "source-1");
        assert_eq!(loaded.stage, RECOVERY_STAGE_REBINDING);
    }

    #[tokio::test]
    async fn rebind_ack_advances_to_waiting_for_target_online() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        let updated = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id, "source-1")
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
        let _ = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id, "source-1")
            .await
            .unwrap();

        let updated = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id, "source-1")
            .await
            .unwrap();

        assert_eq!(updated.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
        assert_eq!(updated.status, "running");
    }

    #[tokio::test]
    async fn rebind_ack_ignores_wrong_stage() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();
        RecoveryJobService::update_stage(&db, &job.job_id, "validating", None, None)
            .await
            .unwrap();

        let updated = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id, "source-1")
            .await
            .unwrap();

        assert_eq!(updated.stage, "validating");

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, "validating");
    }

    #[tokio::test]
    async fn rebind_ack_from_wrong_source_is_ignored() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        let updated = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id, "source-2")
            .await
            .unwrap();

        assert_eq!(updated.job_id, job.job_id);
        assert_eq!(updated.stage, RECOVERY_STAGE_REBINDING);

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, RECOVERY_STAGE_REBINDING);
    }

    #[tokio::test]
    async fn start_rejects_self_merge_at_service_boundary() {
        let (state, _tmp) = test_state_with_servers().await;
        mark_online(&state, "source-1");

        let result = RecoveryMergeService::start(&state, "target-1", "target-1").await;

        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[tokio::test]
    async fn start_rejects_online_target_at_service_boundary() {
        let (state, _tmp) = test_state_with_servers().await;
        mark_online(&state, "target-1");
        mark_online(&state, "source-1");

        let result = RecoveryMergeService::start(&state, "target-1", "source-1").await;

        assert!(
            matches!(result, Err(AppError::Conflict(message)) if message.contains("Target server must be offline"))
        );
    }

    #[tokio::test]
    async fn start_rejects_offline_source_at_service_boundary() {
        let (state, _tmp) = test_state_with_servers().await;

        let result = RecoveryMergeService::start(&state, "target-1", "source-1").await;

        assert!(
            matches!(result, Err(AppError::Conflict(message)) if message.contains("Source server must be online"))
        );
    }

    #[tokio::test]
    async fn duplicate_start_conflict_reuses_matching_pre_rebind_job() {
        let (db, _tmp) = setup_test_db().await;

        let first = RecoveryJobService::create_job(&db, "target-1", "source-1")
            .await
            .unwrap();

        let reused = RecoveryMergeService::recover_duplicate_start(&db, "target-1", "source-1")
            .await
            .unwrap();

        assert_eq!(reused.job_id, first.job_id);
        assert_eq!(reused.stage, RECOVERY_STAGE_REBINDING);
    }

    #[tokio::test]
    async fn reusable_start_keeps_latest_stage_when_rebind_ack_wins_race() {
        let (db, _tmp) = setup_test_db().await;

        let stale_job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();
        let acknowledged = RecoveryMergeService::handle_rebind_ack_on_db(
            &db,
            &stale_job.job_id,
            "source-1",
        )
        .await
        .unwrap();
        assert_eq!(acknowledged.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);

        let advanced = RecoveryMergeService::advance_job_to_rebinding(&db, stale_job)
            .await
            .unwrap();

        assert_eq!(advanced.job_id, acknowledged.job_id);
        assert_eq!(advanced.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);

        let loaded = RecoveryJobService::get_job(&db, &advanced.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
    }
}
