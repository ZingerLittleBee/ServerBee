use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::service::alert::AlertService;
use crate::service::audit::AuditService;
use crate::service::auth::AuthService;
use crate::service::network_probe::NetworkProbeService;
use crate::service::ping::PingService;
use crate::service::record::RecordService;
use crate::service::recovery_job::RecoveryJobService;
use crate::service::recovery_merge::{RECOVERY_STAGE_REBINDING, RecoveryMergeService};
use crate::service::server::ServerService;
use crate::service::upgrade_tracker::UpgradeLookup;
use crate::state::AppState;
use serverbee_common::constants::{MAX_WS_MESSAGE_SIZE, effective_capabilities};
use serverbee_common::protocol::{AgentMessage, BrowserMessage, ServerMessage};
use serverbee_common::types::NetworkProbeTarget as NetworkProbeTargetDto;

#[derive(Debug, Deserialize)]
pub struct OptionalWsQuery {
    token: Option<String>,
}

fn extract_agent_token(headers: &HeaderMap, query: &OptionalWsQuery) -> Option<String> {
    // Prefer query param (reliable through reverse proxies / cloud load balancers)
    if let Some(ref token) = query.token {
        return Some(token.clone());
    }
    // Fallback to Authorization header (direct connections)
    if let Some(auth) = headers.get("authorization")
        && let Ok(val) = auth.to_str()
        && let Some(token) = val.strip_prefix("Bearer ")
    {
        return Some(token.to_string());
    }
    None
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/ws", get(agent_ws_handler))
}

