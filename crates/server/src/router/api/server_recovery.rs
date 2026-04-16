use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

use crate::entity::{recovery_job, server};
use crate::error::{ApiResponse, AppError, ok};
use crate::service::recovery_job::RecoveryJobService;
use crate::state::AppState;

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
    pub status: String,
    pub stage: String,
    pub checkpoint_json: Option<String>,
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
        .route(
            "/servers/{target_id}/recovery-candidates",
            get(list_candidates),
        )
        .route("/servers/recovery-jobs/{job_id}", get(get_recovery_job))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route(
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
        (status = 404, description = "Target server not found", body = crate::error::ErrorBody),
    ),
    security(
        ("session_cookie" = []),
        ("api_key" = [])
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

    let running_jobs = recovery_job::Entity::find()
        .filter(recovery_job::Column::Status.eq("running"))
        .all(&state.db)
        .await?;

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
        (status = 404, description = "Recovery job not found", body = crate::error::ErrorBody),
    ),
    security(
        ("session_cookie" = []),
        ("api_key" = [])
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
        ("api_key" = [])
    ),
    tag = "server-recovery"
)]
async fn start_recovery_merge(
    State(state): State<Arc<AppState>>,
    Path(target_id): Path<String>,
    Json(request): Json<StartRecoveryRequest>,
) -> Result<Json<ApiResponse<RecoveryJobResponse>>, AppError> {
    if request.source_server_id == target_id {
        return Err(AppError::Validation(
            "source_server_id must be different from target_id".to_string(),
        ));
    }

    let target = server::Entity::find_by_id(&target_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;
    let source = server::Entity::find_by_id(&request.source_server_id)
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

    let job = RecoveryJobService::create_job(&state.db, &target.id, &source.id).await?;
    ok(job.into())
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
            status: value.status,
            stage: value.stage,
            checkpoint_json: value.checkpoint_json,
            error: value.error,
            started_at: value.started_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
            last_heartbeat_at: value.last_heartbeat_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CandidateScoreInput, score_candidate};

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
}
