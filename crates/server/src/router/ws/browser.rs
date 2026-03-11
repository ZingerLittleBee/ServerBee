use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};

use crate::service::auth::AuthService;
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::protocol::BrowserMessage;
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
        Some(_) => ws.on_upgrade(move |socket| handle_browser_ws(socket, state)),
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn validate_browser_auth(state: &Arc<AppState>, headers: &HeaderMap) -> Option<String> {
    // Try session cookie
    if let Some(token) = extract_session_cookie(headers) {
        if let Ok(Some(user)) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
        {
            return Some(user.id);
        }
    }

    // Try API key header
    if let Some(key) = extract_api_key(headers) {
        if let Ok(Some(user)) = AuthService::validate_api_key(&state.db, &key).await {
            return Some(user.id);
        }
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

async fn handle_browser_ws(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_sink, mut ws_stream) = socket.split();

    // Build FullSync message from DB servers + agent_manager online/report data
    let full_sync = build_full_sync(&state).await;
    if let Err(e) = send_browser_message(&mut ws_sink, &full_sync).await {
        tracing::error!("Failed to send FullSync to browser: {e}");
        return;
    }

    // Subscribe to browser_tx broadcast channel
    let mut browser_rx = state.browser_tx.subscribe();

    tracing::debug!("Browser WS client connected");

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
            // Handle incoming messages from browser (mostly just Close)
            msg = ws_stream.next() => {
                match msg {
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

    tracing::debug!("Browser WS client disconnected");
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
