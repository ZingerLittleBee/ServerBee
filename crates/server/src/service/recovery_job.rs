use chrono::Utc;
use sea_orm::*;
use uuid::Uuid;

use crate::entity::recovery_job;
use crate::error::AppError;

pub struct RecoveryJobService;

fn is_unique_violation(err: &DbErr) -> bool {
    let message = err.to_string();
    message.contains("UNIQUE constraint failed") || message.contains("UNIQUE")
}

impl RecoveryJobService {
    pub async fn create_job(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        let now = Utc::now();
        let active = recovery_job::ActiveModel {
            job_id: Set(Uuid::new_v4().to_string()),
            target_server_id: Set(target_server_id.to_string()),
            source_server_id: Set(source_server_id.to_string()),
            status: Set("running".to_string()),
            stage: Set("validating".to_string()),
            checkpoint_json: Set(None),
            error: Set(None),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(None),
        };

        match active.insert(db).await {
            Ok(model) => Ok(model),
            Err(err) if is_unique_violation(&err) => Err(AppError::Conflict(
                "A running recovery job already exists for this target or source".to_string(),
            )),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn get_job(
        db: &DatabaseConnection,
        job_id: &str,
    ) -> Result<Option<recovery_job::Model>, AppError> {
        Ok(recovery_job::Entity::find_by_id(job_id).one(db).await?)
    }

    pub async fn update_stage(
        db: &DatabaseConnection,
        job_id: &str,
        stage: &str,
        checkpoint_json: Option<&str>,
        error: Option<&str>,
    ) -> Result<recovery_job::Model, AppError> {
        let model = Self::get_job(db, job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;
        let mut active: recovery_job::ActiveModel = model.into();
        let now = Utc::now();

        active.stage = Set(stage.to_string());
        active.checkpoint_json = Set(checkpoint_json.map(ToOwned::to_owned));
        active.error = Set(error.map(ToOwned::to_owned));
        active.updated_at = Set(now);
        active.last_heartbeat_at = Set(Some(now));

        Ok(active.update(db).await?)
    }

    pub async fn mark_failed(
        db: &DatabaseConnection,
        job_id: &str,
        stage: &str,
        error: &str,
    ) -> Result<(), AppError> {
        let model = Self::get_job(db, job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;
        let mut active: recovery_job::ActiveModel = model.into();
        let now = Utc::now();

        active.status = Set("failed".to_string());
        active.stage = Set(stage.to_string());
        active.error = Set(Some(error.to_string()));
        active.updated_at = Set(now);
        active.last_heartbeat_at = Set(Some(now));

        active.update(db).await?;
        Ok(())
    }

    pub async fn running_for_target(
        db: &DatabaseConnection,
        target_server_id: &str,
    ) -> Result<Option<recovery_job::Model>, AppError> {
        Ok(recovery_job::Entity::find()
            .filter(recovery_job::Column::TargetServerId.eq(target_server_id))
            .filter(recovery_job::Column::Status.eq("running"))
            .one(db)
            .await?)
    }

    pub async fn running_for_source(
        db: &DatabaseConnection,
        source_server_id: &str,
    ) -> Result<Option<recovery_job::Model>, AppError> {
        Ok(recovery_job::Entity::find()
            .filter(recovery_job::Column::SourceServerId.eq(source_server_id))
            .filter(recovery_job::Column::Status.eq("running"))
            .one(db)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::RecoveryJobService;
    use crate::entity::recovery_job;
    use crate::test_utils::setup_test_db;
    use crate::error::AppError;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};

    async fn insert_job(
        db: &sea_orm::DatabaseConnection,
        job_id: &str,
        target_server_id: &str,
        source_server_id: &str,
        status: &str,
    ) -> recovery_job::Model {
        let now = Utc::now();
        recovery_job::ActiveModel {
            job_id: Set(job_id.to_string()),
            target_server_id: Set(target_server_id.to_string()),
            source_server_id: Set(source_server_id.to_string()),
            status: Set(status.to_string()),
            stage: Set("validating".to_string()),
            checkpoint_json: Set(None),
            error: Set(None),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(None),
        }
        .insert(db)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn create_job_persists_running_row_for_target_and_source() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryJobService::create_job(&db, "target-1", "source-1")
            .await
            .unwrap();

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.job_id, job.job_id);
        assert_eq!(loaded.target_server_id, "target-1");
        assert_eq!(loaded.source_server_id, "source-1");
        assert_eq!(loaded.status, "running");
        assert_eq!(loaded.stage, "validating");
        assert_eq!(loaded.checkpoint_json, None);
        assert_eq!(loaded.error, None);
        assert!(loaded.last_heartbeat_at.is_none());
    }

    #[tokio::test]
    async fn update_stage_round_trips_stage_and_checkpoint_json() {
        let (db, _tmp) = setup_test_db().await;
        let job = RecoveryJobService::create_job(&db, "target-1", "source-1")
            .await
            .unwrap();

        RecoveryJobService::update_stage(
            &db,
            &job.job_id,
            "merging_history",
            Some("{\"group\":2}"),
            None,
        )
        .await
        .unwrap();

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, "merging_history");
        assert_eq!(loaded.checkpoint_json.as_deref(), Some("{\"group\":2}"));
        assert_eq!(loaded.error, None);
        assert_eq!(loaded.status, "running");
        assert!(loaded.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn mark_failed_updates_status_stage_and_error() {
        let (db, _tmp) = setup_test_db().await;
        let job = RecoveryJobService::create_job(&db, "target-1", "source-1")
            .await
            .unwrap();

        RecoveryJobService::mark_failed(&db, &job.job_id, "finalizing", "boom")
            .await
            .unwrap();

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.status, "failed");
        assert_eq!(loaded.stage, "finalizing");
        assert_eq!(loaded.error.as_deref(), Some("boom"));
        assert!(loaded.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn running_queries_match_by_target_and_source() {
        let (db, _tmp) = setup_test_db().await;
        let job = RecoveryJobService::create_job(&db, "target-1", "source-1")
            .await
            .unwrap();
        let _failed = insert_job(&db, "job-failed", "target-1", "source-1", "failed").await;

        let by_target = RecoveryJobService::running_for_target(&db, "target-1")
            .await
            .unwrap()
            .unwrap();
        let by_source = RecoveryJobService::running_for_source(&db, "source-1")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(by_target.job_id, job.job_id);
        assert_eq!(by_source.job_id, job.job_id);
    }

    #[tokio::test]
    async fn running_queries_ignore_non_running_jobs() {
        let (db, _tmp) = setup_test_db().await;
        let job = RecoveryJobService::create_job(&db, "target-1", "source-1")
            .await
            .unwrap();
        RecoveryJobService::mark_failed(&db, &job.job_id, "finalizing", "boom")
            .await
            .unwrap();

        assert!(RecoveryJobService::running_for_target(&db, "target-1")
            .await
            .unwrap()
            .is_none());
        assert!(RecoveryJobService::running_for_source(&db, "source-1")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn create_job_rejects_duplicate_active_jobs_for_target_or_source() {
        let (db, _tmp) = setup_test_db().await;

        let _first = RecoveryJobService::create_job(&db, "target-1", "source-1")
            .await
            .unwrap();

        match RecoveryJobService::create_job(&db, "target-1", "source-2").await {
            Err(AppError::Conflict(message)) => {
                assert!(message.contains("running recovery job"));
            }
            other => panic!("expected conflict for duplicate target, got {other:?}"),
        }

        match RecoveryJobService::create_job(&db, "target-2", "source-1").await {
            Err(AppError::Conflict(message)) => {
                assert!(message.contains("running recovery job"));
            }
            other => panic!("expected conflict for duplicate source, got {other:?}"),
        }
    }
}
