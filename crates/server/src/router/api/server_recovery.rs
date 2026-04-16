use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use serde::{Deserialize, Serialize};
use serverbee_common::protocol::ServerMessage;
use tokio::sync::mpsc;

use crate::entity::{recovery_job, server};
use crate::error::{ApiResponse, AppError, ok};
use crate::router::ws::browser::broadcast_recovery_update;
use crate::service::recovery_job::RecoveryJobService;
use crate::service::recovery_merge::RecoveryMergeService;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryJobStatus {
    Running,
    Failed,
    Succeeded,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryJobStage {
    Validating,
    Rebinding,
    AwaitingTargetOnline,
    FreezingWrites,
    MergingHistory,
    Finalizing,
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RecoveryCandidateResponse {
    pub server_id: String,
    pub name: String,
    pub score: i32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StartRecoveryRequest {
    pub source_server_id: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RecoveryJobResponse {
    pub job_id: String,
    pub target_server_id: String,
    pub source_server_id: String,
    pub status: RecoveryJobStatus,
    pub stage: RecoveryJobStage,
    pub error: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug)]
struct CandidateScoreInput {
    same_remote_addr: bool,
    same_cpu_arch: bool,
    same_os: bool,
    same_virtualization: bool,
    created_within_minutes: i64,
    same_country: bool,
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/servers/{target_id}/recovery-candidates",
            get(list_candidates),
        )
        .route("/servers/recovery-jobs/{job_id}", get(get_recovery_job))
        .route(
            "/servers/{target_id}/recover-merge",
            post(start_recovery_merge),
        )
}

#[utoipa::path(
    get,
    path = "/api/servers/{target_id}/recovery-candidates",
    params(
        ("target_id" = String, Path, description = "Original offline server id")
    ),
    responses(
        (status = 200, description = "Recommended recovery candidates", body = Vec<RecoveryCandidateResponse>),
        (status = 401, description = "Authentication required", body = crate::error::ErrorBody),
        (status = 403, description = "Admin required", body = crate::error::ErrorBody),
        (status = 404, description = "Target server not found", body = crate::error::ErrorBody),
        (status = 409, description = "Target must be offline and not already in a running recovery job", body = crate::error::ErrorBody),
    ),
    security(
        ("session_cookie" = []),
        ("api_key" = []),
        ("bearer_token" = [])
    ),
    tag = "server-recovery"
)]
async fn list_candidates(
    State(state): State<Arc<AppState>>,
    Path(target_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<RecoveryCandidateResponse>>>, AppError> {
    let target = server::Entity::find_by_id(&target_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;

    if state.agent_manager.is_online(&target.id) {
        return Err(AppError::Conflict(
            "Target server must be offline before listing recovery candidates".to_string(),
        ));
    }

    let running_jobs = recovery_job::Entity::find()
        .filter(recovery_job::Column::Status.eq("running"))
        .all(&state.db)
        .await?;

    if running_jobs
        .iter()
        .any(|job| job.target_server_id == target.id || job.source_server_id == target.id)
    {
        return Err(AppError::Conflict(
            "Target server is already participating in a running recovery job".to_string(),
        ));
    }

    let active_server_ids: HashSet<String> = running_jobs
        .into_iter()
        .flat_map(|job| [job.target_server_id, job.source_server_id])
        .collect();

    let mut candidates = server::Entity::find()
        .filter(server::Column::Id.ne(target_id.as_str()))
        .all(&state.db)
        .await?
        .into_iter()
        .filter(|source| state.agent_manager.is_online(&source.id))
        .filter(|source| !active_server_ids.contains(&source.id))
        .map(|source| build_candidate_response(&target, &source))
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.server_id.cmp(&right.server_id))
    });

    ok(candidates)
}

