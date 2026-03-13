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

use crate::service::agent_manager::TerminalSessionEvent;
use crate::state::AppState;
use serverbee_common::constants::TERMINAL_IDLE_TIMEOUT_SECS;
use serverbee_common::protocol::ServerMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ws/terminal/{server_id}", get(terminal_ws_handler))
}

async fn terminal_ws_handler(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Auth: session cookie or API key
    let user = validate_auth(&state, &headers).await;
    match user {
        Some((_, role)) => {
            // Terminal access is admin-only
            if role != "admin" {
                return axum::http::StatusCode::FORBIDDEN.into_response();
            }
            // Check agent is online
            if !state.agent_manager.is_online(&server_id) {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    "Agent is offline",
                )
                    .into_response();
            }
            ws.on_upgrade(move |socket| handle_terminal_ws(socket, state, server_id))
        }
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn validate_auth(state: &Arc<AppState>, headers: &HeaderMap) -> Option<(String, String)> {
    use crate::service::auth::AuthService;

    // Try session cookie
    if let Some(token) = extract_session_cookie(headers)
        && let Ok(Some(user)) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
    {
        return Some((user.id, user.role));
    }

    // Try API key header
    if let Some(key) = extract_api_key(headers)
        && let Ok(Some(user)) = AuthService::validate_api_key(&state.db, &key).await
    {
        return Some((user.id, user.role));
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

/// Browser terminal WS message format (JSON)
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BrowserTerminalMessage {
    Input { data: String },
    Resize { rows: u16, cols: u16 },
}

async fn handle_terminal_ws(socket: WebSocket, state: Arc<AppState>, server_id: String) {
    let (mut ws_sink, mut ws_stream) = socket.split();

    // Create unique session ID
    let session_id = uuid::Uuid::new_v4().to_string();

    tracing::info!("Terminal WS opened: session={session_id} server={server_id}");

    // Create channel for terminal output from agent → browser
    let (output_tx, mut output_rx) = mpsc::channel::<TerminalSessionEvent>(256);

    // Register terminal session in agent manager
    state
        .agent_manager
        .register_terminal_session(session_id.clone(), output_tx);

    // Send TerminalOpen to agent to create the PTY
    let agent_tx = match state.agent_manager.get_sender(&server_id) {
        Some(tx) => tx,
        None => {
            tracing::error!("Agent {server_id} not connected for terminal");
            state
                .agent_manager
                .unregister_terminal_session(&session_id);
            let _ = ws_sink
                .send(Message::Text(
                    serde_json::json!({"type": "error", "error": "Agent disconnected"})
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    };

    // Send initial open with default size (will be resized by browser)
    let _ = agent_tx
        .send(ServerMessage::TerminalOpen {
            session_id: session_id.clone(),
            rows: 24,
            cols: 80,
        })
        .await;

    // Send session_id to browser so it knows the session is ready
    let _ = ws_sink
        .send(Message::Text(
            serde_json::json!({"type": "session", "session_id": &session_id})
                .to_string()
                .into(),
        ))
        .await;

    // Idle timeout
    let idle_duration = std::time::Duration::from_secs(TERMINAL_IDLE_TIMEOUT_SECS);
    let idle_timer = tokio::time::sleep(idle_duration);
    tokio::pin!(idle_timer);

    loop {
        tokio::select! {
            // Agent → Browser: forward terminal output
            event = output_rx.recv() => {
                match event {
                    Some(TerminalSessionEvent::Output(data)) => {
                        let msg = serde_json::json!({"type": "output", "data": data});
                        if ws_sink.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Some(TerminalSessionEvent::Started) => {
                        let msg = serde_json::json!({"type": "started"});
                        let _ = ws_sink.send(Message::Text(msg.to_string().into())).await;
                    }
                    Some(TerminalSessionEvent::Error(error)) => {
                        let msg = serde_json::json!({"type": "error", "error": error});
                        let _ = ws_sink.send(Message::Text(msg.to_string().into())).await;
                    }
                    None => {
                        // Channel closed, agent disconnected
                        break;
                    }
                }
            }
            // Browser → Agent: forward input/resize
            browser_msg = ws_stream.next() => {
                match browser_msg {
                    Some(Ok(Message::Text(text))) => {
                        // Reset idle timer on input
                        idle_timer.as_mut().reset(tokio::time::Instant::now() + idle_duration);

                        if let Ok(msg) = serde_json::from_str::<BrowserTerminalMessage>(&text) {
                            match msg {
                                BrowserTerminalMessage::Input { data } => {
                                    let _ = agent_tx.send(ServerMessage::TerminalInput {
                                        session_id: session_id.clone(),
                                        data,
                                    }).await;
                                }
                                BrowserTerminalMessage::Resize { rows, cols } => {
                                    let _ = agent_tx.send(ServerMessage::TerminalResize {
                                        session_id: session_id.clone(),
                                        rows,
                                        cols,
                                    }).await;
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
                        tracing::debug!("Terminal WS error: {e}");
                        break;
                    }
                }
            }
            // Idle timeout
            () = &mut idle_timer => {
                tracing::info!("Terminal session {session_id} timed out after idle");
                let msg = serde_json::json!({"type": "error", "error": "Session timed out due to inactivity"});
                let _ = ws_sink.send(Message::Text(msg.to_string().into())).await;
                break;
            }
        }
    }

    // Cleanup: close agent-side session and unregister
    let _ = agent_tx
        .send(ServerMessage::TerminalClose {
            session_id: session_id.clone(),
        })
        .await;
    state
        .agent_manager
        .unregister_terminal_session(&session_id);

    tracing::info!("Terminal WS closed: session={session_id}");
}
