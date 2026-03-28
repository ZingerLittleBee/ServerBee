use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::state::AppState;
use serverbee_common::constants::{has_capability, CAP_DOCKER, MAX_WS_MESSAGE_SIZE};
use serverbee_common::protocol::ServerMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ws/docker/logs/{server_id}", get(docker_logs_ws_handler))
}

async fn docker_logs_ws_handler(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Auth: session cookie or API key
    let user = validate_auth(&state, &headers).await;
    match user {
        Some(_) => {
            // Check agent is online
            if !state.agent_manager.is_online(&server_id) {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    "Agent is offline",
                )
                    .into_response();
            }
            // Check Docker capability
            match crate::service::server::ServerService::get_server(&state.db, &server_id).await {
                Ok(server) => {
                    if !has_capability(server.capabilities as u32, CAP_DOCKER) {
                        return (
                            axum::http::StatusCode::FORBIDDEN,
                            "Docker is disabled for this server",
                        )
                            .into_response();
                    }
                }
                Err(_) => {
                    return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
            ws.max_message_size(MAX_WS_MESSAGE_SIZE)
                .on_upgrade(move |socket| handle_docker_logs_ws(socket, state, server_id))
        }
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn validate_auth(state: &Arc<AppState>, headers: &HeaderMap) -> Option<String> {
    use crate::service::auth::AuthService;

    // Try session cookie
    if let Some(token) = extract_session_cookie(headers)
        && let Ok(Some((user, _))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
    {
        return Some(user.id);
    }

    // Try API key header
    if let Some(key) = extract_api_key(headers)
        && let Ok(Some(user)) = AuthService::validate_api_key(&state.db, &key).await
    {
        return Some(user.id);
    }

    None
}

fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie
                .strip_prefix("session_token=")
                .map(|v| v.to_string())
        })
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")?
        .to_str()
        .ok()
        .map(|s| s.to_string())
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

async fn handle_docker_logs_ws(socket: WebSocket, state: Arc<AppState>, server_id: String) {
    let (mut ws_sink, mut ws_stream) = socket.split();

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

    loop {
        tokio::select! {
            // Agent -> Browser: forward log entries
            entries = log_rx.recv() => {
                match entries {
                    Some(entries) => {
                        let msg = serde_json::json!({"type": "logs", "entries": entries});
                        if ws_sink.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        // Channel closed
                        break;
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
                        break;
                    }
                    Some(Ok(Message::Ping(_))) => {
                        // axum auto-responds
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::debug!("Docker logs WS error: {e}");
                        break;
                    }
                }
            }
        }
    }

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

    tracing::info!("Docker logs WS closed: session={session_id}");
}
