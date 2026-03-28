use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};

use crate::service::auth::AuthService;
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::constants::MAX_WS_MESSAGE_SIZE;
use serverbee_common::protocol::{BrowserClientMessage, BrowserMessage, ServerMessage};
use serverbee_common::types::ServerStatus;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ws/servers", get(browser_ws_handler))
}

async fn browser_ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Validate auth: try session cookie first, then API key
    let user = validate_browser_auth(&state, &headers).await;
    match user {
        Some(_) => ws
            .max_message_size(MAX_WS_MESSAGE_SIZE)
            .on_upgrade(move |socket| handle_browser_ws(socket, state)),
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn validate_browser_auth(state: &Arc<AppState>, headers: &HeaderMap) -> Option<String> {
    // Try session cookie
    if let Some(token) = extract_session_cookie(headers)
        && let Ok(Some(user)) =
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

    // Try Bearer token (JWT)
    if let Some(token) = extract_bearer_token(headers)
        && let Ok(claims) = state.jwt.validate_access_token(&token)
    {
        return Some(claims.sub);
    }

    None
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|t| t.to_string())
}

fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie.strip_prefix("session_token=").map(|v| v.to_string())
        })
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")?
        .to_str()
        .ok()
        .map(|s| s.to_string())
}

async fn handle_browser_ws(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_sink, mut ws_stream) = socket.split();

    let connection_id = uuid::Uuid::new_v4().to_string();

    // Build FullSync message from DB servers + agent_manager online/report data
    let full_sync = build_full_sync(&state).await;
    if let Err(e) = send_browser_message(&mut ws_sink, &full_sync).await {
        tracing::error!("Failed to send FullSync to browser: {e}");
        return;
    }

    // Subscribe to browser_tx broadcast channel
    let mut browser_rx = state.browser_tx.subscribe();

    tracing::debug!("Browser WS client connected (connection_id={connection_id})");

    loop {
        tokio::select! {
            // Forward broadcast messages to WebSocket
            msg = browser_rx.recv() => {
                match msg {
                    Ok(browser_msg) => {
                        if let Err(e) = send_browser_message(&mut ws_sink, &browser_msg).await {
                            tracing::debug!("Failed to send to browser WS: {e}");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Browser WS lagged by {n} messages, sending full resync");
                        // On lag, send a full resync
                        let resync = build_full_sync(&state).await;
                        if let Err(e) = send_browser_message(&mut ws_sink, &resync).await {
                            tracing::debug!("Failed to send resync to browser WS: {e}");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            // Handle incoming messages from browser
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(client_msg) = serde_json::from_str::<BrowserClientMessage>(&text) {
                            handle_browser_client_message(&state, &connection_id, client_msg).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(Message::Ping(_))) => {
                        // axum auto-responds with Pong
                    }
                    Some(Ok(_)) => {
                        // Ignore other messages from browser
                    }
                    Some(Err(e)) => {
                        tracing::debug!("Browser WS error: {e}");
                        break;
                    }
                }
            }
        }
    }

    // Cleanup: remove all docker viewer subscriptions for this connection
    let affected = state
        .docker_viewers
        .remove_all_for_connection(&connection_id);
    for (server_id, was_last) in affected {
        if was_last {
            // Last viewer disconnected — tell agent to stop streaming docker data
            if let Some(tx) = state.agent_manager.get_sender(&server_id) {
                let _ = tx.send(ServerMessage::DockerStopStats).await;
                let _ = tx.send(ServerMessage::DockerEventsStop).await;
            }
        }
    }

    tracing::debug!("Browser WS client disconnected (connection_id={connection_id})");
}

async fn handle_browser_client_message(
    state: &Arc<AppState>,
    connection_id: &str,
    msg: BrowserClientMessage,
) {
    match msg {
        BrowserClientMessage::DockerSubscribe { server_id } => {
            // Check that Docker is available for this server
            if !state.agent_manager.has_docker_capability(&server_id)
                || !state.agent_manager.has_feature(&server_id, "docker")
            {
                return;
            }
            let is_first = state.docker_viewers.add_viewer(&server_id, connection_id);
            if is_first {
                // First viewer — tell agent to start streaming docker data
                if let Some(tx) = state.agent_manager.get_sender(&server_id) {
                    let _ = tx
                        .send(ServerMessage::DockerStartStats { interval_secs: 3 })
                        .await;
                    let _ = tx.send(ServerMessage::DockerEventsStart).await;
                }
            }
        }
        BrowserClientMessage::DockerUnsubscribe { server_id } => {
            let is_last = state
                .docker_viewers
                .remove_viewer(&server_id, connection_id);
            if is_last {
                // Last viewer — tell agent to stop streaming docker data
                if let Some(tx) = state.agent_manager.get_sender(&server_id) {
                    let _ = tx.send(ServerMessage::DockerStopStats).await;
                    let _ = tx.send(ServerMessage::DockerEventsStop).await;
                }
            }
        }
    }
}

async fn build_full_sync(state: &Arc<AppState>) -> BrowserMessage {
    let servers = match ServerService::list_servers(&state.db).await {
        Ok(servers) => servers,
        Err(e) => {
            tracing::error!("Failed to list servers for FullSync: {e}");
            return BrowserMessage::FullSync {
                servers: Vec::new(),
            };
        }
    };

    let statuses: Vec<ServerStatus> = servers
        .into_iter()
        .map(|server| {
            let online = state.agent_manager.is_online(&server.id);
            let report = state.agent_manager.get_latest_report(&server.id);

            let (cpu, mem_used, swap_used, disk_used, net_in_speed, net_out_speed) =
                if let Some(ref r) = report {
                    (
                        r.cpu,
                        r.mem_used,
                        r.swap_used,
                        r.disk_used,
                        r.net_in_speed,
                        r.net_out_speed,
                    )
                } else {
                    (0.0, 0, 0, 0, 0, 0)
                };

            let (
                net_in_transfer,
                net_out_transfer,
                load1,
                load5,
                load15,
                tcp_conn,
                udp_conn,
                process_count,
                uptime,
            ) = if let Some(ref r) = report {
                (
                    r.net_in_transfer,
                    r.net_out_transfer,
                    r.load1,
                    r.load5,
                    r.load15,
                    r.tcp_conn,
                    r.udp_conn,
                    r.process_count,
                    r.uptime,
                )
            } else {
                (0, 0, 0.0, 0.0, 0.0, 0, 0, 0, 0)
            };

            let last_active = if online {
                chrono::Utc::now().timestamp()
            } else {
                server.updated_at.timestamp()
            };

            ServerStatus {
                id: server.id.clone(),
                name: server.name.clone(),
                online,
                last_active,
                uptime,
                cpu,
                mem_used,
                mem_total: server.mem_total.unwrap_or(0),
                swap_used,
                swap_total: server.swap_total.unwrap_or(0),
                disk_used,
                disk_total: server.disk_total.unwrap_or(0),
                net_in_speed,
                net_out_speed,
                net_in_transfer,
                net_out_transfer,
                load1,
                load5,
                load15,
                tcp_conn,
                udp_conn,
                process_count,
                cpu_name: server.cpu_name,
                os: server.os,
                region: server.region,
                country_code: server.country_code,
                group_id: server.group_id,
                features: serde_json::from_str(&server.features).unwrap_or_default(),
            }
        })
        .collect();

    BrowserMessage::FullSync { servers: statuses }
}

async fn send_browser_message(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &BrowserMessage,
) -> Result<(), axum::Error> {
    let text = serde_json::to_string(msg).map_err(axum::Error::new)?;
    sink.send(Message::Text(text.into())).await
}
