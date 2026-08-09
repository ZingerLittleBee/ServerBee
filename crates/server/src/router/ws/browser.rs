use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use sea_orm::{EntityTrait, QueryOrder};

use crate::entity::server_tag;
use crate::middleware::auth::{AuthenticatedConnection, resolve_ws_connection};
use crate::service::agent_manager::aggregate_disk_io;
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::constants::MAX_WS_MESSAGE_SIZE;
use serverbee_common::protocol::{BrowserClientMessage, BrowserMessage, ServerMessage};
use serverbee_common::types::{
    AgentAuthorityStateSummary, AgentAuthorityStatus, OutstandingEnrollmentSummary, ServerStatus,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ws/servers", get(browser_ws_handler))
}

async fn browser_ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Shared credential policy (precedence, session source, mobile expiry)
    // lives in `middleware::auth`; this adapter only maps the verdict onto
    // the browser WS protocol.
    match resolve_ws_connection(&headers, &state).await {
        Some(conn) => ws
            .max_message_size(MAX_WS_MESSAGE_SIZE)
            .on_upgrade(move |socket| handle_browser_ws(socket, state, conn)),
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn handle_browser_ws(socket: WebSocket, state: Arc<AppState>, auth: AuthenticatedConnection) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    let is_admin = auth.user.role == "admin";
    let mobile_expires = auth.mobile_expires;
    let auth_lease = super::session::auth_lease_invalidated(&state, &auth);
    tokio::pin!(auth_lease);

    let connection_id = uuid::Uuid::new_v4().to_string();

    // Build FullSync message from DB servers + agent_manager online/report data
    let full_sync = build_full_sync(&state, is_admin).await;
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
                        let filtered = filter_browser_message(browser_msg, is_admin);
                        if let Some(filtered) = filtered
                            && let Err(e) = send_browser_message(&mut ws_sink, &filtered).await
                        {
                            tracing::debug!("Failed to send to browser WS: {e}");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Browser WS lagged by {n} messages, sending full resync");
                        // On lag, send a full resync
                        let resync = build_full_sync(&state, is_admin).await;
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
            () = super::session::mobile_token_expired(mobile_expires) => {
                tracing::debug!("Mobile WS token expired, closing connection");
                let _ = ws_sink.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 4001,
                    reason: "token expired".into(),
                }))).await;
                break;
            }
            () = &mut auth_lease => {
                tracing::debug!(user_id = %auth.user.user_id, "Browser WS authorization revoked");
                let _ = ws_sink.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 4003,
                    reason: "authorization revoked".into(),
                }))).await;
                break;
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

async fn build_full_sync(state: &Arc<AppState>, _is_admin: bool) -> BrowserMessage {
    let servers = match ServerService::list_servers(&state.db).await {
        Ok(servers) => servers,
        Err(e) => {
            tracing::error!("Failed to list servers for FullSync: {e}");
            return BrowserMessage::FullSync {
                servers: Vec::new(),
                upgrades: state.upgrade_tracker.snapshot(),
            };
        }
    };

    let tags_rows = server_tag::Entity::find()
        .order_by_asc(server_tag::Column::ServerId)
        .order_by_asc(server_tag::Column::Tag)
        .all(&state.db)
        .await
        .unwrap_or_default();
    let mut tags_by_server: HashMap<String, Vec<String>> = HashMap::new();
    for row in tags_rows {
        tags_by_server
            .entry(row.server_id)
            .or_default()
            .push(row.tag);
    }

    let server_ids: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();
    let authority_ids = match server_ids
        .into_iter()
        .map(crate::service::agent_authority::ServerId::parse)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!("Failed to parse stored Server ID for FullSync: {error}");
            return BrowserMessage::FullSync {
                servers: Vec::new(),
                upgrades: state.upgrade_tracker.snapshot(),
            };
        }
    };
    let mut authority_by_server: HashMap<String, AgentAuthorityStateSummary> =
        match state.agent_authority.states(&authority_ids).await {
            Ok(states) => states
                .into_iter()
                .map(|authority| {
                    let server_id = authority.server_id.as_str().to_string();
                    let outstanding_offer =
                        authority
                            .outstanding_offer
                            .map(|offer| OutstandingEnrollmentSummary {
                                id: offer.id.into_inner(),
                                code_prefix: offer.code_prefix,
                                expires_at: offer.expires_at.to_rfc3339(),
                                created_at: offer.created_at.to_rfc3339(),
                            });
                    let status = match authority.authority {
                        crate::service::agent_authority::AuthorityStatus::Claimed => {
                            AgentAuthorityStatus::Claimed
                        }
                        crate::service::agent_authority::AuthorityStatus::Unclaimed => {
                            AgentAuthorityStatus::Unclaimed
                        }
                    };
                    (
                        server_id,
                        AgentAuthorityStateSummary {
                            status,
                            outstanding_offer,
                        },
                    )
                })
                .collect(),
            Err(error) => {
                tracing::error!("Failed to project Agent Authority for FullSync: {error}");
                return BrowserMessage::FullSync {
                    servers: Vec::new(),
                    upgrades: state.upgrade_tracker.snapshot(),
                };
            }
        };
    if authority_by_server.len() != servers.len() {
        tracing::warn!(
            "Server set changed while building FullSync; retrying on the next connection"
        );
        return BrowserMessage::FullSync {
            servers: Vec::new(),
            upgrades: state.upgrade_tracker.snapshot(),
        };
    }

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

            let (disk_read_bytes_per_sec, disk_write_bytes_per_sec) = report
                .as_ref()
                .map(|r| aggregate_disk_io(r))
                .unwrap_or((0, 0));

            let agent_authority = authority_by_server.remove(&server.id).unwrap_or_default();
            let outstanding_enrollment = agent_authority.outstanding_offer.clone();
            let has_token = agent_authority.status == AgentAuthorityStatus::Claimed;

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
                disk_read_bytes_per_sec,
                disk_write_bytes_per_sec,
                tags: tags_by_server.remove(&server.id).unwrap_or_default(),
                cpu_cores: server.cpu_cores,
                has_token,
                agent_authority,
                outstanding_enrollment,
            }
        })
        .collect();

    BrowserMessage::FullSync {
        servers: statuses,
        upgrades: state.upgrade_tracker.snapshot(),
    }
}

fn filter_browser_message(msg: BrowserMessage, _is_admin: bool) -> Option<BrowserMessage> {
    Some(msg)
}

async fn send_browser_message(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &BrowserMessage,
) -> Result<(), axum::Error> {
    let text = serde_json::to_string(msg).map_err(axum::Error::new)?;
    sink.send(Message::Text(text.into())).await
}
