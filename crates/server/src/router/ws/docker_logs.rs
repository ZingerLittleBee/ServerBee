use std::sync::Arc;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::middleware::auth::AuthenticatedConnection;
use crate::service::audit::AuditService;
use crate::service::high_risk_audit::DockerLogsAuditContext;
use crate::state::AppState;
use serverbee_common::constants::{CAP_DOCKER, MAX_WS_MESSAGE_SIZE};
use serverbee_common::protocol::ServerMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ws/docker/logs/{server_id}", get(docker_logs_ws_handler))
}

async fn docker_logs_ws_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(server_id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Docker log streaming exposes sensitive container output (env vars,
    // connection strings, tokens), so it is admin-only like the terminal;
    // the shared gate audits denials under `docker_logs_subscribe_denied`.
    match super::session::admin_capability_gate(
        &state,
        &headers,
        &ConnectInfo(addr),
        &server_id,
        CAP_DOCKER,
        "docker_logs_subscribe_denied",
    )
    .await
    {
        Ok(gate) => ws
            .max_message_size(MAX_WS_MESSAGE_SIZE)
            .on_upgrade(move |socket| {
                handle_docker_logs_ws(socket, state, server_id, gate.auth, gate.ip)
            }),
        Err(response) => response,
    }
}

/// Browser -> Server messages for docker logs
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum DockerLogCommand {
    Subscribe {
        container_id: String,
        #[serde(default = "default_tail")]
        tail: Option<u64>,
        #[serde(default = "default_true")]
        follow: bool,
    },
    Unsubscribe,
}

fn default_tail() -> Option<u64> {
    Some(100)
}

fn default_true() -> bool {
    true
}

async fn handle_docker_logs_ws(
    socket: WebSocket,
    state: Arc<AppState>,
    server_id: String,
    auth: AuthenticatedConnection,
    ip: String,
) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    let user_id = auth.user.user_id.clone();
    let mobile_expires = auth.mobile_expires;
    let auth_lease = super::session::auth_lease_invalidated(&state, &auth);
    tokio::pin!(auth_lease);

    let session_id = uuid::Uuid::new_v4().to_string();

    tracing::info!("Docker logs WS opened: session={session_id} server={server_id}");

    // Create channel for log entries from agent -> browser
    let (log_tx, mut log_rx) = mpsc::channel(256);

    // Register the log session
    state
        .agent_manager
        .add_docker_log_session(&server_id, session_id.clone(), log_tx);

    // Send session_id to browser
    let _ = ws_sink
        .send(Message::Text(
            serde_json::json!({"type": "session", "session_id": &session_id})
                .to_string()
                .into(),
        ))
        .await;

    let agent_tx = state.agent_manager.get_sender(&server_id);

    let close_reason = loop {
        tokio::select! {
            // Agent -> Browser: forward log entries
            entries = log_rx.recv() => {
                match entries {
                    Some(entries) => {
                        let msg = serde_json::json!({"type": "logs", "entries": entries});
                        if ws_sink.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break "server_disconnect";
                        }
                    }
                    None => {
                        // Channel closed
                        break "agent_disconnect";
                    }
                }
            }
            // Browser -> Server: commands
            browser_msg = ws_stream.next() => {
                match browser_msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<DockerLogCommand>(&text) {
                            match cmd {
                                DockerLogCommand::Subscribe { container_id, tail, follow } => {
                                    let started_at = chrono::Utc::now();
                                    state.docker_logs_audit_contexts.insert(
                                        session_id.clone(),
                                        DockerLogsAuditContext {
                                            server_id: server_id.clone(),
                                            user_id: user_id.clone(),
                                            ip: ip.clone(),
                                            container_id: container_id.clone(),
                                            tail,
                                            follow,
                                            started_at,
                                        },
                                    );
                                    let detail = serde_json::json!({
                                        "server_id": server_id,
                                        "session_id": session_id,
                                        "container_id": container_id,
                                        "tail": tail,
                                        "follow": follow,
                                        "started_at": started_at,
                                    })
                                    .to_string();
                                    let _ = AuditService::log(
                                        &state.db,
                                        &user_id,
                                        "docker_logs_subscribed",
                                        Some(&detail),
                                        &ip,
                                    )
                                    .await;
                                    if let Some(ref tx) = agent_tx {
                                        let _ = tx.send(ServerMessage::DockerLogsStart {
                                            session_id: session_id.clone(),
                                            container_id,
                                            tail,
                                            follow,
                                        }).await;
                                    }
                                }
                                DockerLogCommand::Unsubscribe => {
                                    if let Some(ref tx) = agent_tx {
                                        let _ = tx.send(ServerMessage::DockerLogsStop {
                                            session_id: session_id.clone(),
                                        }).await;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break "client_closed";
                    }
                    Some(Ok(Message::Ping(_))) => {
                        // axum auto-responds
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::debug!("Docker logs WS error: {e}");
                        break "server_disconnect";
                    }
                }
            }
            () = super::session::mobile_token_expired(mobile_expires) => {
                tracing::debug!("Docker logs session {session_id} mobile token expired, closing");
                break "token_expired";
            }
            () = &mut auth_lease => {
                tracing::debug!(user_id, "Docker logs WS authorization revoked");
                break "authorization_revoked";
            }
        }
    };

    // Cleanup: stop the log stream on the agent side and unregister
    if let Some(ref tx) = agent_tx {
        let _ = tx
            .send(ServerMessage::DockerLogsStop {
                session_id: session_id.clone(),
            })
            .await;
    }
    state
        .agent_manager
        .remove_docker_log_session(&server_id, &session_id);
    if let Some((_, context)) = state.docker_logs_audit_contexts.remove(&session_id) {
        let ended_at = chrono::Utc::now();
        let duration_ms = (ended_at - context.started_at).num_milliseconds().max(0);
        let detail = serde_json::json!({
            "server_id": context.server_id,
            "session_id": session_id,
            "container_id": context.container_id,
            "tail": context.tail,
            "follow": context.follow,
            "started_at": context.started_at,
            "ended_at": ended_at,
            "duration_ms": duration_ms,
            "close_reason": close_reason,
        })
        .to_string();
        let _ = AuditService::log(
            &state.db,
            &context.user_id,
            "docker_logs_unsubscribed",
            Some(&detail),
            &context.ip,
        )
        .await;
    }

    tracing::info!("Docker logs WS closed: session={session_id}");
}
