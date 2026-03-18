use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::service::auth::AuthService;
use crate::service::network_probe::NetworkProbeService;
use crate::service::ping::PingService;
use crate::service::record::RecordService;
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::protocol::{AgentMessage, BrowserMessage, ServerMessage};
use serverbee_common::types::NetworkProbeTarget as NetworkProbeTargetDto;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    token: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/ws", get(agent_ws_handler))
}

async fn agent_ws_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    // Validate agent token
    let server = match AuthService::validate_agent_token(&state.db, &query.token).await {
        Ok(Some(server)) => server,
        Ok(None) => {
            return Response::builder()
                .status(401)
                .body("Unauthorized".into())
                .unwrap();
        }
        Err(e) => {
            tracing::error!("Failed to validate agent token: {e}");
            return Response::builder()
                .status(500)
                .body("Internal server error".into())
                .unwrap();
        }
    };

    let server_id = server.id.clone();
    let server_name = server.name.clone();
    let server_capabilities = server.capabilities;
    tracing::info!("Agent WS upgrading for server {server_id} ({server_name}) from {addr}");

    ws.on_upgrade(move |socket| handle_agent_ws(socket, state, server_id, server_name, server_capabilities, addr))
}

async fn handle_agent_ws(
    socket: WebSocket,
    state: Arc<AppState>,
    server_id: String,
    server_name: String,
    server_capabilities: i32,
    remote_addr: SocketAddr,
) {
    let (mut ws_sink, mut ws_stream) = socket.split();

    // Create mpsc channel for outgoing messages to this agent (buffer 64)
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(64);

    // Send Welcome message
    let welcome = ServerMessage::Welcome {
        server_id: server_id.clone(),
        protocol_version: serverbee_common::constants::PROTOCOL_VERSION,
        report_interval: 3,
        capabilities: Some(server_capabilities as u32),
    };
    if let Err(e) = send_server_message(&mut ws_sink, &welcome).await {
        tracing::error!("Failed to send Welcome to {server_id}: {e}");
        return;
    }

    // Register in AgentManager
    state
        .agent_manager
        .add_connection(server_id.clone(), server_name, tx, remote_addr);

    // Send current ping tasks to the newly connected agent
    PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, &server_id).await;

    // Send network probe sync to the newly connected agent
    match NetworkProbeService::get_server_targets(&state.db, &server_id).await {
        Ok(targets) => {
            match NetworkProbeService::get_setting(&state.db).await {
                Ok(setting) => {
                    let target_dtos: Vec<NetworkProbeTargetDto> = targets
                        .into_iter()
                        .map(|t| NetworkProbeTargetDto {
                            target_id: t.id,
                            name: t.name,
                            target: t.target,
                            probe_type: t.probe_type,
                        })
                        .collect();
                    if let Some(tx) = state.agent_manager.get_sender(&server_id) {
                        let _ = tx
                            .send(ServerMessage::NetworkProbeSync {
                                targets: target_dtos,
                                interval: setting.interval,
                                packet_count: setting.packet_count,
                            })
                            .await;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to get network probe setting for {server_id}: {e}");
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to get network probe targets for {server_id}: {e}");
        }
    }

    tracing::info!("Agent {server_id} connected from {remote_addr}");

    // Spawn a task to forward mpsc messages to WebSocket + send periodic Pings
    let sid_write = server_id.clone();
    let write_task = tokio::spawn(async move {
        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
        // Skip the first immediate tick
        ping_interval.tick().await;

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(server_msg) => {
                            if let Err(e) = send_server_message(&mut ws_sink, &server_msg).await {
                                tracing::warn!("Failed to send message to agent {sid_write}: {e}");
                                break;
                            }
                        }
                        None => {
                            // Channel closed, agent removed
                            break;
                        }
                    }
                }
                _ = ping_interval.tick() => {
                    if let Err(e) = ws_sink.send(Message::Ping(vec![].into())).await {
                        tracing::warn!("Failed to send ping to agent {sid_write}: {e}");
                        break;
                    }
                }
            }
        }

        // Try to close the WebSocket gracefully
        let _ = ws_sink.close().await;
    });

    // Read loop
    let sid_read = server_id.clone();
    let state_read = state.clone();
    while let Some(result) = ws_stream.next().await {
        match result {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<AgentMessage>(&text) {
                    Ok(agent_msg) => {
                        handle_agent_message(&state_read, &sid_read, agent_msg).await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Invalid message from agent {sid_read}: {e}, text: {text}"
                        );
                    }
                }
            }
            Ok(Message::Binary(data)) => {
                match serde_json::from_slice::<AgentMessage>(&data) {
                    Ok(agent_msg) => {
                        handle_agent_message(&state_read, &sid_read, agent_msg).await;
                    }
                    Err(e) => {
                        tracing::warn!("Invalid binary message from agent {sid_read}: {e}");
                    }
                }
            }
            Ok(Message::Pong(_)) => {
                // Agent responded to our Ping, update heartbeat timestamp
                state_read.agent_manager.touch_connection(&sid_read);
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Agent {sid_read} sent close frame");
                break;
            }
            Ok(Message::Ping(_)) => {
                // axum auto-responds with Pong
            }
            Err(e) => {
                tracing::warn!("WebSocket error for agent {sid_read}: {e}");
                break;
            }
        }
    }

    // Cleanup: remove from AgentManager and abort write task
    state.agent_manager.remove_connection(&server_id);
    write_task.abort();
    tracing::info!("Agent {server_id} disconnected");
}

