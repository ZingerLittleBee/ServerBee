use std::sync::Arc;

use chrono::Utc;
use sea_orm::DatabaseBackend;
use sea_orm::prelude::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, QueryFilter, Statement,
};

use crate::entity::{network_probe_config, recovery_job, server, server_tag};
use crate::error::AppError;
use crate::service::auth::AuthService;
use crate::service::db_error::is_active_recovery_conflict;
use crate::service::recovery_job::RecoveryJobService;
use crate::service::traffic::TrafficService;
use crate::state::AppState;

pub const RECOVERY_STAGE_VALIDATING: &str = "validating";
pub const RECOVERY_STAGE_REBINDING: &str = "rebinding";
pub const RECOVERY_STAGE_AWAITING_TARGET_ONLINE: &str = "awaiting_target_online";
pub const REBIND_IDENTITY_MIN_PROTOCOL_VERSION: u32 = 4;

pub struct RecoveryMergeService;

pub struct RecoveryStateChange {
    pub job: recovery_job::Model,
    pub transitioned: bool,
}

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
    ) -> Result<RecoveryStateChange, AppError> {
        Self::handle_rebind_ack_on_db(&state.db, job_id, acking_server_id).await
    }

    pub async fn handle_rebind_failure(
        state: &Arc<AppState>,
        job_id: &str,
        source_server_id: &str,
        error: &str,
    ) -> Result<RecoveryStateChange, AppError> {
        Self::handle_rebind_failure_on_db(&state.db, job_id, source_server_id, error).await
    }

    pub async fn rotate_target_token(
        state: &Arc<AppState>,
        target_server_id: &str,
    ) -> Result<String, AppError> {
        Self::rotate_target_token_on_conn(&state.db, target_server_id).await
    }

    pub async fn rotate_target_token_on_txn(
        txn: &DatabaseTransaction,
        target_server_id: &str,
    ) -> Result<String, AppError> {
        Self::rotate_target_token_on_conn(txn, target_server_id).await
    }

    async fn rotate_target_token_on_conn<C>(
        db: &C,
        target_server_id: &str,
    ) -> Result<String, AppError>
    where
        C: ConnectionTrait,
    {
        let target = server::Entity::find_by_id(target_server_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;

        let plaintext_token = AuthService::generate_session_token();
        let token_hash = AuthService::hash_password(&plaintext_token)?;
        let token_prefix = plaintext_token[..8.min(plaintext_token.len())].to_string();

        let mut active: server::ActiveModel = target.into();
        active.token_hash = sea_orm::Set(token_hash);
        active.token_prefix = sea_orm::Set(token_prefix);
        active.updated_at = sea_orm::Set(Utc::now());
        active.update(db).await?;

        Ok(plaintext_token)
    }

    pub async fn validate_start_request(
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

        Self::validate_connectivity_preconditions(
            state,
            &target.id,
            &source.id,
            "Target server must be offline before starting recovery",
            "Source server must be online before starting recovery",
        )?;

        Self::validate_rebind_identity_protocol(state, &source)?;
        Ok(())
    }

    pub async fn validate_dispatch_preconditions(
        state: &Arc<AppState>,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError> {
        Self::validate_connectivity_preconditions(
            state,
            target_server_id,
            source_server_id,
            "Recovery start aborted because target server came back online before dispatch",
            "Recovery start aborted because source server went offline before dispatch",
        )
    }

    async fn start_on_db(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        Self::start_on_connection(db, target_server_id, source_server_id).await
    }

    pub async fn start_on_txn(
        db: &DatabaseTransaction,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        Self::start_on_connection(db, target_server_id, source_server_id).await
    }

    async fn start_on_connection<C>(
        db: &C,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError>
    where
        C: ConnectionTrait,
    {
        if let Some(existing) =
            Self::find_reusable_start_job(db, target_server_id, source_server_id).await?
        {
            return Self::advance_job_to_rebinding(db, existing).await;
        }

        match Self::create_job(db, target_server_id, source_server_id).await {
            Ok(job) => Self::advance_job_to_rebinding(db, job).await,
            Err(AppError::Conflict(_)) => {
                Self::recover_duplicate_start(db, target_server_id, source_server_id).await
            }
            Err(err) => Err(err),
        }
    }

    async fn find_reusable_start_job<C>(
        db: &C,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<Option<recovery_job::Model>, AppError>
    where
        C: ConnectionTrait,
    {
        let running_target = Self::running_for_target(db, target_server_id).await?;
        let running_source = Self::running_for_source(db, source_server_id).await?;

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

    async fn recover_duplicate_start<C>(
        db: &C,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError>
    where
        C: ConnectionTrait,
    {
        match Self::find_reusable_start_job(db, target_server_id, source_server_id).await? {
            Some(job) => Self::advance_job_to_rebinding(db, job).await,
            None => Err(AppError::Conflict(
                "A running recovery job already exists for this target or source".to_string(),
            )),
        }
    }

    async fn advance_job_to_rebinding<C>(
        db: &C,
        job: recovery_job::Model,
    ) -> Result<recovery_job::Model, AppError>
    where
        C: ConnectionTrait,
    {
        let now = Utc::now();
        let result = recovery_job::Entity::update_many()
            .col_expr(
                recovery_job::Column::Stage,
                Expr::value(RECOVERY_STAGE_REBINDING),
            )
            .col_expr(
                recovery_job::Column::CheckpointJson,
                Expr::value(None::<String>),
            )
            .col_expr(recovery_job::Column::Error, Expr::value(None::<String>))
            .col_expr(recovery_job::Column::UpdatedAt, Expr::value(now))
            .col_expr(
                recovery_job::Column::LastHeartbeatAt,
                Expr::value(Some(now)),
            )
            .filter(recovery_job::Column::JobId.eq(&job.job_id))
            .filter(recovery_job::Column::Status.eq("running"))
            .filter(
                recovery_job::Column::Stage
                    .is_in([RECOVERY_STAGE_VALIDATING, RECOVERY_STAGE_REBINDING]),
            )
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            return Self::get_job(db, &job.job_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()));
        }

        Self::get_job(db, &job.job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))
    }

    async fn create_job<C>(
        db: &C,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError>
    where
        C: ConnectionTrait,
    {
        let active = recovery_job::ActiveModel {
            job_id: sea_orm::Set(uuid::Uuid::new_v4().to_string()),
            target_server_id: sea_orm::Set(target_server_id.to_string()),
            source_server_id: sea_orm::Set(source_server_id.to_string()),
            status: sea_orm::Set("running".to_string()),
            stage: sea_orm::Set(RECOVERY_STAGE_VALIDATING.to_string()),
            checkpoint_json: sea_orm::Set(None),
            error: sea_orm::Set(None),
            started_at: sea_orm::Set(Utc::now()),
            created_at: sea_orm::Set(Utc::now()),
            updated_at: sea_orm::Set(Utc::now()),
            last_heartbeat_at: sea_orm::Set(None),
        };

        active.insert(db).await.map_err(|err| {
            if is_active_recovery_conflict(&err) {
                AppError::Conflict(
                    "A running recovery job already exists for this target or source".to_string(),
                )
            } else {
                err.into()
            }
        })
    }

    async fn running_for_target<C>(
        db: &C,
        target_server_id: &str,
    ) -> Result<Option<recovery_job::Model>, AppError>
    where
        C: ConnectionTrait,
    {
        Ok(recovery_job::Entity::find()
            .filter(recovery_job::Column::TargetServerId.eq(target_server_id))
            .filter(recovery_job::Column::Status.eq("running"))
            .one(db)
            .await?)
    }

    async fn running_for_source<C>(
        db: &C,
        source_server_id: &str,
    ) -> Result<Option<recovery_job::Model>, AppError>
    where
        C: ConnectionTrait,
    {
        Ok(recovery_job::Entity::find()
            .filter(recovery_job::Column::SourceServerId.eq(source_server_id))
            .filter(recovery_job::Column::Status.eq("running"))
            .one(db)
            .await?)
    }

    async fn get_job<C>(db: &C, job_id: &str) -> Result<Option<recovery_job::Model>, AppError>
    where
        C: ConnectionTrait,
    {
        Ok(recovery_job::Entity::find_by_id(job_id).one(db).await?)
    }

    fn validate_rebind_identity_protocol(
        state: &Arc<AppState>,
        source: &server::Model,
    ) -> Result<(), AppError> {
        let protocol_version = state
            .agent_manager
            .get_protocol_version(&source.id)
            .unwrap_or(source.protocol_version as u32);

        if protocol_version < REBIND_IDENTITY_MIN_PROTOCOL_VERSION {
            return Err(AppError::Conflict(format!(
                "Source server must support RebindIdentity (protocol v{}+ required)",
                REBIND_IDENTITY_MIN_PROTOCOL_VERSION
            )));
        }

        Ok(())
    }

    fn validate_connectivity_preconditions(
        state: &Arc<AppState>,
        target_server_id: &str,
        source_server_id: &str,
        target_online_message: &str,
        source_offline_message: &str,
    ) -> Result<(), AppError> {
        if state.agent_manager.is_online(target_server_id) {
            return Err(AppError::Conflict(target_online_message.to_string()));
        }

        if !state.agent_manager.is_online(source_server_id) {
            return Err(AppError::Conflict(source_offline_message.to_string()));
        }

        Ok(())
    }

    async fn handle_rebind_ack_on_db(
        db: &DatabaseConnection,
        job_id: &str,
        acking_server_id: &str,
    ) -> Result<RecoveryStateChange, AppError> {
        let now = Utc::now();
        let result = recovery_job::Entity::update_many()
            .col_expr(
                recovery_job::Column::Stage,
                Expr::value(RECOVERY_STAGE_AWAITING_TARGET_ONLINE),
            )
            .col_expr(
                recovery_job::Column::CheckpointJson,
                Expr::value(None::<String>),
            )
            .col_expr(recovery_job::Column::Error, Expr::value(None::<String>))
            .col_expr(recovery_job::Column::UpdatedAt, Expr::value(now))
            .col_expr(
                recovery_job::Column::LastHeartbeatAt,
                Expr::value(Some(now)),
            )
            .filter(recovery_job::Column::JobId.eq(job_id))
            .filter(recovery_job::Column::SourceServerId.eq(acking_server_id))
            .filter(recovery_job::Column::Status.eq("running"))
            .filter(recovery_job::Column::Stage.eq(RECOVERY_STAGE_REBINDING))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            let job = RecoveryJobService::get_job(db, job_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;
            return Ok(RecoveryStateChange {
                job,
                transitioned: false,
            });
        }

        let job = RecoveryJobService::get_job(db, job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;
        Ok(RecoveryStateChange {
            job,
            transitioned: true,
        })
    }

    async fn handle_rebind_failure_on_db(
        db: &DatabaseConnection,
        job_id: &str,
        source_server_id: &str,
        error: &str,
    ) -> Result<RecoveryStateChange, AppError> {
        let now = Utc::now();
        let result = recovery_job::Entity::update_many()
            .col_expr(recovery_job::Column::Status, Expr::value("failed"))
            .col_expr(
                recovery_job::Column::Stage,
                Expr::value(RECOVERY_STAGE_REBINDING),
            )
            .col_expr(
                recovery_job::Column::Error,
                Expr::value(Some(error.to_string())),
            )
            .col_expr(recovery_job::Column::UpdatedAt, Expr::value(now))
            .col_expr(
                recovery_job::Column::LastHeartbeatAt,
                Expr::value(Some(now)),
            )
            .filter(recovery_job::Column::JobId.eq(job_id))
            .filter(recovery_job::Column::SourceServerId.eq(source_server_id))
            .filter(recovery_job::Column::Status.eq("running"))
            .filter(recovery_job::Column::Stage.eq(RECOVERY_STAGE_REBINDING))
            .exec(db)
            .await?;

        if result.rows_affected == 0 {
            let job = RecoveryJobService::get_job(db, job_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;
            return Ok(RecoveryStateChange {
                job,
                transitioned: false,
            });
        }

        let job = RecoveryJobService::get_job(db, job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;
        Ok(RecoveryStateChange {
            job,
            transitioned: true,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn merge_server_history_on_db(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError> {
        Self::merge_server_history_on_connection(db, target_server_id, source_server_id).await
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn merge_server_history_on_txn(
        txn: &DatabaseTransaction,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError> {
        Self::merge_server_history_on_connection(txn, target_server_id, source_server_id).await
    }

    async fn merge_server_history_on_connection<C>(
        db: &C,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        Self::merge_raw_table_on_connection(
            db,
            "records",
            "time",
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::merge_raw_table_on_connection(
            db,
            "gpu_records",
            "time",
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::merge_raw_table_on_connection(
            db,
            "ping_records",
            "time",
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::merge_raw_table_on_connection(
            db,
            "task_results",
            "finished_at",
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::merge_raw_table_on_connection(
            db,
            "network_probe_record",
            "timestamp",
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::merge_raw_table_on_connection(
            db,
            "docker_event",
            "timestamp",
            target_server_id,
            source_server_id,
        )
        .await?;

        Self::merge_unique_key_table_on_connection(
            db,
            "records_hourly",
            &["time"],
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::merge_unique_key_table_on_connection(
            db,
            "network_probe_record_hourly",
            &["target_id", "hour"],
            target_server_id,
            source_server_id,
        )
        .await?;
        TrafficService::merge_recovered_server_history_on_connection(
            db,
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::merge_unique_key_table_on_connection(
            db,
            "uptime_daily",
            &["date"],
            target_server_id,
            source_server_id,
        )
        .await?;
        Self::merge_alert_states_on_connection(db, target_server_id, source_server_id).await?;
        Self::rewrite_server_ids_json_tables_on_connection(db, target_server_id, source_server_id)
            .await?;

        Ok(())
    }

    async fn merge_raw_table_on_connection<C>(
        db: &C,
        table: &str,
        time_column: &str,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            format!(
                "DELETE FROM {table} \
                 WHERE server_id = $1 \
                 AND (SELECT MIN({time_column}) FROM {table} WHERE server_id = $2) IS NOT NULL \
                 AND {time_column} >= (SELECT MIN({time_column}) FROM {table} WHERE server_id = $2) \
                 AND {time_column} <= (SELECT MAX({time_column}) FROM {table} WHERE server_id = $2)"
            ),
            [target_server_id.into(), source_server_id.into()],
        ))
        .await?;

        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            format!("UPDATE {table} SET server_id = $1 WHERE server_id = $2"),
            [target_server_id.into(), source_server_id.into()],
        ))
        .await?;

        Ok(())
    }

    async fn merge_unique_key_table_on_connection<C>(
        db: &C,
        table: &str,
        key_columns: &[&str],
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        TrafficService::replace_unique_key_table_server_id_on_connection(
            db,
            table,
            key_columns,
            target_server_id,
            source_server_id,
        )
        .await
    }

    async fn merge_alert_states_on_connection<C>(
        db: &C,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            "DELETE FROM alert_states AS source \
             WHERE source.server_id = $1 \
             AND EXISTS ( \
                 SELECT 1 FROM alert_states AS target \
                 WHERE target.server_id = $2 AND target.rule_id = source.rule_id \
             )",
            [source_server_id.into(), target_server_id.into()],
        ))
        .await?;

        db.execute(Statement::from_sql_and_values(
            db.get_database_backend(),
            "UPDATE alert_states SET server_id = $1 WHERE server_id = $2",
            [target_server_id.into(), source_server_id.into()],
        ))
        .await?;

        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn rewrite_server_ids_json_tables(
        db: &DatabaseConnection,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError> {
        Self::rewrite_server_ids_json_tables_on_connection(db, target_server_id, source_server_id)
            .await
    }

    async fn rewrite_server_ids_json_tables_on_connection<C>(
        db: &C,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        let tables = [
            ("alert_rules", "server_ids_json"),
            ("ping_tasks", "server_ids_json"),
            ("tasks", "server_ids_json"),
            ("service_monitor", "server_ids_json"),
            ("maintenance", "server_ids_json"),
            ("incident", "server_ids_json"),
            ("status_page", "server_ids_json"),
        ];

        for (table, column) in tables {
            Self::rewrite_server_ids_json_table_on_connection(
                db,
                table,
                column,
                target_server_id,
                source_server_id,
            )
            .await?;
        }

        Ok(())
    }

    async fn rewrite_server_ids_json_table_on_connection<C>(
        db: &C,
        table: &str,
        column: &str,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        let rows = db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                format!("SELECT id, {column} FROM {table} WHERE {column} LIKE '%' || $1 || '%'"),
                [source_server_id.into()],
            ))
            .await?;

        for row in rows {
            let id: String = row.try_get_by_index(0)?;
            let current: Option<String> = row.try_get_by_index(1)?;
            let Some(current) = current else {
                continue;
            };

            let rewritten =
                Self::rewrite_server_ids_json_value(&current, target_server_id, source_server_id)?;
            if rewritten.as_deref() == Some(current.as_str()) {
                continue;
            }

            let value = rewritten.unwrap_or_else(|| "[]".to_string()).into();

            db.execute(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                format!("UPDATE {table} SET {column} = $1 WHERE id = $2"),
                [value, id.into()],
            ))
            .await?;
        }

        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn rewrite_server_ids_json_value(
        current: &str,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<Option<String>, AppError> {
        let ids: Vec<String> = serde_json::from_str(current).map_err(|error| {
            AppError::Internal(format!(
                "Failed to parse server_ids_json during recovery merge: {error}"
            ))
        })?;

        let mut rewritten = Vec::new();
        for id in ids {
            let next = if id == source_server_id {
                target_server_id.to_string()
            } else {
                id
            };
            if !rewritten.iter().any(|existing| existing == &next) {
                rewritten.push(next);
            }
        }

        serde_json::to_string(&rewritten)
            .map(Some)
            .map_err(|error| {
                AppError::Internal(format!(
                    "Failed to serialize server_ids_json during recovery merge: {error}"
                ))
            })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn finalize_target_server_row(
        db: &DatabaseConnection,
        target_server_id: &str,
        source: &server::Model,
    ) -> Result<(), AppError> {
        Self::finalize_target_server_row_on_connection(db, target_server_id, source).await
    }

    async fn finalize_target_server_row_on_connection<C>(
        db: &C,
        target_server_id: &str,
        source: &server::Model,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        if source.fingerprint.is_some() {
            server::Entity::update_many()
                .col_expr(server::Column::Fingerprint, Expr::value(None::<String>))
                .col_expr(server::Column::UpdatedAt, Expr::value(Utc::now()))
                .filter(server::Column::Id.eq(source.id.clone()))
                .exec(db)
                .await?;
        }

        let target = server::Entity::find_by_id(target_server_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;

        let mut active: server::ActiveModel = target.into();
        active.cpu_name = sea_orm::Set(source.cpu_name.clone());
        active.cpu_cores = sea_orm::Set(source.cpu_cores);
        active.cpu_arch = sea_orm::Set(source.cpu_arch.clone());
        active.os = sea_orm::Set(source.os.clone());
        active.kernel_version = sea_orm::Set(source.kernel_version.clone());
        active.mem_total = sea_orm::Set(source.mem_total);
        active.swap_total = sea_orm::Set(source.swap_total);
        active.disk_total = sea_orm::Set(source.disk_total);
        active.ipv4 = sea_orm::Set(source.ipv4.clone());
        active.ipv6 = sea_orm::Set(source.ipv6.clone());
        active.region = sea_orm::Set(source.region.clone());
        active.country_code = sea_orm::Set(source.country_code.clone());
        active.virtualization = sea_orm::Set(source.virtualization.clone());
        active.agent_version = sea_orm::Set(source.agent_version.clone());
        active.protocol_version = sea_orm::Set(source.protocol_version);
        active.features = sea_orm::Set(source.features.clone());
        active.last_remote_addr = sea_orm::Set(source.last_remote_addr.clone());
        active.fingerprint = sea_orm::Set(source.fingerprint.clone());
        active.updated_at = sea_orm::Set(Utc::now());
        active.update(db).await?;

        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn delete_intentionally_unmerged_source_rows(
        db: &DatabaseConnection,
        source_server_id: &str,
    ) -> Result<(), AppError> {
        Self::delete_intentionally_unmerged_source_rows_on_connection(db, source_server_id).await
    }

    async fn delete_intentionally_unmerged_source_rows_on_connection<C>(
        db: &C,
        source_server_id: &str,
    ) -> Result<(), AppError>
    where
        C: ConnectionTrait,
    {
        server_tag::Entity::delete_many()
            .filter(server_tag::Column::ServerId.eq(source_server_id))
            .exec(db)
            .await?;
        network_probe_config::Entity::delete_many()
            .filter(network_probe_config::Column::ServerId.eq(source_server_id))
            .exec(db)
            .await?;

        Ok(())
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
        REBIND_IDENTITY_MIN_PROTOCOL_VERSION, RECOVERY_STAGE_AWAITING_TARGET_ONLINE,
        RECOVERY_STAGE_REBINDING, RecoveryFailurePhase, RecoveryMergeService,
        RecoveryRetryStrategy, is_pre_rebind_stage, recovery_phase_for_stage,
        retry_strategy_for_phase, retry_strategy_for_stage,
    };
    use crate::config::AppConfig;
    use crate::entity::{
        alert_rule, alert_state, record, server, server_tag, service_monitor, traffic_daily,
        traffic_hourly, traffic_state,
    };
    use crate::error::AppError;
    use crate::service::auth::AuthService;
    use crate::service::recovery_job::RecoveryJobService;
    use crate::state::AppState;
    use crate::test_utils::setup_test_db;
    use chrono::{NaiveDate, Utc};
    use sea_orm::{
        ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
        TransactionTrait,
    };
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
            protocol_version: Set(REBIND_IDENTITY_MIN_PROTOCOL_VERSION as i32),
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
        state
            .agent_manager
            .set_protocol_version(server_id, REBIND_IDENTITY_MIN_PROTOCOL_VERSION);
    }

    async fn insert_record(
        db: &DatabaseConnection,
        server_id: &str,
        time: chrono::DateTime<Utc>,
        cpu: f64,
    ) {
        record::ActiveModel {
            server_id: Set(server_id.to_string()),
            time: Set(time),
            cpu: Set(cpu),
            mem_used: Set(1),
            swap_used: Set(1),
            disk_used: Set(1),
            net_in_speed: Set(1),
            net_out_speed: Set(1),
            net_in_transfer: Set(1),
            net_out_transfer: Set(1),
            load1: Set(1.0),
            load5: Set(1.0),
            load15: Set(1.0),
            tcp_conn: Set(1),
            udp_conn: Set(1),
            process_count: Set(1),
            temperature: Set(None),
            gpu_usage: Set(None),
            disk_io_json: Set(None),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert record should succeed");
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

        assert!(updated.transitioned);
        assert_eq!(updated.job.job_id, job.job_id);
        assert_eq!(updated.job.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
        assert_eq!(updated.job.status, "running");

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

        assert!(!updated.transitioned);
        assert_eq!(updated.job.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
        assert_eq!(updated.job.status, "running");
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

        assert!(!updated.transitioned);
        assert_eq!(updated.job.stage, "validating");

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

        assert!(!updated.transitioned);
        assert_eq!(updated.job.job_id, job.job_id);
        assert_eq!(updated.job.stage, RECOVERY_STAGE_REBINDING);

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, RECOVERY_STAGE_REBINDING);
    }

    #[tokio::test]
    async fn rebind_failure_marks_job_failed() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        let failed = RecoveryMergeService::handle_rebind_failure_on_db(
            &db,
            &job.job_id,
            "source-1",
            "agent failed",
        )
        .await
        .unwrap();

        assert!(failed.transitioned);
        assert_eq!(failed.job.job_id, job.job_id);
        assert_eq!(failed.job.status, "failed");
        assert_eq!(failed.job.stage, RECOVERY_STAGE_REBINDING);
        assert_eq!(failed.job.error.as_deref(), Some("agent failed"));
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
        let acknowledged =
            RecoveryMergeService::handle_rebind_ack_on_db(&db, &stale_job.job_id, "source-1")
                .await
                .unwrap();
        assert!(acknowledged.transitioned);
        assert_eq!(
            acknowledged.job.stage,
            RECOVERY_STAGE_AWAITING_TARGET_ONLINE
        );

        let advanced = RecoveryMergeService::advance_job_to_rebinding(&db, stale_job)
            .await
            .unwrap();

        assert_eq!(advanced.job_id, acknowledged.job.job_id);
        assert_eq!(advanced.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);

        let loaded = RecoveryJobService::get_job(&db, &advanced.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
    }

    #[tokio::test]
    async fn advance_job_to_rebinding_does_not_overwrite_failed_job() {
        let (db, _tmp) = setup_test_db().await;

        let stale_job = RecoveryJobService::create_job(&db, "target-1", "source-1")
            .await
            .unwrap();
        RecoveryJobService::mark_failed(&db, &stale_job.job_id, "validating", "boom")
            .await
            .unwrap();

        let advanced = RecoveryMergeService::advance_job_to_rebinding(&db, stale_job)
            .await
            .unwrap();

        assert_eq!(advanced.status, "failed");
        assert_eq!(advanced.stage, "validating");

        let loaded = RecoveryJobService::get_job(&db, &advanced.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.status, "failed");
        assert_eq!(loaded.stage, "validating");
    }

    #[tokio::test]
    async fn rebind_ack_does_not_overwrite_moved_job() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryMergeService::start_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();
        RecoveryJobService::update_stage(
            &db,
            &job.job_id,
            RECOVERY_STAGE_AWAITING_TARGET_ONLINE,
            None,
            None,
        )
        .await
        .unwrap();

        let updated = RecoveryMergeService::handle_rebind_ack_on_db(&db, &job.job_id, "source-1")
            .await
            .unwrap();

        assert!(!updated.transitioned);
        assert_eq!(updated.job.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
        assert_eq!(updated.job.status, "running");

        let loaded = RecoveryJobService::get_job(&db, &job.job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.stage, RECOVERY_STAGE_AWAITING_TARGET_ONLINE);
        assert_eq!(loaded.status, "running");
    }

    #[tokio::test]
    async fn dispatch_validation_rejects_stale_source_offline_state() {
        let (state, _tmp) = test_state_with_servers().await;
        mark_online(&state, "source-1");

        RecoveryMergeService::validate_start_request(&state, "target-1", "source-1")
            .await
            .expect("initial start validation should succeed");

        state.agent_manager.remove_connection("source-1");

        let result =
            RecoveryMergeService::validate_dispatch_preconditions(&state, "target-1", "source-1")
                .await;

        assert!(
            matches!(result, Err(AppError::Conflict(message)) if message.contains("went offline before dispatch"))
        );
    }

    #[tokio::test]
    async fn merge_raw_records_replaces_target_overlap_with_source() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "target-1", "Target").await;
        insert_test_server(&db, "source-1", "Source").await;

        let before_overlap = NaiveDate::from_ymd_opt(2026, 4, 16)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
            .and_utc();
        let overlap_start = NaiveDate::from_ymd_opt(2026, 4, 16)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_utc();
        let overlap_end = NaiveDate::from_ymd_opt(2026, 4, 16)
            .unwrap()
            .and_hms_opt(11, 0, 0)
            .unwrap()
            .and_utc();

        insert_record(&db, "target-1", before_overlap, 10.0).await;
        insert_record(&db, "target-1", overlap_start, 20.0).await;
        insert_record(&db, "target-1", overlap_end, 30.0).await;
        insert_record(&db, "source-1", overlap_start, 200.0).await;
        insert_record(&db, "source-1", overlap_end, 300.0).await;

        RecoveryMergeService::merge_server_history_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        let target_rows = record::Entity::find()
            .filter(record::Column::ServerId.eq("target-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(target_rows.len(), 3);
        assert!(
            target_rows
                .iter()
                .any(|row| row.time == before_overlap && row.cpu == 10.0)
        );
        assert!(
            target_rows
                .iter()
                .any(|row| row.time == overlap_start && row.cpu == 200.0)
        );
        assert!(
            target_rows
                .iter()
                .any(|row| row.time == overlap_end && row.cpu == 300.0)
        );

        let source_rows = record::Entity::find()
            .filter(record::Column::ServerId.eq("source-1"))
            .all(&db)
            .await
            .unwrap();
        assert!(source_rows.is_empty());
    }

    #[tokio::test]
    async fn merge_alert_state_keeps_target_when_rule_conflicts() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "target-1", "Target").await;
        insert_test_server(&db, "source-1", "Source").await;

        let now = Utc::now();
        alert_state::ActiveModel {
            rule_id: Set("rule-1".to_string()),
            server_id: Set("target-1".to_string()),
            first_triggered_at: Set(now),
            last_notified_at: Set(now),
            count: Set(5),
            resolved: Set(false),
            resolved_at: Set(None),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        alert_state::ActiveModel {
            rule_id: Set("rule-1".to_string()),
            server_id: Set("source-1".to_string()),
            first_triggered_at: Set(now),
            last_notified_at: Set(now),
            count: Set(1),
            resolved: Set(true),
            resolved_at: Set(Some(now)),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        RecoveryMergeService::merge_server_history_on_db(&db, "target-1", "source-1")
            .await
            .unwrap();

        let target_states = alert_state::Entity::find()
            .filter(alert_state::Column::ServerId.eq("target-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(target_states.len(), 1);
        assert_eq!(target_states[0].rule_id, "rule-1");
        assert_eq!(target_states[0].count, 5);
        assert!(!target_states[0].resolved);

        let source_states = alert_state::Entity::find()
            .filter(alert_state::Column::ServerId.eq("source-1"))
            .all(&db)
            .await
            .unwrap();
        assert!(source_states.is_empty());
    }

    #[tokio::test]
    async fn merge_server_history_can_be_rolled_back_atomically() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "target-1", "Target").await;
        insert_test_server(&db, "source-1", "Source").await;

        let before_overlap = NaiveDate::from_ymd_opt(2026, 4, 16)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
            .and_utc();
        let overlap = NaiveDate::from_ymd_opt(2026, 4, 16)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_utc();

        insert_record(&db, "target-1", before_overlap, 10.0).await;
        insert_record(&db, "target-1", overlap, 20.0).await;
        insert_record(&db, "source-1", overlap, 200.0).await;

        let txn = db.begin().await.unwrap();
        RecoveryMergeService::merge_server_history_on_txn(&txn, "target-1", "source-1")
            .await
            .unwrap();
        txn.rollback().await.unwrap();

        let target_rows = record::Entity::find()
            .filter(record::Column::ServerId.eq("target-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(target_rows.len(), 2);
        assert!(
            target_rows
                .iter()
                .any(|row| row.time == before_overlap && row.cpu == 10.0)
        );
        assert!(
            target_rows
                .iter()
                .any(|row| row.time == overlap && row.cpu == 20.0)
        );

        let source_rows = record::Entity::find()
            .filter(record::Column::ServerId.eq("source-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(source_rows.len(), 1);
        assert_eq!(source_rows[0].time, overlap);
        assert_eq!(source_rows[0].cpu, 200.0);
    }

    #[tokio::test]
    async fn rewrite_server_ids_json_replaces_source_with_target_once() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "target-1", "Target").await;
        insert_test_server(&db, "source-1", "Source").await;
        let now = Utc::now();

        alert_rule::ActiveModel {
            id: Set("rule-1".to_string()),
            name: Set("rule".to_string()),
            enabled: Set(true),
            rules_json: Set("[]".to_string()),
            trigger_mode: Set("any".to_string()),
            notification_group_id: Set(None),
            fail_trigger_tasks: Set(None),
            recover_trigger_tasks: Set(None),
            cover_type: Set("include".to_string()),
            server_ids_json: Set(Some(r#"["target-1","source-1","source-1"]"#.to_string())),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        service_monitor::ActiveModel {
            id: Set("monitor-1".to_string()),
            name: Set("monitor".to_string()),
            monitor_type: Set("http".to_string()),
            target: Set("https://example.com".to_string()),
            interval: Set(60),
            config_json: Set("{}".to_string()),
            notification_group_id: Set(None),
            retry_count: Set(0),
            server_ids_json: Set(Some(r#"["source-1","target-1","source-1"]"#.to_string())),
            enabled: Set(true),
            last_status: Set(None),
            consecutive_failures: Set(0),
            last_checked_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        RecoveryMergeService::rewrite_server_ids_json_tables(&db, "target-1", "source-1")
            .await
            .unwrap();

        let rule = alert_rule::Entity::find_by_id("rule-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rule.server_ids_json.as_deref(), Some(r#"["target-1"]"#));

        let monitor = service_monitor::Entity::find_by_id("monitor-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(monitor.server_ids_json.as_deref(), Some(r#"["target-1"]"#));
    }

    #[tokio::test]
    async fn finalize_target_server_row_copies_runtime_fields_and_cleans_source_rows() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "target-1", "Target").await;
        insert_test_server(&db, "source-1", "Source").await;
        let now = Utc::now();

        let mut source: server::ActiveModel = server::Entity::find_by_id("source-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .into();
        source.cpu_name = Set(Some("Ryzen".to_string()));
        source.cpu_cores = Set(Some(16));
        source.cpu_arch = Set(Some("x86_64".to_string()));
        source.os = Set(Some("Linux".to_string()));
        source.kernel_version = Set(Some("6.9.0".to_string()));
        source.mem_total = Set(Some(64));
        source.swap_total = Set(Some(32));
        source.disk_total = Set(Some(1024));
        source.ipv4 = Set(Some("1.2.3.4".to_string()));
        source.ipv6 = Set(Some("::1".to_string()));
        source.region = Set(Some("Taipei".to_string()));
        source.country_code = Set(Some("TW".to_string()));
        source.virtualization = Set(Some("kvm".to_string()));
        source.agent_version = Set(Some("1.2.3".to_string()));
        source.protocol_version = Set(4);
        source.features = Set(r#"["docker","process"]"#.to_string());
        source.last_remote_addr = Set(Some("192.0.2.10:9527".to_string()));
        source.fingerprint = Set(Some("fingerprint-123".to_string()));
        let source_model = source.update(&db).await.unwrap();

        server_tag::ActiveModel {
            server_id: Set("source-1".to_string()),
            tag: Set("temporary".to_string()),
        }
        .insert(&db)
        .await
        .unwrap();
        traffic_hourly::ActiveModel {
            server_id: Set("source-1".to_string()),
            hour: Set(now),
            bytes_in: Set(10),
            bytes_out: Set(20),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        traffic_daily::ActiveModel {
            server_id: Set("source-1".to_string()),
            date: Set(now.date_naive()),
            bytes_in: Set(30),
            bytes_out: Set(40),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        traffic_state::ActiveModel {
            server_id: Set("source-1".to_string()),
            last_in: Set(100),
            last_out: Set(200),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();

        RecoveryMergeService::finalize_target_server_row(&db, "target-1", &source_model)
            .await
            .unwrap();
        RecoveryMergeService::delete_intentionally_unmerged_source_rows(&db, "source-1")
            .await
            .unwrap();

        let target = server::Entity::find_by_id("target-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(target.cpu_name.as_deref(), Some("Ryzen"));
        assert_eq!(target.protocol_version, 4);
        assert_eq!(target.features, r#"["docker","process"]"#);
        assert_eq!(target.last_remote_addr.as_deref(), Some("192.0.2.10:9527"));
        assert_eq!(target.fingerprint.as_deref(), Some("fingerprint-123"));

        let source_tags = server_tag::Entity::find()
            .filter(server_tag::Column::ServerId.eq("source-1"))
            .all(&db)
            .await
            .unwrap();
        assert!(source_tags.is_empty());
    }
}
