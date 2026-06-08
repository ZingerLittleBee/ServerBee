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

use crate::router::utils::extract_client_ip;
use crate::service::alert::AlertService;
use crate::service::audit::AuditService;
use crate::service::auth::AuthService;
use crate::service::geoip;
use crate::service::ip_quality::IpQualityService;
use crate::service::ip_risk::IpRiskService;
use crate::service::network_probe::NetworkProbeService;
use crate::service::ping::PingService;
use crate::service::record::RecordService;
use crate::service::server::ServerService;
use crate::service::upgrade_tracker::UpgradeLookup;
use crate::state::AppState;
use serverbee_common::constants::{
    CAP_SECURITY_EVENTS, MAX_WS_MESSAGE_SIZE, effective_capabilities, has_capability,
};
use serverbee_common::protocol::{AgentMessage, BrowserMessage, ServerMessage};
use serverbee_common::types::NetworkProbeTarget as NetworkProbeTargetDto;

#[derive(Debug, Deserialize)]
pub struct OptionalWsQuery {
    token: Option<String>,
}

fn extract_agent_token(headers: &HeaderMap, query: &OptionalWsQuery) -> Option<String> {
    // Prefer the Authorization header. Unlike the query string, it is not
    // captured in reverse-proxy access logs, browser history, or Referer headers
    // (CWE-598). The agent always sends this header alongside the query param.
    if let Some(auth) = headers.get("authorization")
        && let Ok(val) = auth.to_str()
        && let Some(token) = val.strip_prefix("Bearer ")
    {
        return Some(token.to_string());
    }
    // Fall back to the query param for proxies/load balancers that strip the
    // Authorization header.
    if let Some(ref token) = query.token {
        return Some(token.clone());
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
    // Honor X-Forwarded-For when the TCP source is a trusted proxy (e.g. Railway,
    // Cloudflare). Without this, behind-proxy deployments record the LB's
    // internal IP as the agent's remote_addr and GeoIP can never resolve a
    // country.
    let client_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    );
    let addr = SocketAddr::new(client_ip, addr.port());

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

    // Send IP quality sync to the newly connected agent (mirrors NetworkProbeSync)
    match IpQualityService::enabled_service_defs(&state.db).await {
        Ok(services) => match IpQualityService::get_setting(&state.db).await {
            Ok(setting) => {
                if let Some(tx) = state.agent_manager.get_sender(&server_id) {
                    let _ = tx
                        .send(ServerMessage::IpQualitySync {
                            services,
                            interval_hours: setting.check_interval_hours as u32,
                        })
                        .await;
                }
            }
            Err(e) => {
                tracing::error!("Failed to get IP quality setting for {server_id}: {e}");
            }
        },
        Err(e) => {
            tracing::error!("Failed to get IP quality service defs for {server_id}: {e}");
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
            // Resolve GeoIP. Walk the candidate chain agent ipv4 → ipv6 →
            // remote_addr and skip loopback/private addresses — GeoIP can't
            // resolve those (e.g. agents inside a docker container report the
            // bridge gateway 172.17.0.1 as their primary IP).
            let parse = |s: Option<&str>| s.and_then(|v| v.parse::<std::net::IpAddr>().ok());
            let candidates = [
                parse(info.ipv4.as_deref()),
                parse(info.ipv6.as_deref()),
                state
                    .agent_manager
                    .get_remote_addr(server_id)
                    .map(|addr| addr.ip()),
            ];
            let ip = candidates
                .into_iter()
                .flatten()
                .find(|ip| !ip.is_loopback() && !geoip::is_private(ip));

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
                }

                // Check if agent-reported IPs changed
                let ipv4_changed = old_ipv4 != info.ipv4;
                let ipv6_changed = old_ipv6 != info.ipv6;
                let remote_changed = old_remote_addr.as_ref() != current_remote_addr.as_ref();

                if ipv4_changed || ipv6_changed || remote_changed {
                    if let Err(e) = AlertService::check_event_rules(
                        &state.db,
                        &state.config,
                        &state.alert_state_manager,
                        server_id,
                        "ip_changed",
                    )
                    .await
                    {
                        tracing::error!("Failed to check event rules for IP change: {e}");
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
                if let Some(ref addr) = current_remote_addr
                    && let Err(e) = update_last_remote_addr(&state.db, server_id, addr).await
                {
                    tracing::error!("Failed to update last_remote_addr for {server_id}: {e}");
                }
            }

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

            let _ = crate::service::server::ServerService::update_features(
                &state.db,
                server_id,
                &info.features,
            )
            .await;
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

            // Record agent's external IP so the firewall guardrail's
            // dynamic allow-list keeps the agent from blocking itself.
            let fw_ip = info
                .ipv4
                .as_deref()
                .or(info.ipv6.as_deref())
                .and_then(|s| s.parse::<std::net::IpAddr>().ok());
            state
                .firewall
                .note_agent_external_ip(server_id, fw_ip)
                .await;

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

            // Firewall blocklist: on every fresh SystemInfo, drop whatever the
            // agent may have leftover from a previous boot, then resend the
            // authoritative set. Gated on capability + protocol version.
            {
                use serverbee_common::constants::{CAP_FIREWALL_BLOCK, has_capability};
                use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;

                let caps = state
                    .agent_manager
                    .get_effective_capabilities(server_id)
                    .unwrap_or(0);
                if has_capability(caps, CAP_FIREWALL_BLOCK) && agent_pv >= FIREWALL_MIN_PROTOCOL {
                    state
                        .firewall
                        .push_reset_to(server_id, &state.agent_manager)
                        .await;
                    if let Err(e) = state
                        .firewall
                        .push_sync_to(server_id, &state.agent_manager)
                        .await
                    {
                        tracing::warn!(server_id, error = %e, "firewall sync push failed");
                    }
                }
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
                if let Err(e) = save_task_result(&state.db, server_id, &result).await {
                    tracing::error!("Failed to save task result for {server_id}: {e}");
                }
            }
            if let Err(e) = audit_exec_finished(state, server_id, &result).await {
                tracing::error!("Failed to write exec_finished audit log for {server_id}: {e}");
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
            if let Err(e) = save_ping_result(&state.db, server_id, &result).await {
                tracing::error!("Failed to save ping result for {server_id}: {e}");
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
            if let Err(e) = NetworkProbeService::save_results(&state.db, server_id, results).await {
                tracing::error!("Failed to save network probe results for {server_id}: {e}");
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
            let _ =
                crate::service::docker::DockerService::save_event(&state.db, server_id, &event)
                    .await;
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
            let _ = crate::service::server::ServerService::update_features(
                &state.db, server_id, features,
            )
            .await;
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
            // Refresh the firewall guardrail's dynamic allow-list with the
            // agent's new external IP. Done first so that any later auto-block
            // evaluation in this scope sees the up-to-date value.
            let fw_ip = ipv4
                .as_deref()
                .or(ipv6.as_deref())
                .and_then(|s| s.parse::<std::net::IpAddr>().ok());
            state
                .firewall
                .note_agent_external_ip(server_id, fw_ip)
                .await;

            match ServerService::get_server(&state.db, server_id).await {
                Ok(srv) => {
                    let old_ipv4 = srv.ipv4.clone();
                    let old_ipv6 = srv.ipv6.clone();
                    let ipv4_changed = old_ipv4 != ipv4;
                    let ipv6_changed = old_ipv6 != ipv6;

                    if ipv4_changed || ipv6_changed {
                        // Update ipv4/ipv6 in DB
                        if let Err(e) = update_server_ips(&state.db, server_id, &ipv4, &ipv6).await
                        {
                            tracing::error!("Failed to update IPs for {server_id}: {e}");
                        }

                        // Re-run GeoIP lookup. Same private/loopback filter as
                        // the SystemInfo path; fall back to remote_addr when
                        // the agent only knows internal/bridge addresses.
                        let parse =
                            |s: Option<&str>| s.and_then(|v| v.parse::<std::net::IpAddr>().ok());
                        let candidates = [
                            parse(ipv4.as_deref()),
                            parse(ipv6.as_deref()),
                            state
                                .agent_manager
                                .get_remote_addr(server_id)
                                .map(|addr| addr.ip()),
                        ];
                        let ip_to_lookup = candidates
                            .into_iter()
                            .flatten()
                            .find(|ip| !ip.is_loopback() && !geoip::is_private(ip));
                        if let Some(ip) = ip_to_lookup {
                            let geo = {
                                let guard = state.geoip.read().unwrap();
                                guard.as_ref().map(|g| g.lookup(ip))
                            };
                            if let Some(geo) = geo
                                && let Err(e) = update_server_geo(
                                    &state.db,
                                    server_id,
                                    geo.region,
                                    geo.country_code,
                                )
                                .await
                            {
                                tracing::error!("Failed to update GeoIP for {server_id}: {e}");
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

                        if let Err(e) = AlertService::check_event_rules(
                            &state.db,
                            &state.config,
                            &state.alert_state_manager,
                            server_id,
                            "ip_changed",
                        )
                        .await
                        {
                            tracing::error!("Failed to check event rules for IP change: {e}");
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
                "Received legacy TracerouteResult from {server_id} (request_id={request_id})"
            );
            // Legacy agent does not report which probe protocol actually ran (UDP
            // for Unix `traceroute`, ICMP for `mtr` / Windows `tracert`). Persist
            // with the "legacy" sentinel.
            state.agent_manager.set_traceroute_meta_protocol(
                &request_id,
                serverbee_common::protocol::RecordedProtocol::Legacy,
            );
            // Re-dispatch into the new pipeline as a single-round update.
            let synthetic = AgentMessage::TracerouteRoundUpdate {
                request_id, target, round: 1, total_rounds: 1, hops, completed, error,
            };
            handle_traceroute_round_update(state, server_id, synthetic).await;
        }
        msg @ AgentMessage::TracerouteRoundUpdate { .. } => {
            handle_traceroute_round_update(state, server_id, msg).await;
        }
        AgentMessage::SecurityEvent(payload) => {
            // The hot-path cache is populated once the agent sends `SystemInfo`
            // on the current connection; before that, fall back to the DB row.
            let caps = match state.agent_manager.get_effective_capabilities(server_id) {
                Some(c) => c,
                None => {
                    use crate::entity::server;
                    use sea_orm::EntityTrait;
                    server::Entity::find_by_id(server_id)
                        .one(&state.db)
                        .await
                        .ok()
                        .flatten()
                        .and_then(|s| u32::try_from(s.capabilities).ok())
                        .unwrap_or(0)
                }
            };
            if !has_capability(caps, CAP_SECURITY_EVENTS) {
                let detail = serde_json::json!({ "server_id": server_id }).to_string();
                if let Err(e) = AuditService::log(
                    &state.db,
                    "system",
                    "security_event_denied",
                    Some(&detail),
                    "",
                )
                .await
                {
                    tracing::warn!(server_id, error = %e, "audit log for security_event_denied failed");
                }
                return;
            }
            if let Err(e) = state.security_service.record_event(server_id, payload).await {
                tracing::error!(server_id, error = %e, "security_event record failed");
            }
        }
        AgentMessage::BlocklistAck { results } => {
            for item in results {
                state.firewall.record_ack(server_id, item, &state.db).await;
            }
        }
        AgentMessage::BlocklistResetAck { ok, reason } => {
            state
                .firewall
                .record_reset_ack(server_id, ok, reason, &state.db)
                .await;
        }
        AgentMessage::UnlockResults {
            egress_ip,
            results,
            checked_at,
        } => {
            // Phase 1 (synchronous-ish): save unlock results + broadcast immediately
            // with ip_quality = None so the UI shows fresh unlock data right away.
            if let Err(e) =
                IpQualityService::save_unlock_results(&state.db, server_id, results.clone()).await
            {
                tracing::error!("Failed to save unlock results for {server_id}: {e}");
            }

            state
                .agent_manager
                .broadcast_browser(BrowserMessage::IpQualityUpdate {
                    server_id: server_id.to_string(),
                    unlock_results: results.clone(),
                    ip_quality: None,
                });

            // Phase 2 (non-blocking): spawn a background task to run IP risk scoring
            // and emit a second broadcast with the full ip_quality snapshot.
            // Wrapped in a 30s timeout so a slow/down provider never blocks the agent loop.
            // Skip entirely when egress_ip is empty — an empty IP produces no
            // meaningful snapshot and would contaminate ip_risk_cache with a "" key.
            if egress_ip.trim().is_empty() {
                tracing::debug!(
                    "UnlockResults from {server_id}: egress_ip is empty, skipping IP risk scoring"
                );
            } else {
            let db_bg = state.db.clone();
            let geoip_bg = Arc::clone(&state.geoip);
            let config_bg = state.config.ip_quality.clone();
            let browser_tx_bg = state.browser_tx.clone();
            let server_id_owned = server_id.to_string();
            // Keep a copy for the timeout warning (the inner async moves server_id_owned)
            let server_id_for_warn = server_id_owned.clone();

            tokio::spawn(async move {
                let result = tokio::time::timeout(
                    Duration::from_secs(30),
                    async move {
                        let risk_service = IpRiskService::new(config_bg);
                        // score_ip returns None for a blank IP (defensive double-guard).
                        let Some(snapshot) = risk_service
                            .score_ip(&db_bg, &geoip_bg, &egress_ip)
                            .await
                        else {
                            return;
                        };

                        if let Err(e) = IpQualityService::save_ip_quality_snapshot(
                            &db_bg,
                            &server_id_owned,
                            &snapshot,
                        )
                        .await
                        {
                            // Phase 2 is a non-critical enrichment step: the UI already
                            // received the unlock matrix from the Phase 1 broadcast, so a
                            // failed snapshot persist is logged at warn (not error).
                            tracing::warn!(
                                "Failed to save ip_quality_snapshot for {}: {e}",
                                server_id_owned
                            );
                        }

                        let _ = browser_tx_bg.send(BrowserMessage::IpQualityUpdate {
                            server_id: server_id_owned,
                            unlock_results: results,
                            ip_quality: Some(snapshot),
                        });

                        // checked_at is part of the protocol message but the server uses
                        // its own Utc::now() for timestamps (the agent's clock may differ).
                        let _ = checked_at;
                    },
                )
                .await;

                if result.is_err() {
                    tracing::warn!(
                        "IP risk scoring timed out for agent {server_id_for_warn}"
                    );
                }
            });
            } // end else egress_ip non-empty
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

async fn handle_traceroute_round_update(state: &Arc<AppState>, server_id: &str, msg: AgentMessage) {
    let AgentMessage::TracerouteRoundUpdate {
        request_id,
        target: _,
        round,
        total_rounds,
        mut hops,
        completed,
        error,
    } = msg
    else {
        unreachable!("handle_traceroute_round_update called with non-TracerouteRoundUpdate msg");
    };

    // Defense-in-depth: reject updates whose request_id was registered for a
    // different server. The placeholder is keyed by request_id only, so a
    // compromised agent that learned another server's request_id could
    // otherwise overwrite the victim's cache and trigger a poisoned DB insert.
    if let Some(meta) = state.agent_manager.get_traceroute_meta(&request_id)
        && meta.server_id != server_id
    {
        tracing::warn!(
            "Dropping TracerouteRoundUpdate {request_id}: server_id mismatch (placeholder={}, sender={server_id})",
            meta.server_id
        );
        return;
    }

    // Server-side enrich: PTR hostnames and ASN (when MMDB is installed).
    state.traceroute_enricher.enrich(&mut hops).await;

    // Update in-memory cache
    let Some(snapshot) = state.agent_manager.update_traceroute_round(
        &request_id,
        round,
        total_rounds,
        hops.clone(),
        completed,
        error.clone(),
    ) else {
        tracing::warn!(
            "Dropping TracerouteRoundUpdate {request_id}: no cached placeholder"
        );
        return;
    };

    // On completion, persist a DB row
    if completed {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let new_record = crate::service::traceroute::NewTracerouteRecord {
            id: request_id.clone(),
            server_id: snapshot.server_id.clone(),
            target: snapshot.target.clone(),
            protocol: snapshot.protocol,
            started_at: snapshot.started_at,
            completed_at: Some(now_ms),
            total_rounds: snapshot.total_rounds,
            completed_rounds: snapshot.round,
            hops: snapshot.hops.clone(),
            error: snapshot.error.clone(),
        };
        if let Err(e) = crate::service::traceroute::insert_completed_record(&state.db, new_record).await {
            tracing::warn!("Failed to persist traceroute record {request_id}: {e:?}");
        }
    }

    // Broadcast to subscribed browsers
    let _ = state.browser_tx.send(serverbee_common::protocol::BrowserMessage::TracerouteUpdate {
        server_id: snapshot.server_id.clone(),
        request_id: request_id.clone(),
        target: snapshot.target.clone(),
        protocol: snapshot.protocol,
        started_at: snapshot.started_at,
        round: snapshot.round,
        total_rounds: snapshot.total_rounds,
        hops: snapshot.hops,
        completed: snapshot.completed,
        error: snapshot.error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::server;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::time::{Duration, timeout};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    #[test]
    fn extract_agent_token_prefers_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer header-token".parse().unwrap());
        let query = OptionalWsQuery {
            token: Some("query-token".to_string()),
        };
        // Header wins so the secret stays out of proxy access logs.
        assert_eq!(
            extract_agent_token(&headers, &query),
            Some("header-token".to_string())
        );
    }

    #[test]
    fn extract_agent_token_falls_back_to_query() {
        let headers = HeaderMap::new();
        let query = OptionalWsQuery {
            token: Some("query-token".to_string()),
        };
        assert_eq!(
            extract_agent_token(&headers, &query),
            Some("query-token".to_string())
        );
    }

    #[test]
    fn extract_agent_token_none_when_absent() {
        let headers = HeaderMap::new();
        let query = OptionalWsQuery { token: None };
        assert_eq!(extract_agent_token(&headers, &query), None);
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

    // ── SecurityEvent capability gating ──

    async fn insert_server_with_caps(
        db: &sea_orm::DatabaseConnection,
        id: &str,
        name: &str,
        capabilities: u32,
    ) {
        let now = Utc::now();
        let token_hash = AuthService::hash_password("test").unwrap();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some(token_hash)),
            token_prefix: Set(Some("serverbee_test".to_string())),
            name: Set(name.to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(capabilities as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
    }

    fn security_event_payload(ip: &str) -> serverbee_common::security::SecurityEventPayload {
        use serverbee_common::security::{
            DetectorSource, SecurityEventPayload, SecurityEventType, SecurityEvidence, Severity,
        };
        SecurityEventPayload {
            event_type: SecurityEventType::SshBruteForce,
            severity: Severity::High,
            source_ip: ip.to_string(),
            source_port: None,
            username: None,
            started_at: 1_700_000_000,
            ended_at: 1_700_000_060,
            first_seen: false,
            detector_source: DetectorSource::Journal,
            evidence: SecurityEvidence::SshBruteForce {
                failed_count: 12,
                distinct_users: 1,
                sample_users: vec!["root".into()],
                invalid_user_count: 0,
                window_seconds: 60,
                threshold: 10,
            },
        }
    }

    // ── IP Quality: UnlockResults handling ──

    #[tokio::test]
    async fn unlock_results_persists_rows_and_broadcasts_ip_quality_update() {
        use crate::entity::unlock_result;
        use crate::service::ip_quality::IpQualityService;
        use serverbee_common::constants::CAP_IP_QUALITY;
        use serverbee_common::protocol::{BrowserMessage, UnlockResultData, UnlockStatus};

        let (db, _tmp) = setup_test_db().await;

        // Insert a server with CAP_IP_QUALITY set
        let now = Utc::now();
        let token_hash = AuthService::hash_password("test").unwrap();
        server::ActiveModel {
            id: Set("srv-iq".to_string()),
            token_hash: Set(Some(token_hash)),
            token_prefix: Set(Some("serverbee_test".to_string())),
            name: Set("IQ Server".to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_IP_QUALITY as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();
        let mut browser_rx = state.browser_tx.subscribe();

        // Get the first enabled service to use as the service_id in results
        let services = IpQualityService::enabled_service_defs(&db).await.unwrap();
        let svc_id = services[0].id.clone();

        let results = vec![UnlockResultData {
            service_id: svc_id.clone(),
            status: UnlockStatus::Unlocked,
            region: Some("US".to_string()),
            latency_ms: Some(150),
            detail: None,
        }];

        handle_agent_message(
            &state,
            "srv-iq",
            AgentMessage::UnlockResults {
                egress_ip: "203.0.113.10".to_string(),
                results,
                checked_at: Utc::now(),
            },
        )
        .await;

        // (a) Verify unlock_result rows persisted (fetch all, filter in Rust)
        let db_results: Vec<_> = unlock_result::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.server_id == "srv-iq")
            .collect();
        assert_eq!(db_results.len(), 1, "one unlock_result row should be persisted");
        assert_eq!(db_results[0].service_id, svc_id);
        assert_eq!(db_results[0].status, "unlocked");

        // (b) Verify the immediate IpQualityUpdate broadcast (ip_quality = None)
        let msg = timeout(Duration::from_millis(200), browser_rx.recv())
            .await
            .expect("should receive immediate broadcast")
            .unwrap();

        match msg {
            BrowserMessage::IpQualityUpdate {
                server_id,
                unlock_results,
                ip_quality,
            } => {
                assert_eq!(server_id, "srv-iq");
                assert_eq!(unlock_results.len(), 1);
                assert_eq!(unlock_results[0].service_id, svc_id);
                assert!(
                    ip_quality.is_none(),
                    "first broadcast must have ip_quality = None"
                );
            }
            other => panic!("expected IpQualityUpdate, got {other:?}"),
        }

        // (c) Wait for the background task's second broadcast (ip_quality = Some)
        // The scoring runs in a spawned task; give it up to 2 seconds.
        let second_msg = timeout(Duration::from_secs(2), browser_rx.recv())
            .await
            .expect("should receive background ip_quality broadcast")
            .unwrap();

        match second_msg {
            BrowserMessage::IpQualityUpdate {
                server_id,
                ip_quality,
                ..
            } => {
                assert_eq!(server_id, "srv-iq");
                assert!(
                    ip_quality.is_some(),
                    "second broadcast must carry ip_quality snapshot"
                );
                let snap = ip_quality.unwrap();
                assert_eq!(snap.ip, "203.0.113.10");
            }
            other => panic!("expected second IpQualityUpdate with ip_quality, got {other:?}"),
        }

        // (d) Verify the ip_quality_snapshot was persisted (filter in Rust)
        let snapshot_rows: Vec<_> = crate::entity::ip_quality_snapshot::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.server_id == "srv-iq")
            .collect();
        assert!(!snapshot_rows.is_empty(), "ip_quality_snapshot row should be persisted");
        assert_eq!(snapshot_rows[0].ip, "203.0.113.10");
    }

    #[tokio::test]
    async fn security_event_persists_when_capability_granted() {
        use crate::entity::security_event;
        use serverbee_common::constants::CAP_SECURITY_EVENTS;

        let (db, _tmp) = setup_test_db().await;
        insert_server_with_caps(&db, "srv-1", "Srv", CAP_SECURITY_EVENTS).await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        handle_agent_message(
            &state,
            "srv-1",
            AgentMessage::SecurityEvent(security_event_payload("203.0.113.5")),
        )
        .await;

        let rows = security_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_ip, "203.0.113.5");
    }

    #[tokio::test]
    async fn security_event_denied_audits_when_capability_missing() {
        use crate::entity::{audit_log, security_event};

        let (db, _tmp) = setup_test_db().await;
        // capabilities = 0 → CAP_SECURITY_EVENTS bit cleared.
        insert_server_with_caps(&db, "srv-1", "Srv", 0).await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        handle_agent_message(
            &state,
            "srv-1",
            AgentMessage::SecurityEvent(security_event_payload("203.0.113.6")),
        )
        .await;

        let rows = security_event::Entity::find().all(&db).await.unwrap();
        assert!(rows.is_empty(), "should not persist without capability");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "security_event_denied"),
            "expected security_event_denied audit row, got {logs:?}"
        );
    }
}
