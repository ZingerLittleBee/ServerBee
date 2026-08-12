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
use crate::service::agent_manager::TerminalSessionEvent;
use crate::service::audit::AuditService;
use crate::service::high_risk_audit::TerminalAuditContext;
use crate::state::AppState;
use serverbee_common::constants::{MAX_WS_MESSAGE_SIZE, TERMINAL_IDLE_TIMEOUT_SECS};
use serverbee_common::protocol::ServerMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ws/terminal/{server_id}", get(terminal_ws_handler))
}

async fn terminal_ws_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(server_id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Terminal is admin-only and needs the terminal capability; the shared
    // gate audits denials under `terminal_open_denied`.
    match super::session::admin_capability_gate(
        &state,
        &headers,
        &ConnectInfo(addr),
        &server_id,
        serverbee_common::constants::CAP_TERMINAL,
        "terminal_open_denied",
    )
    .await
    {
        Ok(gate) => ws
            .max_message_size(MAX_WS_MESSAGE_SIZE)
            .on_upgrade(move |socket| {
                handle_terminal_ws(socket, state, server_id, gate.auth, gate.ip)
            }),
        Err(response) => response,
    }
}

/// Browser terminal WS message format (JSON)
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BrowserTerminalMessage {
    Input { data: String },
    Resize { rows: u16, cols: u16 },
}

async fn handle_terminal_ws(
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

    // Create unique session ID
    let session_id = uuid::Uuid::new_v4().to_string();

    tracing::info!("Terminal WS opened: session={session_id} server={server_id}");

    // Create channel for terminal output from agent → browser
    let (output_tx, mut output_rx) = mpsc::channel::<TerminalSessionEvent>(256);

    // Register terminal session in agent manager
    state
        .agent_manager
        .register_terminal_session(session_id.clone(), server_id.clone(), output_tx);

    // Send TerminalOpen to agent to create the PTY
    let agent_tx = match state.agent_manager.get_sender(&server_id) {
        Some(tx) => tx,
        None => {
            tracing::error!("Agent {server_id} not connected for terminal");
            state.agent_manager.unregister_terminal_session(&session_id);
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

    let started_at = chrono::Utc::now();
    state.terminal_audit_contexts.insert(
        session_id.clone(),
        TerminalAuditContext {
            server_id: server_id.clone(),
            user_id: user_id.clone(),
            ip: ip.clone(),
            started_at,
        },
    );
    let open_detail = serde_json::json!({
        "server_id": server_id,
        "session_id": session_id,
        "started_at": started_at,
    })
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &user_id,
        "terminal_opened",
        Some(&open_detail),
        &ip,
    )
    .await;

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
    let mut close_reason = "client_closed".to_string();

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
                        close_reason = "agent_disconnect".to_string();
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
                        close_reason = "client_closed".to_string();
                        break;
                    }
                    Some(Ok(Message::Ping(_))) => {
                        // axum auto-responds
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::debug!("Terminal WS error: {e}");
                        close_reason = "server_disconnect".to_string();
                        break;
                    }
                }
            }
            // Idle timeout
            () = &mut idle_timer => {
                tracing::info!("Terminal session {session_id} timed out after idle");
                let msg = serde_json::json!({"type": "error", "error": "Session timed out due to inactivity"});
                let _ = ws_sink.send(Message::Text(msg.to_string().into())).await;
                close_reason = "idle_timeout".to_string();
                break;
            }
            () = super::session::mobile_token_expired(mobile_expires) => {
                tracing::debug!("Terminal session {session_id} mobile token expired, closing");
                let msg = serde_json::json!({"type": "error", "error": "Session token expired"});
                let _ = ws_sink.send(Message::Text(msg.to_string().into())).await;
                close_reason = "token_expired".to_string();
                break;
            }
            () = &mut auth_lease => {
                tracing::debug!(user_id, "Terminal WS authorization revoked");
                let msg = serde_json::json!({"type": "error", "error": "Authorization revoked"});
                let _ = ws_sink.send(Message::Text(msg.to_string().into())).await;
                close_reason = "authorization_revoked".to_string();
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
    state.agent_manager.unregister_terminal_session(&session_id);
    if let Some((_, context)) = state.terminal_audit_contexts.remove(&session_id) {
        let ended_at = chrono::Utc::now();
        let duration_ms = (ended_at - context.started_at).num_milliseconds().max(0);
        let detail = serde_json::json!({
            "server_id": context.server_id,
            "session_id": session_id,
            "started_at": context.started_at,
            "ended_at": ended_at,
            "duration_ms": duration_ms,
            "close_reason": close_reason,
        })
        .to_string();
        let _ = AuditService::log(
            &state.db,
            &context.user_id,
            "terminal_closed",
            Some(&detail),
            &context.ip,
        )
        .await;
    }
    let _ = ws_sink.send(Message::Close(None)).await;

    tracing::info!("Terminal WS closed: session={session_id}");
}