#[utoipa::path(
    get,
    path = "/api/servers/recovery-jobs/{job_id}",
    params(
        ("job_id" = String, Path, description = "Recovery job id")
    ),
    responses(
        (status = 200, description = "Recovery job details", body = RecoveryJobResponse),
        (status = 401, description = "Authentication required", body = crate::error::ErrorBody),
        (status = 403, description = "Admin required", body = crate::error::ErrorBody),
        (status = 404, description = "Recovery job not found", body = crate::error::ErrorBody),
    ),
    security(
        ("session_cookie" = []),
        ("api_key" = []),
        ("bearer_token" = [])
    ),
    tag = "server-recovery"
)]
async fn get_recovery_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<ApiResponse<RecoveryJobResponse>>, AppError> {
    let job = RecoveryJobService::get_job(&state.db, &job_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Recovery job not found".to_string()))?;

    ok(job.into())
}

#[utoipa::path(
    post,
    path = "/api/servers/{target_id}/recover-merge",
    request_body = StartRecoveryRequest,
    params(
        ("target_id" = String, Path, description = "Original offline server id")
    ),
    responses(
        (status = 200, description = "Recovery job created", body = RecoveryJobResponse),
        (status = 401, description = "Authentication required", body = crate::error::ErrorBody),
        (status = 403, description = "Admin required", body = crate::error::ErrorBody),
        (status = 404, description = "Server not found", body = crate::error::ErrorBody),
        (status = 409, description = "Recovery cannot be started in the current state", body = crate::error::ErrorBody),
        (status = 422, description = "Invalid request", body = crate::error::ErrorBody),
    ),
    security(
        ("session_cookie" = []),
        ("api_key" = []),
        ("bearer_token" = [])
    ),
    tag = "server-recovery"
)]
async fn start_recovery_merge(
    State(state): State<Arc<AppState>>,
    Path(target_id): Path<String>,
    Json(request): Json<StartRecoveryRequest>,
) -> Result<Json<ApiResponse<RecoveryJobResponse>>, AppError> {
    let sender = state.agent_manager.get_sender(&request.source_server_id);
    let job =
        start_recovery_merge_with_sender(&state, &target_id, &request.source_server_id, sender)
            .await?;
    broadcast_recovery_update(&state).await;
    ok(job.into())
}

async fn start_recovery_merge_with_sender(
    state: &Arc<AppState>,
    target_id: &str,
    source_server_id: &str,
    sender: Option<mpsc::Sender<ServerMessage>>,
) -> Result<recovery_job::Model, AppError> {
    let sender = sender.ok_or_else(|| {
        AppError::Conflict("Source server must be online before starting recovery".to_string())
    })?;

    RecoveryMergeService::validate_start_request(state, target_id, source_server_id).await?;

    let txn = state.db.begin().await?;
    let job = RecoveryMergeService::start_on_txn(&txn, target_id, source_server_id).await?;
    let token = RecoveryMergeService::rotate_target_token_on_txn(&txn, target_id).await?;

    if let Err(error) = sender
        .send(ServerMessage::RebindIdentity {
            job_id: job.job_id.clone(),
            target_server_id: target_id.to_string(),
            token,
        })
        .await
    {
        txn.rollback().await?;
        return Err(AppError::Internal(format!(
            "Failed to dispatch RebindIdentity to source agent: {error}"
        )));
    }

    txn.commit().await?;
    Ok(job)
}