async fn handle_agent_message(state: &Arc<AppState>, server_id: &str, msg: AgentMessage) {
    match msg {
        AgentMessage::SystemInfo { msg_id, info } => {
            // Resolve GeoIP from agent's remote address
            let geo = state.geoip.as_ref().and_then(|g| {
                let conn = state.agent_manager.get_remote_addr(server_id);
                conn.map(|addr| g.lookup(addr.ip()))
            });

            let (region, country_code) = match geo {
                Some(ref g) => (g.region.clone(), g.country_code.clone()),
                None => (None, None),
            };

            if let Err(e) = ServerService::update_system_info(&state.db, server_id, &info, region, country_code).await {
                tracing::error!("Failed to update system info for {server_id}: {e}");
            }

            // Update in-memory protocol_version
            let agent_pv = info.protocol_version;
            state.agent_manager.set_protocol_version(server_id, agent_pv);

            // Broadcast to browsers
            state.agent_manager.broadcast_browser(BrowserMessage::AgentInfoUpdated {
                server_id: server_id.to_string(),
                protocol_version: agent_pv,
            });

            // Send Ack
            if let Some(tx) = state.agent_manager.get_sender(server_id) {
                let _ = tx.send(ServerMessage::Ack { msg_id }).await;
            }
        }
        AgentMessage::Report(report) => {
            // Save GPU records if present
            if let Some(ref gpu) = report.gpu
                && let Err(e) = RecordService::save_gpu_records(&state.db, server_id, gpu).await
            {
                tracing::error!("Failed to save GPU records for {server_id}: {e}");
            }
            state.agent_manager.update_report(server_id, report);
        }
        AgentMessage::TaskResult { msg_id, result } => {
            // Store task result in DB
            if let Err(e) = save_task_result(&state.db, server_id, &result).await {
                tracing::error!("Failed to save task result for {server_id}: {e}");
            }
            // Send Ack
            if let Some(tx) = state.agent_manager.get_sender(server_id) {
                let _ = tx.send(ServerMessage::Ack { msg_id }).await;
            }
        }
        AgentMessage::PingResult(result) => {
            if let Err(e) = save_ping_result(&state.db, server_id, &result).await {
                tracing::error!("Failed to save ping result for {server_id}: {e}");
            }
        }
        AgentMessage::TerminalOutput { session_id, data } => {
            if let Some(tx) = state.agent_manager.get_terminal_session(&session_id) {
                let _ = tx.send(crate::service::agent_manager::TerminalSessionEvent::Output(data)).await;
            }
        }
        AgentMessage::TerminalStarted { session_id } => {
            if let Some(tx) = state.agent_manager.get_terminal_session(&session_id) {
                let _ = tx.send(crate::service::agent_manager::TerminalSessionEvent::Started).await;
            }
        }
        AgentMessage::TerminalError { session_id, error } => {
            if let Some(tx) = state.agent_manager.get_terminal_session(&session_id) {
                let _ = tx.send(crate::service::agent_manager::TerminalSessionEvent::Error(error)).await;
            }
        }
        AgentMessage::CapabilityDenied { msg_id, session_id, capability } => {
            tracing::warn!(
                "Agent {server_id} denied capability '{capability}' (msg_id={msg_id:?}, session_id={session_id:?})"
            );
            // For exec: write synthetic task_result so frontend polling resolves
            if let Some(task_id) = &msg_id {
                use crate::entity::task_result;
                use sea_orm::{ActiveModelTrait, NotSet, Set};
                let result = task_result::ActiveModel {
                    id: NotSet,
                    task_id: Set(task_id.clone()),
                    server_id: Set(server_id.to_string()),
                    output: Set("Capability denied by agent".to_string()),
                    exit_code: Set(-1),
                    run_id: NotSet,
                    attempt: Set(1),
                    started_at: NotSet,
                    finished_at: Set(chrono::Utc::now()),
                };
                if let Err(e) = result.insert(&state.db).await {
                    tracing::error!("Failed to write CapabilityDenied task result: {e}");
                }
            }
            // For terminal: unregister session so browser gets notified
            if let Some(sid) = &session_id {
                state.agent_manager.unregister_terminal_session(sid);
            }
        }
        AgentMessage::NetworkProbeResults { results } => {
            // Broadcast to browsers before saving (clone needed for save)
            let _ = state.browser_tx.send(BrowserMessage::NetworkProbeUpdate {
                server_id: server_id.to_string(),
                results: results.clone(),
            });
            if let Err(e) =
                NetworkProbeService::save_results(&state.db, server_id, results).await
            {
                tracing::error!("Failed to save network probe results for {server_id}: {e}");
            }
        }
        // File management control responses — relay to pending HTTP requests
        AgentMessage::FileListResult { ref msg_id, .. } => {
            if !state.agent_manager.dispatch_pending_response(msg_id, msg.clone()) {
                tracing::debug!("Orphaned FileListResult for msg_id={msg_id}");
            }
        }
        AgentMessage::FileStatResult { ref msg_id, .. } => {
            if !state.agent_manager.dispatch_pending_response(msg_id, msg.clone()) {
                tracing::debug!("Orphaned FileStatResult for msg_id={msg_id}");
            }
        }
        AgentMessage::FileReadResult { ref msg_id, .. } => {
            if !state.agent_manager.dispatch_pending_response(msg_id, msg.clone()) {
                tracing::debug!("Orphaned FileReadResult for msg_id={msg_id}");
            }
        }
        AgentMessage::FileOpResult { ref msg_id, .. } => {
            if !state.agent_manager.dispatch_pending_response(msg_id, msg.clone()) {
                tracing::debug!("Orphaned FileOpResult for msg_id={msg_id}");
            }
        }

        // File download transfer messages
        AgentMessage::FileDownloadReady { ref transfer_id, size } => {
            state.file_transfers.update_size(transfer_id, size);
            state.file_transfers.mark_in_progress(transfer_id);
            // Create the temp file and keep it open for the duration of the transfer
            if let Some(path) = state.file_transfers.temp_file_path(transfer_id) {
                match tokio::fs::File::create(&path).await {
                    Ok(file) => {
                        state.file_transfers.store_file_handle(transfer_id, file);
                    }
                    Err(e) => {
                        tracing::error!("Failed to create temp file for transfer {transfer_id}: {e}");
                        state.file_transfers.mark_failed(transfer_id, format!("Failed to create temp file: {e}"));
                    }
                }
            }
        }
        AgentMessage::FileDownloadChunk { ref transfer_id, offset, ref data } => {
            use base64::Engine;
            use tokio::io::{AsyncSeekExt, AsyncWriteExt};
            if let Some(file_handle) = state.file_transfers.get_file_handle(transfer_id) {
                match base64::engine::general_purpose::STANDARD.decode(data) {
                    Ok(bytes) => {
                        let result = async {
                            let mut file = file_handle.lock().await;
                            file.seek(std::io::SeekFrom::Start(offset)).await?;
                            file.write_all(&bytes).await?;
                            Ok::<(), std::io::Error>(())
                        }
                        .await;
                        match result {
                            Ok(()) => {
                                state.file_transfers.update_progress(transfer_id, offset + bytes.len() as u64);
                            }
                            Err(e) => {
                                tracing::error!("Failed to write chunk for transfer {transfer_id}: {e}");
                                state.file_transfers.mark_failed(transfer_id, format!("Write error: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to decode base64 chunk for transfer {transfer_id}: {e}");
                        state.file_transfers.mark_failed(transfer_id, format!("Base64 decode error: {e}"));
                    }
                }
            }
        }
        AgentMessage::FileDownloadEnd { ref transfer_id } => {
            state.file_transfers.remove_file_handle(transfer_id);
            state.file_transfers.mark_ready(transfer_id);
        }
        AgentMessage::FileDownloadError { ref transfer_id, ref error } => {
            state.file_transfers.remove_file_handle(transfer_id);
            state.file_transfers.mark_failed(transfer_id, error.clone());
        }

        // File upload transfer messages
        AgentMessage::FileUploadAck { ref transfer_id, offset } => {
            state.file_transfers.update_progress(transfer_id, offset);
            let ack_key = format!("upload-ack-{transfer_id}");
            state.agent_manager.dispatch_pending_response(&ack_key, msg.clone());
        }
        AgentMessage::FileUploadComplete { ref transfer_id } => {
            state.file_transfers.mark_ready(transfer_id);
            let complete_key = format!("upload-complete-{transfer_id}");
            state.agent_manager.dispatch_pending_response(&complete_key, msg.clone());
        }
        AgentMessage::FileUploadError { ref transfer_id, ref error } => {
            state.file_transfers.mark_failed(transfer_id, error.clone());
            // The HTTP handler may be waiting on either an ack or complete key — try both.
            let ack_key = format!("upload-ack-{transfer_id}");
            let complete_key = format!("upload-complete-{transfer_id}");
            if !state.agent_manager.dispatch_pending_response(&complete_key, msg.clone()) {
                state.agent_manager.dispatch_pending_response(&ack_key, msg.clone());
            }
        }

        AgentMessage::Pong => {
            // Agent responded to our protocol-level Ping; already handled by WS Pong frames
        }

        // Docker variants — handled in Task 10
        AgentMessage::DockerInfo { .. }
        | AgentMessage::DockerContainers { .. }
        | AgentMessage::DockerStats { .. }
        | AgentMessage::DockerLog { .. }
        | AgentMessage::DockerEvent { .. }
        | AgentMessage::FeaturesUpdate { .. }
        | AgentMessage::DockerUnavailable
        | AgentMessage::DockerNetworks { .. }
        | AgentMessage::DockerVolumes { .. }
        | AgentMessage::DockerActionResult { .. } => {
            tracing::debug!("Received Docker message from {server_id}, handler not yet wired");
        }
    }
}

async fn send_server_message(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
) -> Result<(), axum::Error> {
    let text = serde_json::to_string(msg).map_err(axum::Error::new)?;
    sink.send(Message::Text(text.into())).await
}

/// Save a task result to the database.
async fn save_task_result(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    result: &serverbee_common::types::TaskResult,
) -> Result<(), crate::error::AppError> {
    use crate::entity::task_result;
    use sea_orm::{ActiveModelTrait, NotSet, Set};

    let new_result = task_result::ActiveModel {
        id: NotSet,
        task_id: Set(result.task_id.clone()),
        server_id: Set(server_id.to_string()),
        output: Set(result.output.clone()),
        exit_code: Set(result.exit_code),
        run_id: NotSet,
        attempt: Set(1),
        started_at: NotSet,
        finished_at: Set(chrono::Utc::now()),
    };
    new_result.insert(db).await?;
    Ok(())
}

/// Save a ping result to the database.
async fn save_ping_result(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    result: &serverbee_common::types::PingResult,
) -> Result<(), crate::error::AppError> {
    use crate::entity::ping_record;
    use sea_orm::{ActiveModelTrait, NotSet, Set};

    let new_record = ping_record::ActiveModel {
        id: NotSet,
        task_id: Set(result.task_id.clone()),
        server_id: Set(server_id.to_string()),
        latency: Set(result.latency),
        success: Set(result.success),
        error: Set(result.error.clone()),
        time: Set(result.time),
    };
    new_record.insert(db).await?;
    Ok(())
}