async fn agent_ws_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<OptionalWsQuery>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    let query_present = query.token.as_ref().is_some_and(|token| !token.is_empty());
    let auth_present = headers.get("authorization").is_some();

    // Extract agent token from Authorization header or query param
    let token = match extract_agent_token(&headers, &query) {
        Some(t) => t,
        None => {
            tracing::warn!(
                "Agent WS unauthorized from {addr}: missing token (query_present={query_present}, authorization_present={auth_present})"
            );
            return Response::builder()
                .status(401)
                .body("Unauthorized".into())
                .unwrap();
        }
    };

    // Validate agent token
    let server = match AuthService::validate_agent_token(&state.db, &token).await {
        Ok(Some(server)) => server,
        Ok(None) => {
            tracing::warn!(
                "Agent WS unauthorized from {addr}: invalid token (source={}, prefix={})",
                if query.token.as_deref() == Some(token.as_str()) {
                    "query"
                } else {
                    "authorization"
                },
                &token[..8.min(token.len())]
            );
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

    ws.max_message_size(MAX_WS_MESSAGE_SIZE)
        .on_upgrade(move |socket| {
            handle_agent_ws(
                socket,
                state,
                server_id,
                server_name,
                server_capabilities,
                addr,
            )
        })
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
    let connection_id = {
        let server_lock = state.agent_manager.server_cleanup_lock(&server_id);
        let _guard = server_lock.lock().await;
        let connection_id =
            state
                .agent_manager
                .add_connection(server_id.clone(), server_name, tx, remote_addr);
        state
            .agent_manager
            .update_capabilities(&server_id, server_capabilities as u32);
        connection_id
    };

    // Send current ping tasks to the newly connected agent
    PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, &server_id).await;

    // Send network probe sync to the newly connected agent
    match NetworkProbeService::get_server_targets(&state.db, &server_id).await {
        Ok(targets) => match NetworkProbeService::get_setting(&state.db).await {
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
        },
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
            Ok(Message::Text(text)) => match serde_json::from_str::<AgentMessage>(&text) {
                Ok(agent_msg) => {
                    if !handle_current_connection_frame(
                        &state_read,
                        &sid_read,
                        connection_id,
                        CurrentConnectionFrame::AgentMessage(Box::new(agent_msg)),
                    )
                    .await
                    {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("Invalid message from agent {sid_read}: {e}, text: {text}");
                }
            },
            Ok(Message::Binary(data)) => match serde_json::from_slice::<AgentMessage>(&data) {
                Ok(agent_msg) => {
                    if !handle_current_connection_frame(
                        &state_read,
                        &sid_read,
                        connection_id,
                        CurrentConnectionFrame::AgentMessage(Box::new(agent_msg)),
                    )
                    .await
                    {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("Invalid binary message from agent {sid_read}: {e}");
                }
            },
            Ok(Message::Pong(_)) => {
                if !handle_current_connection_frame(
                    &state_read,
                    &sid_read,
                    connection_id,
                    CurrentConnectionFrame::Pong,
                )
                .await
                {
                    break;
                }
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
    let server_lock = state.agent_manager.server_cleanup_lock(&server_id);
    let _guard = server_lock.lock().await;
    if state
        .agent_manager
        .remove_connection_if_current(&server_id, connection_id)
    {
        crate::service::agent_manager::cleanup_disconnected_docker_state(&state, &server_id).await;
    }
    write_task.abort();
    tracing::info!("Agent {server_id} disconnected");
}

enum CurrentConnectionFrame {
    AgentMessage(Box<AgentMessage>),
    Pong,
}

async fn handle_current_connection_frame(
    state: &Arc<AppState>,
    server_id: &str,
    connection_id: u64,
    frame: CurrentConnectionFrame,
) -> bool {
    {
        let server_lock = state.agent_manager.server_cleanup_lock(server_id);
        let _guard = server_lock.lock().await;

        if !state
            .agent_manager
            .is_current_connection(server_id, connection_id)
        {
            tracing::info!(
                "Stopping superseded agent socket for {server_id} (connection_id={connection_id})"
            );
            return false;
        }
    }

    match frame {
        CurrentConnectionFrame::AgentMessage(agent_msg) => {
            handle_agent_message(state, server_id, *agent_msg).await;
        }
        CurrentConnectionFrame::Pong => {
            state.agent_manager.touch_connection(server_id);
        }
    }

    true
}

async fn handle_agent_message(state: &Arc<AppState>, server_id: &str, msg: AgentMessage) {
    match msg {
        AgentMessage::SystemInfo {
            msg_id,
            info,
            agent_local_capabilities,
        } => {
            // Resolve GeoIP — prefer agent-reported public IP, fall back to remote_addr
            let ip = info
                .ipv4
                .as_deref()
                .or(info.ipv6.as_deref())
                .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
                .or_else(|| {
                    state
                        .agent_manager
                        .get_remote_addr(server_id)
                        .map(|addr| addr.ip())
                });

            let (region, country_code) = match ip {
                Some(ip) => {
                    let guard = state.geoip.read().unwrap();
                    match guard.as_ref() {
                        Some(g) => {
                            let geo = g.lookup(ip);
                            (geo.region, geo.country_code)
                        }
                        None => (None, None),
                    }
                }
                None => (None, None),
            };

            // --- Passive IP change detection (remote_addr) ---
            let current_remote_addr = state
                .agent_manager
                .get_remote_addr(server_id)
                .map(|a| a.ip().to_string());

            if let Ok(srv) = ServerService::get_server(&state.db, server_id).await {
                let old_remote_addr = srv.last_remote_addr.clone();
                let old_ipv4 = srv.ipv4.clone();
                let old_ipv6 = srv.ipv6.clone();

                // Check if remote_addr changed
                if let Some(ref new_addr) = current_remote_addr
                    && let Some(ref old_addr) = old_remote_addr
                    && old_addr != new_addr
                {
                    tracing::info!(
                        "Server {server_id} remote address changed: {old_addr} -> {new_addr}"
                    );
                    if state.recovery_lock.writes_allowed_for(server_id) {
                        if let Err(e) = AuditService::log(
                            &state.db,
                            "system",
                            "ip_changed",
                            Some(&format!(
                                "Remote address changed from {old_addr} to {new_addr} for server {server_id}"
                            )),
                            new_addr,
                        )
                        .await
                        {
                            tracing::error!("Failed to write audit log for IP change: {e}");
                        }
                    } else {
                        tracing::info!(
                            "Skipping recovery-frozen IP-change audit write for {server_id}"
                        );
                    }
                }

                // Check if agent-reported IPs changed
                let ipv4_changed = old_ipv4 != info.ipv4;
                let ipv6_changed = old_ipv6 != info.ipv6;
                let remote_changed = old_remote_addr.as_ref() != current_remote_addr.as_ref();

                if ipv4_changed || ipv6_changed || remote_changed {
                    if state.recovery_lock.writes_allowed_for(server_id) {
                        if let Err(e) = AlertService::check_event_rules(
                            &state.db,
                            &state.alert_state_manager,
                            server_id,
                            "ip_changed",
                        )
                        .await
                        {
                            tracing::error!("Failed to check event rules for IP change: {e}");
                        }
                    } else {
                        tracing::info!("Skipping recovery-frozen alert evaluation for {server_id}");
                    }

                    state
                        .agent_manager
                        .broadcast_browser(BrowserMessage::ServerIpChanged {
                            server_id: server_id.to_string(),
                            old_ipv4,
                            new_ipv4: info.ipv4.clone(),
                            old_ipv6,
                            new_ipv6: info.ipv6.clone(),
                            old_remote_addr,
                            new_remote_addr: current_remote_addr.clone(),
                        });
                }

                // Always update last_remote_addr
                if let Some(ref addr) = current_remote_addr {
                    if state.recovery_lock.writes_allowed_for(server_id) {
                        if let Err(e) = update_last_remote_addr(&state.db, server_id, addr).await {
                            tracing::error!(
                                "Failed to update last_remote_addr for {server_id}: {e}"
                            );
                        }
                    } else {
                        tracing::info!(
                            "Skipping recovery-frozen system-info write for {server_id}"
                        );
                    }
                }
            }

            if state.recovery_lock.writes_allowed_for(server_id) {
                if let Err(e) = ServerService::update_system_info(
                    &state.db,
                    server_id,
                    &info,
                    region,
                    country_code,
                )
                .await
                {
                    tracing::error!("Failed to update system info for {server_id}: {e}");
                }
            } else {
                tracing::info!("Skipping recovery-frozen system-info write for {server_id}");
            }

            if state.recovery_lock.writes_allowed_for(server_id) {
                let _ = crate::service::server::ServerService::update_features(
                    &state.db,
                    server_id,
                    &info.features,
                )
                .await;
            } else {
                tracing::info!("Skipping recovery-frozen system-info write for {server_id}");
            }
            state
                .agent_manager
                .update_features(server_id, info.features.clone());

            // Update in-memory protocol_version
            let agent_pv = info.protocol_version;
            state
                .agent_manager
                .set_protocol_version(server_id, agent_pv);

            // Store os/arch for upgrade platform mapping
            state.agent_manager.update_agent_platform(
                server_id,
                info.os.clone(),
                info.cpu_arch.clone(),
            );

            if let Some(bits) = agent_local_capabilities {
                state
                    .agent_manager
                    .update_agent_local_capabilities(server_id, bits);

                if let Some(configured) = state.agent_manager.get_server_capabilities(server_id) {
                    state
                        .agent_manager
                        .broadcast_browser(BrowserMessage::CapabilitiesChanged {
                            server_id: server_id.to_string(),
                            capabilities: configured,
                            agent_local_capabilities: Some(bits),
                            effective_capabilities: Some(effective_capabilities(configured, bits)),
                        });
                }
            }

            // Broadcast to browsers
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::AgentInfoUpdated {
                    server_id: server_id.to_string(),
                    protocol_version: agent_pv,
                    agent_version: Some(info.agent_version.clone()),
                });

            if let Some(job) = state.upgrade_tracker.get(server_id)
                && job.status == serverbee_common::protocol::UpgradeStatus::Running
                && job.target_version == info.agent_version
            {
                state
                    .upgrade_tracker
                    .mark_succeeded(UpgradeLookup::from_job(&job), None);
            }

            // Send Ack
            if let Some(tx) = state.agent_manager.get_sender(server_id) {
                let _ = tx.send(ServerMessage::Ack { msg_id }).await;

                if state.docker_viewers.has_viewers(server_id)
                    && info.features.iter().any(|feature| feature == "docker")
                {
                    let _ = tx
                        .send(ServerMessage::DockerStartStats { interval_secs: 3 })
                        .await;
                    let _ = tx.send(ServerMessage::DockerEventsStart).await;
                }
            }
        }
        AgentMessage::Report(report) => {
            // Save GPU records if present
            if let Some(ref gpu) = report.gpu {
                if state.recovery_lock.writes_allowed_for(server_id) {
                    if let Err(e) = RecordService::save_gpu_records(&state.db, server_id, gpu).await
                    {
                        tracing::error!("Failed to save GPU records for {server_id}: {e}");
                    }
                } else {
                    tracing::info!("Skipping recovery-frozen report write for {server_id}");
                }
            }
            state.agent_manager.update_report(server_id, report);
        }
        AgentMessage::TaskResult { msg_id, result } => {
            // Try pending dispatch first (scheduler or other waiters)
            let dispatched = state.agent_manager.dispatch_pending_response(
                &result.task_id,
                AgentMessage::TaskResult {
                    msg_id: msg_id.clone(),
                    result: result.clone(),
                },
            );
            if !dispatched {
                // No waiter — one-shot task, save directly
                if state.recovery_lock.writes_allowed_for(server_id) {
                    if let Err(e) = save_task_result(&state.db, server_id, &result).await {
                        tracing::error!("Failed to save task result for {server_id}: {e}");
                    }
                } else {
                    tracing::info!("Skipping recovery-frozen task-result write for {server_id}");
                }
            }
            if state.recovery_lock.writes_allowed_for(server_id) {
                if let Err(e) = audit_exec_finished(state, server_id, &result).await {
                    tracing::error!("Failed to write exec_finished audit log for {server_id}: {e}");
                }
            } else {
                tracing::info!("Skipping recovery-frozen exec audit write for {server_id}");
            }
            // Send Ack
            if let Some(tx) = state.agent_manager.get_sender(server_id) {
                let _ = tx.send(ServerMessage::Ack { msg_id }).await;
            }
        }
        AgentMessage::UpgradeProgress {
            msg_id,
            job_id,
            target_version,
            stage,
        } => {
            state
                .upgrade_tracker
                .update_stage(UpgradeLookup::new(server_id, job_id, target_version), stage);

            if let Some(tx) = state.agent_manager.get_sender(server_id) {
                let _ = tx.send(ServerMessage::Ack { msg_id }).await;
            }
        }
        AgentMessage::UpgradeResult {
            msg_id,
            job_id,
            target_version,
            stage,
            error,
            backup_path,
        } => {
            state.upgrade_tracker.mark_failed(
                UpgradeLookup::new(server_id, job_id, target_version),
                stage,
                error,
                backup_path,
            );

            if let Some(tx) = state.agent_manager.get_sender(server_id) {
                let _ = tx.send(ServerMessage::Ack { msg_id }).await;
            }
        }
        AgentMessage::PingResult(result) => {
            if state.recovery_lock.writes_allowed_for(server_id) {
                if let Err(e) = save_ping_result(&state.db, server_id, &result).await {
                    tracing::error!("Failed to save ping result for {server_id}: {e}");
                }
            } else {
                tracing::info!("Skipping recovery-frozen ping write for {server_id}");
            }
        }
        AgentMessage::TerminalOutput { session_id, data } => {
            if let Some(tx) = state.agent_manager.get_terminal_session(&session_id) {
                let _ = tx
                    .send(crate::service::agent_manager::TerminalSessionEvent::Output(
                        data,
                    ))
                    .await;
            }
        }
        AgentMessage::TerminalStarted { session_id } => {
            if let Some(tx) = state.agent_manager.get_terminal_session(&session_id) {
                let _ = tx
                    .send(crate::service::agent_manager::TerminalSessionEvent::Started)
                    .await;
            }
        }
        AgentMessage::TerminalError { session_id, error } => {
            if let Some(tx) = state.agent_manager.get_terminal_session(&session_id) {
                let _ = tx
                    .send(crate::service::agent_manager::TerminalSessionEvent::Error(
                        error,
                    ))
                    .await;
            }
        }
        AgentMessage::CapabilityDenied {
            msg_id,
            session_id,
            capability,
            reason,
        } => {
            tracing::warn!(
                "Agent {server_id} denied capability '{capability}' with reason {reason:?} (msg_id={msg_id:?}, session_id={session_id:?})"
            );
            // For exec: try pending dispatch first, then save directly
            if let Some(task_id) = &msg_id {
                let synthetic = serverbee_common::types::TaskResult {
                    task_id: task_id.clone(),
                    output: capability_denied_output(&capability, reason),
                    exit_code: -2,
                };
                let dispatched = state.agent_manager.dispatch_pending_response(
                    task_id,
                    AgentMessage::TaskResult {
                        msg_id: task_id.clone(),
                        result: synthetic,
                    },
                );
                if !dispatched {
                    if state.recovery_lock.writes_allowed_for(server_id) {
                        use crate::entity::task_result;
                        use sea_orm::{ActiveModelTrait, NotSet, Set};
                        let result = task_result::ActiveModel {
                            id: NotSet,
                            task_id: Set(task_id.clone()),
                            server_id: Set(server_id.to_string()),
                            output: Set(capability_denied_output(&capability, reason)),
                            exit_code: Set(-2),
                            run_id: Set(None),
                            attempt: Set(1),
                            started_at: Set(None),
                            finished_at: Set(chrono::Utc::now()),
                        };
                        if let Err(e) = result.insert(&state.db).await {
                            tracing::error!("Failed to write CapabilityDenied task result: {e}");
                        }
                    } else {
                        tracing::info!(
                            "Skipping recovery-frozen capability-denied write for {server_id}"
                        );
                    }
                }
            }
            if capability == "upgrade"
                && let Some(job) = state.upgrade_tracker.get(server_id)
            {
                state
                    .upgrade_tracker
                    .mark_failed_by_capability_denied(UpgradeLookup::from_job(&job), reason);
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
            if state.recovery_lock.writes_allowed_for(server_id) {
                if let Err(e) =
                    NetworkProbeService::save_results(&state.db, server_id, results).await
                {
                    tracing::error!("Failed to save network probe results for {server_id}: {e}");
                }
            } else {
                tracing::info!("Skipping recovery-frozen network probe write for {server_id}");
            }
        }
        // File management control responses — relay to pending HTTP requests
        AgentMessage::FileListResult { ref msg_id, .. } => {
            if !state
                .agent_manager
                .dispatch_pending_response(msg_id, msg.clone())
            {
                tracing::debug!("Orphaned FileListResult for msg_id={msg_id}");
            }
        }
        AgentMessage::FileStatResult { ref msg_id, .. } => {
            if !state
                .agent_manager
                .dispatch_pending_response(msg_id, msg.clone())
            {
                tracing::debug!("Orphaned FileStatResult for msg_id={msg_id}");
            }
        }
        AgentMessage::FileReadResult { ref msg_id, .. } => {
            if !state
                .agent_manager
                .dispatch_pending_response(msg_id, msg.clone())
            {
                tracing::debug!("Orphaned FileReadResult for msg_id={msg_id}");
            }
        }
        AgentMessage::FileOpResult { ref msg_id, .. } => {
            if !state
                .agent_manager
                .dispatch_pending_response(msg_id, msg.clone())
            {
                tracing::debug!("Orphaned FileOpResult for msg_id={msg_id}");
            }
        }

        // File download transfer messages
        AgentMessage::FileDownloadReady {
            ref transfer_id,
            size,
        } => {
            state.file_transfers.update_size(transfer_id, size);
            state.file_transfers.mark_in_progress(transfer_id);
            // Create the temp file and keep it open for the duration of the transfer
            if let Some(path) = state.file_transfers.temp_file_path(transfer_id) {
                match tokio::fs::File::create(&path).await {
                    Ok(file) => {
                        state.file_transfers.store_file_handle(transfer_id, file);
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to create temp file for transfer {transfer_id}: {e}"
                        );
                        state
                            .file_transfers
                            .mark_failed(transfer_id, format!("Failed to create temp file: {e}"));
                    }
                }
            }
        }
        AgentMessage::FileDownloadChunk {
            ref transfer_id,
            offset,
            ref data,
        } => {
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
                                state
                                    .file_transfers
                                    .update_progress(transfer_id, offset + bytes.len() as u64);
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to write chunk for transfer {transfer_id}: {e}"
                                );
                                state
                                    .file_transfers
                                    .mark_failed(transfer_id, format!("Write error: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to decode base64 chunk for transfer {transfer_id}: {e}"
                        );
                        state
                            .file_transfers
                            .mark_failed(transfer_id, format!("Base64 decode error: {e}"));
                    }
                }
            }
        }
        AgentMessage::FileDownloadEnd { ref transfer_id } => {
            state.file_transfers.remove_file_handle(transfer_id);
            state.file_transfers.mark_ready(transfer_id);
        }
        AgentMessage::FileDownloadError {
            ref transfer_id,
            ref error,
        } => {
            state.file_transfers.remove_file_handle(transfer_id);
            state.file_transfers.mark_failed(transfer_id, error.clone());
        }

        // File upload transfer messages
        AgentMessage::FileUploadAck {
            ref transfer_id,
            offset,
        } => {
            state.file_transfers.update_progress(transfer_id, offset);
            let ack_key = format!("upload-ack-{transfer_id}");
            state
                .agent_manager
                .dispatch_pending_response(&ack_key, msg.clone());
        }
        AgentMessage::FileUploadComplete { ref transfer_id } => {
            state.file_transfers.mark_ready(transfer_id);
            let complete_key = format!("upload-complete-{transfer_id}");
            state
                .agent_manager
                .dispatch_pending_response(&complete_key, msg.clone());
        }
        AgentMessage::FileUploadError {
            ref transfer_id,
            ref error,
        } => {
            state.file_transfers.mark_failed(transfer_id, error.clone());
            // The HTTP handler may be waiting on either an ack or complete key — try both.
            let ack_key = format!("upload-ack-{transfer_id}");
            let complete_key = format!("upload-complete-{transfer_id}");
            if !state
                .agent_manager
                .dispatch_pending_response(&complete_key, msg.clone())
            {
                state
                    .agent_manager
                    .dispatch_pending_response(&ack_key, msg.clone());
            }
        }

        AgentMessage::Pong => {
            // Agent responded to our protocol-level Ping; already handled by WS Pong frames
        }
        AgentMessage::RebindIdentityAck { job_id } => {
            match RecoveryMergeService::handle_rebind_ack(state, &job_id, server_id).await {
                Ok(job) => {
                    tracing::info!(
                        "Applied RebindIdentityAck from agent {server_id} for job_id={job_id}, stage={}",
                        job.stage
                    );
                    crate::router::ws::browser::broadcast_recovery_update(state).await;
                }
                Err(error) => {
                    tracing::warn!(
                        "Failed to apply RebindIdentityAck from agent {server_id} for job_id={job_id}: {error}"
                    );
                }
            }
        }
        AgentMessage::RebindIdentityFailed { job_id, error } => {
            match RecoveryJobService::mark_failed(
                &state.db,
                &job_id,
                RECOVERY_STAGE_REBINDING,
                &error,
            )
            .await
            {
                Ok(()) => {
                    tracing::warn!(
                        "Recorded RebindIdentityFailed from agent {server_id} for job_id={job_id}: {error}"
                    );
                    crate::router::ws::browser::broadcast_recovery_update(state).await;
                }
                Err(mark_error) => {
                    tracing::warn!(
                        "Failed to record RebindIdentityFailed from agent {server_id} for job_id={job_id}: {mark_error}"
                    );
                }
            }
        }

        // Docker variants
        AgentMessage::DockerInfo {
            ref msg_id,
            ref info,
        } => {
            state
                .agent_manager
                .update_docker_info(server_id, info.clone());
            if let Some(msg_id) = msg_id {
                state
                    .agent_manager
                    .dispatch_pending_response(msg_id, msg.clone());
            }
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
                    server_id: server_id.to_string(),
                    available: true,
                });
        }
        AgentMessage::DockerContainers {
            ref msg_id,
            ref containers,
        } => {
            state
                .agent_manager
                .update_docker_containers(server_id, containers.clone());
            if let Some(msg_id) = msg_id {
                state
                    .agent_manager
                    .dispatch_pending_response(msg_id, msg.clone());
            }
            let stats = state.agent_manager.get_docker_stats(server_id);
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::DockerUpdate {
                    server_id: server_id.to_string(),
                    containers: containers.clone(),
                    stats,
                });
        }
        AgentMessage::DockerStats { ref stats } => {
            state
                .agent_manager
                .update_docker_stats(server_id, stats.clone());
            if let Some(containers) = state.agent_manager.get_docker_containers(server_id) {
                state
                    .agent_manager
                    .broadcast_browser(BrowserMessage::DockerUpdate {
                        server_id: server_id.to_string(),
                        containers,
                        stats: Some(stats.clone()),
                    });
            }
        }
        AgentMessage::DockerLog {
            ref session_id,
            entries,
        } => {
            if let Some(tx) = state
                .agent_manager
                .get_docker_log_session(server_id, session_id)
            {
                let _ = tx.send(entries).await;
            }
        }
        AgentMessage::DockerEvent { event } => {
            if state.recovery_lock.writes_allowed_for(server_id) {
                let _ =
                    crate::service::docker::DockerService::save_event(&state.db, server_id, &event)
                        .await;
            } else {
                tracing::info!("Skipping recovery-frozen docker event write for {server_id}");
            }
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::DockerEvent {
                    server_id: server_id.to_string(),
                    event,
                });
        }
        AgentMessage::DockerUnavailable { ref msg_id } => {
            handle_docker_unavailable(state, server_id).await;

            if let Some(msg_id) = msg_id {
                state
                    .agent_manager
                    .dispatch_pending_response(msg_id, msg.clone());
            }
        }
        AgentMessage::FeaturesUpdate { ref features } => {
            if state.recovery_lock.writes_allowed_for(server_id) {
                let _ = crate::service::server::ServerService::update_features(
                    &state.db, server_id, features,
                )
                .await;
            } else {
                tracing::info!("Skipping recovery-frozen features write for {server_id}");
            }
            state
                .agent_manager
                .update_features(server_id, features.clone());
            let docker_available = features.contains(&"docker".to_string());
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
                    server_id: server_id.to_string(),
                    available: docker_available,
                });
        }
        AgentMessage::DockerNetworks { ref msg_id, .. }
        | AgentMessage::DockerVolumes { ref msg_id, .. }
        | AgentMessage::DockerActionResult { ref msg_id, .. } => {
            state
                .agent_manager
                .dispatch_pending_response(msg_id, msg.clone());
        }
        AgentMessage::IpChanged {
            ipv4,
            ipv6,
            interfaces: _,
        } => {
            match ServerService::get_server(&state.db, server_id).await {
                Ok(srv) => {
                    let old_ipv4 = srv.ipv4.clone();
                    let old_ipv6 = srv.ipv6.clone();
                    let ipv4_changed = old_ipv4 != ipv4;
                    let ipv6_changed = old_ipv6 != ipv6;

                    if ipv4_changed || ipv6_changed {
                        if state.recovery_lock.writes_allowed_for(server_id) {
                            // Update ipv4/ipv6 in DB
                            if let Err(e) =
                                update_server_ips(&state.db, server_id, &ipv4, &ipv6).await
                            {
                                tracing::error!("Failed to update IPs for {server_id}: {e}");
                            }
                        } else {
                            tracing::info!(
                                "Skipping recovery-frozen IP update write for {server_id}"
                            );
                        }

                        // Re-run GeoIP lookup based on the new IPs
                        let ip_to_lookup = ipv4
                            .as_deref()
                            .or(ipv6.as_deref())
                            .and_then(|ip| ip.parse::<std::net::IpAddr>().ok());
                        if let Some(ip) = ip_to_lookup {
                            let geo = {
                                let guard = state.geoip.read().unwrap();
                                guard.as_ref().map(|g| g.lookup(ip))
                            };
                            if let Some(geo) = geo {
                                if state.recovery_lock.writes_allowed_for(server_id) {
                                    if let Err(e) = update_server_geo(
                                        &state.db,
                                        server_id,
                                        geo.region,
                                        geo.country_code,
                                    )
                                    .await
                                    {
                                        tracing::error!(
                                            "Failed to update GeoIP for {server_id}: {e}"
                                        );
                                    }
                                } else {
                                    tracing::info!(
                                        "Skipping recovery-frozen GeoIP write for {server_id}"
                                    );
                                }
                            }
                        }

                        let detail = format!(
                            "IP changed for server {server_id}: ipv4 {:?} -> {:?}, ipv6 {:?} -> {:?}",
                            old_ipv4, ipv4, old_ipv6, ipv6
                        );
                        tracing::info!("{detail}");

                        let remote_ip = state
                            .agent_manager
                            .get_remote_addr(server_id)
                            .map(|a| a.ip().to_string())
                            .unwrap_or_default();
                        if state.recovery_lock.writes_allowed_for(server_id) {
                            if let Err(e) = AuditService::log(
                                &state.db,
                                "system",
                                "ip_changed",
                                Some(&detail),
                                &remote_ip,
                            )
                            .await
                            {
                                tracing::error!("Failed to write audit log for IP change: {e}");
                            }
                        } else {
                            tracing::info!(
                                "Skipping recovery-frozen IP-change audit write for {server_id}"
                            );
                        }

                        if state.recovery_lock.writes_allowed_for(server_id) {
                            if let Err(e) = AlertService::check_event_rules(
                                &state.db,
                                &state.alert_state_manager,
                                server_id,
                                "ip_changed",
                            )
                            .await
                            {
                                tracing::error!("Failed to check event rules for IP change: {e}");
                            }
                        } else {
                            tracing::info!(
                                "Skipping recovery-frozen alert evaluation for {server_id}"
                            );
                        }

                        state
                            .agent_manager
                            .broadcast_browser(BrowserMessage::ServerIpChanged {
                                server_id: server_id.to_string(),
                                old_ipv4,
                                new_ipv4: ipv4,
                                old_ipv6,
                                new_ipv6: ipv6,
                                old_remote_addr: None,
                                new_remote_addr: None,
                            });
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to load server {server_id} for IpChanged: {e}");
                }
            }
        }
        AgentMessage::TracerouteResult {
            request_id,
            target,
            hops,
            completed,
            error,
        } => {
            tracing::info!(
                "Received TracerouteResult from {server_id} (request_id={request_id}, completed={completed})"
            );
            state.agent_manager.update_traceroute_result(
                &request_id,
                crate::service::agent_manager::TracerouteResultData {
                    target,
                    hops,
                    completed,
                    error,
                },
            );
        }
    }
}

