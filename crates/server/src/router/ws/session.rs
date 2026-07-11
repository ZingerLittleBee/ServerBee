//! Shared session scaffold for browser-facing WS routes.
//!
//! Every control-plane WS route must make the same security-sensitive
//! decisions before upgrading (credential policy → admin role → agent online
//! → capability, denials audited) and watch the same mobile-token deadline
//! while pumping. Both live here so a fix applies to every route at once;
//! the routes keep only their own pumps.

use std::sync::Arc;

use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::middleware::auth::resolve_ws_connection;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::capability_gate::require_capability_audited;
use crate::state::AppState;

/// What a gated WS pump needs from the pre-upgrade checks.
pub(super) struct WsGate {
    pub user_id: String,
    pub ip: String,
    pub mobile_expires: Option<chrono::DateTime<chrono::Utc>>,
}

/// Admin gate for control-plane WS routes (terminal, docker logs).
///
/// Runs the shared credential policy, then requires the admin role, an online
/// agent, and `capability` — auditing role/capability denials under
/// `denied_action`. Returns the ready-to-serve response on any failure so the
/// route handler can simply `match`.
pub(super) async fn admin_capability_gate(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    addr: &ConnectInfo<std::net::SocketAddr>,
    server_id: &str,
    capability: u32,
    denied_action: &str,
) -> Result<WsGate, Response> {
    let ip = extract_client_ip(addr, headers, &state.config.server.trusted_proxies).to_string();

    let Some(conn) = resolve_ws_connection(headers, state).await else {
        return Err(axum::http::StatusCode::UNAUTHORIZED.into_response());
    };
    let user_id = conn.user.user_id;

    if conn.user.role != "admin" {
        let detail = serde_json::json!({
            "server_id": server_id,
            "deny_reason": "role_forbidden",
        })
        .to_string();
        let _ = AuditService::log(&state.db, &user_id, denied_action, Some(&detail), &ip).await;
        return Err(axum::http::StatusCode::FORBIDDEN.into_response());
    }

    if !state.agent_manager.is_online(server_id) {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Agent is offline").into_response());
    }

    if let Err(error) =
        require_capability_audited(state, server_id, capability, &user_id, &ip, denied_action)
            .await
    {
        return Err(error.into_response());
    }

    Ok(WsGate {
        user_id,
        ip,
        mobile_expires: conn.mobile_expires,
    })
}

/// Resolves when a fixed-lifetime mobile token expires; pends forever for web
/// sessions and API keys. The shared `select!` arm of every browser-facing
/// WS pump.
pub(super) async fn mobile_token_expired(expires: Option<chrono::DateTime<chrono::Utc>>) {
    match expires {
        Some(exp) => {
            let dur = (exp - chrono::Utc::now()).to_std().unwrap_or_default();
            tokio::time::sleep(dur).await;
        }
        None => std::future::pending::<()>().await,
    }
}
