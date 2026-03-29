use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::server;
use crate::error::{ok, ApiResponse, AppError};
use crate::router::utils::extract_client_ip;
use crate::service::auth::AuthService;
use crate::service::config::ConfigService;
use crate::service::network_probe::NetworkProbeService;
use crate::state::AppState;

const CONFIG_KEY_AUTO_DISCOVERY: &str = "auto_discovery_key";
const DEFAULT_SERVER_NAME: &str = "New Server";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    #[serde(default)]
    fingerprint: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    server_id: String,
    token: String,
}

/// Public routes for agent registration (Bearer auth checked inside handler).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/register", post(register))
}

#[utoipa::path(
    post,
    path = "/api/agent/register",
    tag = "agent",
    responses(
        (status = 200, description = "Agent registered", body = RegisterResponse),
        (status = 400, description = "Auto-discovery key not configured or server limit reached"),
        (status = 401, description = "Invalid auto-discovery key"),
    ),
    security(("bearer_token" = []))
)]
async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Option<Json<RegisterRequest>>,
) -> Result<Json<ApiResponse<RegisterResponse>>, AppError> {
    // 1. Rate limiting
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    if !state.check_register_rate(&ip) {
        return Err(AppError::TooManyRequests(
            "Too many registration attempts, please try later".to_string(),
        ));
    }

    // 2. Discovery key validation
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let stored_key = ConfigService::get(&state.db, CONFIG_KEY_AUTO_DISCOVERY)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest("Auto-discovery key not configured".to_string())
        })?;

    if stored_key.is_empty() {
        return Err(AppError::BadRequest(
            "Auto-discovery key not configured".to_string(),
        ));
    }

    if auth_header != stored_key {
        return Err(AppError::Unauthorized);
    }

    let fingerprint = body
        .as_ref()
        .map(|b| b.fingerprint.clone())
        .unwrap_or_default();

    // Validate fingerprint format if provided
    if !fingerprint.is_empty()
        && (fingerprint.len() != 64 || !fingerprint.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(AppError::BadRequest(
            "Invalid fingerprint format".to_string(),
        ));
    }

    // 3. Fingerprint dedup: try to reuse existing server
    if !fingerprint.is_empty()
        && let Some(existing) = server::Entity::find()
            .filter(server::Column::Fingerprint.eq(&fingerprint))
            .one(&state.db)
            .await?
    {
        let server_id = existing.id.clone();
        tracing::info!("Reusing server {server_id} for fingerprint {fingerprint}");

        let plaintext_token = AuthService::generate_session_token();
        let token_hash = AuthService::hash_password(&plaintext_token)?;
        let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];

        let mut active: server::ActiveModel = existing.into();
        active.token_hash = Set(token_hash);
        active.token_prefix = Set(token_prefix.to_string());
        active.last_remote_addr = Set(Some(ip));
        active.updated_at = Set(Utc::now());
        active.update(&state.db).await?;

        return ok(RegisterResponse {
            server_id,
            token: plaintext_token,
        });
    }

    // 4. Global server limit check (soft cap, only for new servers)
    let max_servers = state.config.auth.max_servers;
    if max_servers > 0 {
        let count = server::Entity::find().count(&state.db).await?;
        if count >= max_servers as u64 {
            return Err(AppError::BadRequest(format!(
                "Server limit reached ({max_servers}). Delete unused servers or increase max_servers in config."
            )));
        }
    }

    // 5. Create new server
    let server_id = Uuid::new_v4().to_string();
    let plaintext_token = AuthService::generate_session_token();
    let token_hash = AuthService::hash_password(&plaintext_token)?;
    let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];
    let now = Utc::now();

    let fp = if fingerprint.is_empty() {
        None
    } else {
        Some(fingerprint.clone())
    };

    let new_server = server::ActiveModel {
        id: Set(server_id.clone()),
        token_hash: Set(token_hash),
        token_prefix: Set(token_prefix.to_string()),
        name: Set(DEFAULT_SERVER_NAME.to_string()),
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
        capabilities: Set(56),
        protocol_version: Set(1),
        features: Set("[]".to_string()),
        last_remote_addr: Set(Some(ip.clone())),
        fingerprint: Set(fp.clone()),
        created_at: Set(now),
        updated_at: Set(now),
    };

    // Handle race condition: if another request with the same fingerprint inserted
    // between our SELECT and INSERT, catch the unique constraint violation and retry as reuse.
    match new_server.insert(&state.db).await {
        Ok(_) => {}
        Err(DbErr::Query(ref e)) if fp.is_some() && e.to_string().contains("UNIQUE") => {
            tracing::info!("Fingerprint race detected, falling back to reuse path");
            if let Some(existing) = server::Entity::find()
                .filter(server::Column::Fingerprint.eq(fp.as_ref().unwrap()))
                .one(&state.db)
                .await?
            {
                let server_id = existing.id.clone();
                let plaintext_token = AuthService::generate_session_token();
                let token_hash = AuthService::hash_password(&plaintext_token)?;
                let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];

                let mut active: server::ActiveModel = existing.into();
                active.token_hash = Set(token_hash);
                active.token_prefix = Set(token_prefix.to_string());
                active.last_remote_addr = Set(Some(ip));
                active.updated_at = Set(Utc::now());
                active.update(&state.db).await?;

                return ok(RegisterResponse {
                    server_id,
                    token: plaintext_token,
                });
            }
            return Err(AppError::Internal("Fingerprint race recovery failed".to_string()));
        }
        Err(e) => return Err(e.into()),
    }

    // Apply default network probe targets
    if let Err(e) = NetworkProbeService::apply_defaults(&state.db, &server_id).await {
        tracing::warn!("Failed to apply default network probe targets to {server_id}: {e}");
    }

    ok(RegisterResponse {
        server_id,
        token: plaintext_token,
    })
}