fn build_candidate_response(
    target: &server::Model,
    source: &server::Model,
) -> RecoveryCandidateResponse {
    let same_remote_addr = remote_addr_key(target.last_remote_addr.as_deref())
        .zip(remote_addr_key(source.last_remote_addr.as_deref()))
        .is_some_and(|(left, right)| left == right);
    let same_cpu_arch = option_eq(target.cpu_arch.as_deref(), source.cpu_arch.as_deref());
    let same_os = option_eq(target.os.as_deref(), source.os.as_deref());
    let same_virtualization = option_eq(
        target.virtualization.as_deref(),
        source.virtualization.as_deref(),
    );
    let same_country = option_eq(
        target.country_code.as_deref(),
        source.country_code.as_deref(),
    ) || option_eq(target.region.as_deref(), source.region.as_deref());
    let created_within_minutes = (source.created_at - target.created_at).num_minutes().abs();

    let score = score_candidate(CandidateScoreInput {
        same_remote_addr,
        same_cpu_arch,
        same_os,
        same_virtualization,
        created_within_minutes,
        same_country,
    });

    let mut reasons = Vec::new();
    if same_remote_addr {
        reasons.push("same remote address".to_string());
    }
    if same_cpu_arch {
        reasons.push("same cpu architecture".to_string());
    }
    if same_os {
        reasons.push("same operating system".to_string());
    }
    if same_virtualization {
        reasons.push("same virtualization".to_string());
    }
    if same_country {
        reasons.push("same region or country".to_string());
    }
    if created_within_minutes <= 60 {
        reasons.push("created close in time".to_string());
    }
    if reasons.is_empty() {
        reasons.push("online replacement candidate".to_string());
    }

    RecoveryCandidateResponse {
        server_id: source.id.clone(),
        name: source.name.clone(),
        score,
        reasons,
    }
}

fn score_candidate(input: CandidateScoreInput) -> i32 {
    let mut score = 0;

    if input.same_remote_addr {
        score += 40;
    }
    if input.same_cpu_arch {
        score += 15;
    }
    if input.same_os {
        score += 15;
    }
    if input.same_virtualization {
        score += 10;
    }
    if input.same_country {
        score += 10;
    }

    score
        + match input.created_within_minutes {
            0..=15 => 20,
            16..=60 => 12,
            61..=240 => 4,
            _ => 0,
        }
}

fn option_eq(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn remote_addr_key(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }

    if let Ok(addr) = SocketAddr::from_str(value) {
        return Some(addr.ip().to_string());
    }

    Some(value.to_string())
}

impl From<recovery_job::Model> for RecoveryJobResponse {
    fn from(value: recovery_job::Model) -> Self {
        Self {
            job_id: value.job_id,
            target_server_id: value.target_server_id,
            source_server_id: value.source_server_id,
            status: RecoveryJobStatus::from(value.status.as_str()),
            stage: RecoveryJobStage::from(value.stage.as_str()),
            error: value.error,
            started_at: value.started_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
            last_heartbeat_at: value.last_heartbeat_at,
        }
    }
}

impl From<&str> for RecoveryJobStatus {
    fn from(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "failed" => Self::Failed,
            "succeeded" => Self::Succeeded,
            _ => Self::Unknown,
        }
    }
}

