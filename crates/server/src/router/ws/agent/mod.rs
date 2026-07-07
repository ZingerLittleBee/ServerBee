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
use crate::service::auth::AuthService;
use crate::service::ip_quality::IpQualityService;
use crate::service::network_probe::NetworkProbeService;
use crate::service::ping::PingService;
use crate::service::record::RecordService;
use crate::service::upgrade_tracker::UpgradeLookup;
use crate::state::AppState;
use serverbee_common::constants::MAX_WS_MESSAGE_SIZE;
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use serverbee_common::types::NetworkProbeTarget as NetworkProbeTargetDto;

mod docker;
mod file_transfer;
mod network;
mod security;
mod system_info;
mod task_exec;

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

    // Send Welcome message. Capabilities are agent-owned, so the server does
    // NOT advertise any: the agent enforces purely on its local policy and
    // ignores this field.
    let welcome = ServerMessage::Welcome {
        server_id: server_id.clone(),
        protocol_version: serverbee_common::constants::PROTOCOL_VERSION,
        report_interval: 3,
        capabilities: None,
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
        // Seed the last-known agent capabilities from the persisted mirror so
        // enforcement/display has a value before the agent's first SystemInfo.
        // The agent overwrites this with its live value moments later.
        state
            .agent_manager
            .update_agent_local_capabilities(&server_id, server_capabilities as u32);
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
    // Drop any temporary-grant countdowns so a disconnected agent does not leave
    // stale grants ticking in the REST DTO / browser view.
    state
        .agent_manager
        .update_temporary_grants(&server_id, vec![]);
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
            temporary,
        } => {
            system_info::on_system_info(
                state,
                server_id,
                msg_id,
                info,
                agent_local_capabilities,
                temporary,
            )
            .await;
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
            task_exec::on_task_result(state, server_id, msg_id, result).await;
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
            network::on_ping_result(state, server_id, result).await;
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
            task_exec::on_capability_denied(state, server_id, msg_id, session_id, capability, reason)
                .await;
        }
        AgentMessage::NetworkProbeResults { results } => {
            network::on_network_probe_results(state, server_id, results).await;
        }
        // File management control responses — relay to pending HTTP requests
        AgentMessage::FileListResult { ref msg_id, .. }
        | AgentMessage::FileStatResult { ref msg_id, .. }
        | AgentMessage::FileReadResult { ref msg_id, .. }
        | AgentMessage::FileOpResult { ref msg_id, .. } => {
            file_transfer::relay_control_response(state, msg_id, &msg);
        }
        // File download transfer messages
        AgentMessage::FileDownloadReady {
            ref transfer_id,
            size,
        } => {
            file_transfer::on_download_ready(state, transfer_id, size).await;
        }
        AgentMessage::FileDownloadChunk {
            ref transfer_id,
            offset,
            ref data,
        } => {
            file_transfer::on_download_chunk(state, transfer_id, offset, data).await;
        }
        AgentMessage::FileDownloadEnd { ref transfer_id } => {
            file_transfer::on_download_end(state, transfer_id);
        }
        AgentMessage::FileDownloadError {
            ref transfer_id,
            ref error,
        } => {
            file_transfer::on_download_error(state, transfer_id, error);
        }
        // File upload transfer messages
        AgentMessage::FileUploadAck {
            ref transfer_id,
            offset,
        } => {
            file_transfer::on_upload_ack(state, transfer_id, offset, &msg);
        }
        AgentMessage::FileUploadComplete { ref transfer_id } => {
            file_transfer::on_upload_complete(state, transfer_id, &msg);
        }
        AgentMessage::FileUploadError {
            ref transfer_id,
            ref error,
        } => {
            file_transfer::on_upload_error(state, transfer_id, error, &msg);
        }
        AgentMessage::Pong => {
            // Agent responded to our protocol-level Ping; already handled by WS Pong frames
        }
        // Docker variants
        AgentMessage::DockerInfo {
            ref msg_id,
            ref info,
        } => {
            docker::on_docker_info(state, server_id, msg_id.as_ref(), info, &msg);
        }
        AgentMessage::DockerContainers {
            ref msg_id,
            ref containers,
        } => {
            docker::on_docker_containers(state, server_id, msg_id.as_ref(), containers, &msg);
        }
        AgentMessage::DockerStats { ref stats } => {
            docker::on_docker_stats(state, server_id, stats);
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
            docker::on_docker_event(state, server_id, event).await;
        }
        AgentMessage::DockerUnavailable { ref msg_id } => {
            docker::on_docker_unavailable(state, server_id, msg_id.as_ref(), &msg).await;
        }
        AgentMessage::FeaturesUpdate { ref features } => {
            docker::on_features_update(state, server_id, features).await;
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
            system_info::on_ip_changed(state, server_id, ipv4, ipv6).await;
        }
        AgentMessage::TracerouteResult {
            request_id,
            target,
            hops,
            completed,
            error,
        } => {
            network::on_traceroute_result(
                state, server_id, request_id, target, hops, completed, error,
            )
            .await;
        }
        msg @ AgentMessage::TracerouteRoundUpdate { .. } => {
            network::on_traceroute_round_update(state, server_id, msg).await;
        }
        AgentMessage::SecurityEvent(payload) => {
            security::on_security_event(state, server_id, payload).await;
        }
        AgentMessage::BlocklistAck { results } => {
            security::on_blocklist_ack(state, server_id, results).await;
        }
        AgentMessage::BlocklistResetAck { ok, reason } => {
            security::on_blocklist_reset_ack(state, server_id, ok, reason).await;
        }
        AgentMessage::CapabilitiesChanged {
            msg_id: _,
            capabilities,
            temporary,
            changes,
        } => {
            security::on_capabilities_changed(state, server_id, capabilities, temporary, changes)
                .await;
        }
        AgentMessage::UnlockResults {
            egress_ip,
            results,
            checked_at,
        } => {
            security::on_unlock_results(state, server_id, egress_ip, results, checked_at).await;
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

    #[tokio::test]
    async fn unlock_results_denied_when_ip_quality_capability_missing() {
        use crate::entity::{audit_log, unlock_result};
        use crate::service::ip_quality::IpQualityService;
        use serverbee_common::protocol::{UnlockResultData, UnlockStatus};

        let (db, _tmp) = setup_test_db().await;
        // capabilities = 0 → CAP_IP_QUALITY bit cleared (e.g. revoked mid-run).
        insert_server_with_caps(&db, "srv-noiq", "No IQ", 0).await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();
        let mut browser_rx = state.browser_tx.subscribe();

        let svc_id = IpQualityService::enabled_service_defs(&db).await.unwrap()[0]
            .id
            .clone();

        handle_agent_message(
            &state,
            "srv-noiq",
            AgentMessage::UnlockResults {
                egress_ip: "203.0.113.10".to_string(),
                results: vec![UnlockResultData {
                    service_id: svc_id,
                    status: UnlockStatus::Unlocked,
                    region: Some("US".to_string()),
                    latency_ms: Some(150),
                    detail: None,
                }],
                checked_at: Utc::now(),
            },
        )
        .await;

        // No unlock_result rows persisted.
        let rows: Vec<_> = unlock_result::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.server_id == "srv-noiq")
            .collect();
        assert!(
            rows.is_empty(),
            "unlock results must be dropped when CAP_IP_QUALITY is not effective"
        );

        // No IpQualityUpdate broadcast.
        let recv = timeout(Duration::from_millis(200), browser_rx.recv()).await;
        assert!(
            recv.is_err(),
            "no IpQualityUpdate should be broadcast when CAP_IP_QUALITY is revoked"
        );

        // The denial is audited.
        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "ip_quality_results_denied"),
            "expected ip_quality_results_denied audit row, got {logs:?}"
        );
    }
}