async fn handle_docker_unavailable(state: &Arc<AppState>, server_id: &str) {
    // Clear Docker caches (containers, stats, info) and log sessions — these are
    // also cleared by finish_connection_removal() on disconnect, but the
    // DockerUnavailable message can arrive while the agent is still connected
    // (e.g., Docker daemon stopped), so we must clear them here too.
    state.agent_manager.clear_docker_caches(server_id);
    state
        .agent_manager
        .remove_docker_log_sessions_for_server(server_id);

    // Shared cleanup: viewer tracker, features, DB persist, browser broadcast.
    crate::service::agent_manager::cleanup_disconnected_docker_state(state, server_id).await;
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

async fn audit_exec_finished(
    state: &Arc<AppState>,
    server_id: &str,
    result: &serverbee_common::types::TaskResult,
) -> Result<(), crate::error::AppError> {
    use crate::entity::task;
    use sea_orm::EntityTrait;

    let base_task_id = result.task_id.split(':').next().unwrap_or(&result.task_id);
    let Some(task_model) = task::Entity::find_by_id(base_task_id)
        .one(&state.db)
        .await?
    else {
        return Ok(());
    };

    if let Some(run_id) = result.task_id.split(':').nth(1)
        && let Some(context) = state.exec_audit_contexts.get(run_id)
    {
        let detail = serde_json::json!({
            "server_id": server_id,
            "task_id": task_model.id,
            "command": task_model.command,
            "exit_code": result.exit_code,
        })
        .to_string();
        AuditService::log(
            &state.db,
            &context.user_id,
            "exec_finished",
            Some(&detail),
            &context.ip,
        )
        .await?;
        return Ok(());
    }

    if task_model.task_type != "oneshot" {
        return Ok(());
    }

    let detail = serde_json::json!({
        "server_id": server_id,
        "task_id": task_model.id,
        "command": task_model.command,
        "exit_code": result.exit_code,
    })
    .to_string();
    AuditService::log(
        &state.db,
        &task_model.created_by,
        "exec_finished",
        Some(&detail),
        "system",
    )
    .await
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

/// Update the `last_remote_addr` field on a server record.
async fn update_last_remote_addr(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    addr: &str,
) -> Result<(), crate::error::AppError> {
    use crate::entity::server;
    use sea_orm::{ActiveModelTrait, Set};

    let model = ServerService::get_server(db, server_id).await?;
    let mut active: server::ActiveModel = model.into();
    active.last_remote_addr = Set(Some(addr.to_string()));
    active.updated_at = Set(chrono::Utc::now());
    active.update(db).await?;
    Ok(())
}

fn capability_denied_output(
    capability: &str,
    reason: serverbee_common::constants::CapabilityDeniedReason,
) -> String {
    match (capability, reason) {
        ("exec", serverbee_common::constants::CapabilityDeniedReason::ServerCapabilityDisabled) => {
            "Capability denied: exec disabled on server".to_string()
        }
        ("exec", serverbee_common::constants::CapabilityDeniedReason::AgentCapabilityDisabled) => {
            "Capability denied: exec blocked by agent local policy".to_string()
        }
        _ => format!("Capability denied: {capability}"),
    }
}

/// Update the `ipv4` and `ipv6` fields on a server record.
async fn update_server_ips(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    ipv4: &Option<String>,
    ipv6: &Option<String>,
) -> Result<(), crate::error::AppError> {
    use crate::entity::server;
    use sea_orm::{ActiveModelTrait, Set};

    let model = ServerService::get_server(db, server_id).await?;
    let mut active: server::ActiveModel = model.into();
    active.ipv4 = Set(ipv4.clone());
    active.ipv6 = Set(ipv6.clone());
    active.updated_at = Set(chrono::Utc::now());
    active.update(db).await?;
    Ok(())
}

/// Update the `region` and `country_code` GeoIP fields on a server record.
async fn update_server_geo(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    region: Option<String>,
    country_code: Option<String>,
) -> Result<(), crate::error::AppError> {
    use crate::entity::server;
    use sea_orm::{ActiveModelTrait, Set};

    let model = ServerService::get_server(db, server_id).await?;
    let mut active: server::ActiveModel = model.into();
    active.region = Set(region);
    active.country_code = Set(country_code);
    active.updated_at = Set(chrono::Utc::now());
    active.update(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::{recovery_job, server};
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use serverbee_common::constants::CAP_DEFAULT;
    use serverbee_common::protocol::{BrowserMessage, RecoveryJobStage};
    use std::net::{IpAddr, Ipv4Addr};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    async fn insert_server(db: &sea_orm::DatabaseConnection, id: &str, name: &str) {
        let now = Utc::now();
        let token_hash = AuthService::hash_password("test").unwrap();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(token_hash),
            token_prefix: Set("serverbee_test".to_string()),
            name: Set(name.to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
    }

    async fn insert_recovery_job(
        db: &sea_orm::DatabaseConnection,
        job_id: &str,
        target_server_id: &str,
        source_server_id: &str,
    ) {
        let now = Utc::now();
        recovery_job::ActiveModel {
            job_id: Set(job_id.to_string()),
            target_server_id: Set(target_server_id.to_string()),
            source_server_id: Set(source_server_id.to_string()),
            status: Set("running".to_string()),
            stage: Set("rebinding".to_string()),
            checkpoint_json: Set(None),
            error: Set(None),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(Some(now)),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn current_connection_frame_handler_waits_for_server_lock() {
        let (db, _tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();
        let (tx, _) = mpsc::channel(1);
        let connection_id =
            state
                .agent_manager
                .add_connection("s1".into(), "Srv".into(), tx, test_addr());

        let server_lock = state.agent_manager.server_cleanup_lock("s1");
        let held_guard = server_lock.lock().await;

        let task_state = Arc::clone(&state);
        let handle_task = tokio::spawn(async move {
            handle_current_connection_frame(
                &task_state,
                "s1",
                connection_id,
                CurrentConnectionFrame::Pong,
            )
            .await
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!handle_task.is_finished());

        drop(held_guard);

        assert!(handle_task.await.unwrap());
    }

    #[tokio::test]
    async fn current_connection_frame_handler_stops_superseded_connection() {
        let (db, _tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();
        let (tx1, _) = mpsc::channel(1);
        let (tx2, _) = mpsc::channel(1);
        let first_connection_id =
            state
                .agent_manager
                .add_connection("s1".into(), "Srv".into(), tx1, test_addr());
        let second_connection_id =
            state
                .agent_manager
                .add_connection("s1".into(), "Srv".into(), tx2, test_addr());

        assert_ne!(first_connection_id, second_connection_id);
        assert!(
            !handle_current_connection_frame(
                &state,
                "s1",
                first_connection_id,
                CurrentConnectionFrame::Pong,
            )
            .await
        );
        assert!(
            state
                .agent_manager
                .is_current_connection("s1", second_connection_id)
        );
    }

    #[tokio::test]
    async fn rebind_identity_ack_advances_job_and_broadcasts_recovery_update() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        insert_recovery_job(&db, "job-1", "target-1", "source-1").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();
        let mut browser_rx = state.browser_tx.subscribe();

        handle_agent_message(
            &state,
            "source-1",
            AgentMessage::RebindIdentityAck {
                job_id: "job-1".to_string(),
            },
        )
        .await;

        let job = recovery_job::Entity::find_by_id("job-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.stage, "awaiting_target_online");

        let msg = browser_rx.recv().await.unwrap();
        match msg {
            BrowserMessage::Update {
                recoveries: Some(recoveries),
                ..
            } => {
                assert_eq!(recoveries.len(), 1);
                assert_eq!(recoveries[0].job_id, "job-1");
                assert_eq!(recoveries[0].stage, RecoveryJobStage::AwaitingTargetOnline);
            }
            other => panic!("expected recovery update, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rebind_identity_failed_marks_job_failed_and_broadcasts_empty_recovery_update() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        insert_recovery_job(&db, "job-1", "target-1", "source-1").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();
        let mut browser_rx = state.browser_tx.subscribe();

        handle_agent_message(
            &state,
            "source-1",
            AgentMessage::RebindIdentityFailed {
                job_id: "job-1".to_string(),
                error: "agent failed".to_string(),
            },
        )
        .await;

        let job = recovery_job::Entity::find_by_id("job-1")
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.status, "failed");
        assert_eq!(job.stage, "rebinding");
        assert_eq!(job.error.as_deref(), Some("agent failed"));

        let msg = browser_rx.recv().await.unwrap();
        match msg {
            BrowserMessage::Update {
                recoveries: Some(recoveries),
                ..
            } => {
                assert!(recoveries.is_empty());
            }
            other => panic!("expected recovery update, got {other:?}"),
        }
    }
}