impl From<&str> for RecoveryJobStage {
    fn from(value: &str) -> Self {
        match value {
            "validating" => Self::Validating,
            "rebinding" => Self::Rebinding,
            "awaiting_target_online" => Self::AwaitingTargetOnline,
            "freezing_writes" => Self::FreezingWrites,
            "merging_history" => Self::MergingHistory,
            "finalizing" => Self::Finalizing,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CandidateScoreInput, RecoveryJobStage, StartRecoveryRequest, score_candidate,
        start_recovery_merge, start_recovery_merge_with_sender,
    };
    use crate::config::AppConfig;
    use crate::entity::{recovery_job, server};
    use crate::error::AppError;
    use crate::service::auth::AuthService;
    use crate::state::AppState;
    use crate::test_utils::setup_test_db;
    use axum::Json;
    use axum::extract::{Path, State};
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use serverbee_common::constants::CAP_DEFAULT;
    use serverbee_common::protocol::ServerMessage;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::time::{Duration, timeout};

    async fn insert_server(db: &sea_orm::DatabaseConnection, id: &str, name: &str) {
        let now = Utc::now();
        let token_hash = AuthService::hash_password("test").unwrap();
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
        .unwrap();
    }

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9527)
    }

    #[test]
    fn higher_score_when_ip_arch_and_created_at_match() {
        let strong = score_candidate(CandidateScoreInput {
            same_remote_addr: true,
            same_cpu_arch: true,
            same_os: true,
            same_virtualization: true,
            created_within_minutes: 10,
            same_country: true,
        });
        let weak = score_candidate(CandidateScoreInput {
            same_remote_addr: false,
            same_cpu_arch: false,
            same_os: true,
            same_virtualization: false,
            created_within_minutes: 240,
            same_country: false,
        });

        assert!(strong > weak);
    }

    #[tokio::test]
    async fn start_recovery_merge_returns_rebinding_stage() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();

        let (tx, mut rx) = mpsc::channel(1);
        state
            .agent_manager
            .add_connection("source-1".into(), "Source".into(), tx, test_addr());

        let Json(response) = start_recovery_merge(
            State(Arc::clone(&state)),
            Path("target-1".to_string()),
            Json(StartRecoveryRequest {
                source_server_id: "source-1".to_string(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(response.data.stage, RecoveryJobStage::Rebinding);
        let _message = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("rebind command should be sent in time")
            .expect("rebind command channel should stay open");
    }

    #[tokio::test]
    async fn start_recovery_merge_sends_rebind_identity_command() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let (tx, mut rx) = mpsc::channel(1);
        state
            .agent_manager
            .add_connection("source-1".into(), "Source".into(), tx, test_addr());

        let Json(response) = start_recovery_merge(
            State(Arc::clone(&state)),
            Path("target-1".to_string()),
            Json(StartRecoveryRequest {
                source_server_id: "source-1".to_string(),
            }),
        )
        .await
        .unwrap();

        let message = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("rebind command should be sent in time")
            .expect("rebind command channel should stay open");
        let token = match message {
            ServerMessage::RebindIdentity {
                job_id,
                target_server_id,
                token,
            } => {
                assert_eq!(job_id, response.data.job_id);
                assert_eq!(target_server_id, "target-1");
                token
            }
            other => panic!("expected rebind command, got {other:?}"),
        };

        let target = server::Entity::find_by_id("target-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(target.token_prefix, token[..8.min(token.len())]);
        let validated = AuthService::validate_agent_token(&db, &token)
            .await
            .unwrap()
            .expect("target token should validate");
        assert_eq!(validated.id, "target-1");
    }

    #[tokio::test]
    async fn start_recovery_merge_fails_safely_when_sender_missing() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let before = server::Entity::find_by_id("target-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();

        let error = start_recovery_merge_with_sender(&state, "target-1", "source-1", None)
            .await
            .expect_err("missing sender should fail safely");

        assert!(
            matches!(error, AppError::Conflict(message) if message.contains("Source server must be online"))
        );

        let after = server::Entity::find_by_id("target-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.token_prefix, before.token_prefix);
        assert_eq!(after.token_hash, before.token_hash);

        let jobs = recovery_job::Entity::find().all(&db).await.unwrap();
        assert!(jobs.is_empty(), "no recovery job should be persisted");
    }

    #[tokio::test]
    async fn start_recovery_merge_fails_safely_when_dispatch_fails() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        state
            .agent_manager
            .add_connection("source-1".into(), "Source".into(), tx, test_addr());

        let before = server::Entity::find_by_id("target-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();

        let error = start_recovery_merge(
            State(Arc::clone(&state)),
            Path("target-1".to_string()),
            Json(StartRecoveryRequest {
                source_server_id: "source-1".to_string(),
            }),
        )
        .await
        .expect_err("dispatch failure should fail safely");

        assert!(
            matches!(error, AppError::Internal(message) if message.contains("Failed to dispatch RebindIdentity"))
        );

        let after = server::Entity::find_by_id("target-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.token_prefix, before.token_prefix);
        assert_eq!(after.token_hash, before.token_hash);

        let jobs = recovery_job::Entity::find().all(&db).await.unwrap();
        assert!(
            jobs.is_empty(),
            "no recovery job should remain after failed dispatch"
        );
    }
}
