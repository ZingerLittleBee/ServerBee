use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use sea_orm::Set;
use serde::Serialize;
use uuid::Uuid;

use crate::error::{ok, ApiResponse, AppError};
use crate::service::auth::AuthService;
use crate::service::config::ConfigService;
use crate::service::network_probe::NetworkProbeService;
use crate::state::AppState;

const CONFIG_KEY_AUTO_DISCOVERY: &str = "auto_discovery_key";

fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
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
        (status = 400, description = "Auto-discovery key not configured"),
        (status = 401, description = "Invalid auto-discovery key"),
    ),
    security(("bearer_token" = []))
)]
async fn register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<RegisterResponse>>, AppError> {
    // Rate limiting
    let ip = extract_client_ip(&headers);
    if !state.check_register_rate(&ip) {
        return Err(AppError::TooManyRequests(
            "Too many registration attempts, please try later".to_string(),
        ));
    }

    // Extract Bearer token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    // Verify against stored auto_discovery_key
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

    // Generate server_id
    let server_id = Uuid::new_v4().to_string();

    // Generate random token (32 bytes base64url)
    let plaintext_token = AuthService::generate_session_token();

    // Hash token with argon2
    let token_hash = AuthService::hash_password(&plaintext_token)?;
    let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];

    let now = Utc::now();

    // Create server record
    let new_server = crate::entity::server::ActiveModel {
        id: Set(server_id.clone()),
        token_hash: Set(token_hash),
        token_prefix: Set(token_prefix.to_string()),
        name: Set("New Server".to_string()),
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
        created_at: Set(now),
        updated_at: Set(now),
    };

    use sea_orm::ActiveModelTrait;
    new_server.insert(&state.db).await?;

    // Apply default network probe targets to the newly registered server
    if let Err(e) = NetworkProbeService::apply_defaults(&state.db, &server_id).await {
        tracing::warn!("Failed to apply default network probe targets to {server_id}: {e}");
    }

    ok(RegisterResponse {
        server_id,
        token: plaintext_token,
    })
}
