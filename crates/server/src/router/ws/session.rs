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

use crate::middleware::auth::AuthenticatedConnection;
use crate::middleware::auth::resolve_ws_connection;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::capability_gate::require_capability_audited;
use crate::state::AppState;

/// What a gated WS pump needs from the pre-upgrade checks.
pub(super) struct WsGate {
    pub auth: AuthenticatedConnection,
    pub ip: String,
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
    let user_id = conn.user.user_id.clone();

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
        require_capability_audited(state, server_id, capability, &user_id, &ip, denied_action).await
    {
        return Err(error.into_response());
    }

    Ok(WsGate { auth: conn, ip })
}

#[cfg(test)]
const AUTH_LEASE_RECHECK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
#[cfg(not(test))]
const AUTH_LEASE_RECHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Resolves when a WebSocket's persisted credential is revoked or its user
/// policy changes. Temporary database failures are logged and retried so an
/// availability incident does not disconnect every active client at once.
pub(super) async fn auth_lease_invalidated(state: &AppState, auth: &AuthenticatedConnection) {
    let mut interval = tokio::time::interval(AUTH_LEASE_RECHECK_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        interval.tick().await;
        match auth.lease_is_valid(state).await {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => {
                tracing::warn!(
                    user_id = %auth.user.user_id,
                    error = %error,
                    "Failed to refresh WebSocket authorization lease"
                );
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::user;
    use crate::service::auth::{AuthService, LoginParams};
    use crate::test_utils::setup_test_db;
    use sea_orm::{ActiveModelTrait, Set};

    #[tokio::test]
    async fn session_logout_invalidates_existing_ws_lease() {
        let (db, _temp) = setup_test_db().await;
        AuthService::create_user(&db, "ws-user", "password123", "member")
            .await
            .expect("create user");
        let (session, _) = AuthService::login(
            &db,
            LoginParams {
                username: "ws-user",
                password: "password123",
                totp_code: None,
                ip: "127.0.0.1",
                user_agent: "test",
                session_ttl: 3600,
            },
        )
        .await
        .expect("login");
        let state = AppState::new(db, AppConfig::default())
            .await
            .expect("app state");
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            format!("session_token={}", session.token)
                .parse()
                .expect("cookie header"),
        );
        let auth = resolve_ws_connection(&headers, &state)
            .await
            .expect("authenticated connection");
        assert!(auth.lease_is_valid(&state).await.expect("lease check"));

        AuthService::logout(&state.db, &session.token)
            .await
            .expect("logout");
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            auth_lease_invalidated(&state, &auth),
        )
        .await
        .expect("revoked lease should resolve");
    }

    #[tokio::test]
    async fn api_key_deletion_invalidates_existing_ws_lease() {
        let (db, _temp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "key-user", "password123", "admin")
            .await
            .expect("create user");
        let (key, secret) = AuthService::create_api_key(&db, &user.id, "ws")
            .await
            .expect("create API key");
        let state = AppState::new(db, AppConfig::default())
            .await
            .expect("app state");
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", secret.parse().expect("API key header"));
        let auth = resolve_ws_connection(&headers, &state)
            .await
            .expect("authenticated connection");

        AuthService::delete_api_key(&state.db, &key.id, &user.id)
            .await
            .expect("delete API key");
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            auth_lease_invalidated(&state, &auth),
        )
        .await
        .expect("revoked lease should resolve");
    }

    #[tokio::test]
    async fn role_change_invalidates_existing_ws_lease() {
        let (db, _temp) = setup_test_db().await;
        let user = AuthService::create_user(&db, "role-user", "password123", "admin")
            .await
            .expect("create user");
        let (_, secret) = AuthService::create_api_key(&db, &user.id, "ws")
            .await
            .expect("create API key");
        let state = AppState::new(db, AppConfig::default())
            .await
            .expect("app state");
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", secret.parse().expect("API key header"));
        let auth = resolve_ws_connection(&headers, &state)
            .await
            .expect("authenticated connection");

        let mut active: user::ActiveModel = user.into();
        active.role = Set("member".to_string());
        active.update(&state.db).await.expect("demote user");
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            auth_lease_invalidated(&state, &auth),
        )
        .await
        .expect("role change should invalidate lease");
    }
}
