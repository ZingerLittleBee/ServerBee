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

use crate::entity::{recovery_job, server_tag};
use crate::service::agent_manager::aggregate_disk_io;
use crate::service::auth::AuthService;
use crate::service::server::ServerService;
use crate::state::AppState;
use serverbee_common::constants::MAX_WS_MESSAGE_SIZE;
use serverbee_common::protocol::{
    BrowserClientMessage, BrowserMessage, RecoveryJobDto, RecoveryJobStage, RecoveryJobStatus,
    ServerMessage,
};
use serverbee_common::types::ServerStatus;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ws/servers", get(browser_ws_handler))
}

async fn browser_ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Validate auth: try session cookie first, then API key, then Bearer token
    let auth = validate_browser_auth(&state, &headers).await;
    match auth {
        Some((_user_id, is_admin, mobile_expires)) => ws
            .max_message_size(MAX_WS_MESSAGE_SIZE)
            .on_upgrade(move |socket| handle_browser_ws(socket, state, is_admin, mobile_expires)),
        None => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// Returns `Some((user_id, mobile_expires))` on success.
/// `mobile_expires` is `Some(expires_at)` when authenticated via a non-web session
/// (Bearer token from mobile), so the WS connection can be auto-closed on expiry.
/// For web sessions and API keys, `mobile_expires` is `None` (they use sliding expiry
/// or never expire respectively).
async fn validate_browser_auth(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Option<(String, bool, Option<chrono::DateTime<chrono::Utc>>)> {
    // Try session cookie (always web source → no mobile expiry)
    if let Some(token) = extract_session_cookie(headers)
        && let Ok(Some((user, _session))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
    {
        return Some((user.id, user.role == "admin", None));
    }

    // Try API key header (no expiry)
    if let Some(key) = extract_api_key(headers)
        && let Ok(Some(user)) = AuthService::validate_api_key(&state.db, &key).await
    {
        return Some((user.id, user.role == "admin", None));
    }

    // Try Bearer token (may be a mobile session with a fixed expiry)
    if let Some(token) = extract_bearer_token(headers)
        && let Ok(Some((user, session))) =
            AuthService::validate_session(&state.db, &token, state.config.auth.session_ttl).await
    {
        let mobile_expires = if session.source != "web" {
            Some(session.expires_at)
        } else {
            None
        };
        return Some((user.id, user.role == "admin", mobile_expires));
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

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}

async fn handle_browser_ws(
    socket: WebSocket,
    state: Arc<AppState>,
    is_admin: bool,
    mobile_expires: Option<chrono::DateTime<chrono::Utc>>,
) {
    let (mut ws_sink, mut ws_stream) = socket.split();

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
            // Mobile token expiry: auto-close when the token expires
            _ = async {
                if let Some(exp) = mobile_expires {
                    let dur = (exp - chrono::Utc::now()).to_std().unwrap_or_default();
                    tokio::time::sleep(dur).await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                tracing::debug!("Mobile WS token expired, closing connection");
                let _ = ws_sink.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 4001,
                    reason: "token expired".into(),
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

async fn build_full_sync(state: &Arc<AppState>, is_admin: bool) -> BrowserMessage {
    let recoveries = if is_admin {
        recovery_snapshot(state).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let servers = match ServerService::list_servers(&state.db).await {
        Ok(servers) => servers,
        Err(e) => {
            tracing::error!("Failed to list servers for FullSync: {e}");
            return BrowserMessage::FullSync {
                servers: Vec::new(),
                upgrades: state.upgrade_tracker.snapshot(),
                recoveries,
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
        tags_by_server.entry(row.server_id).or_default().push(row.tag);
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
            }
        })
        .collect();

    BrowserMessage::FullSync {
        servers: statuses,
        upgrades: state.upgrade_tracker.snapshot(),
        recoveries,
    }
}

pub(crate) async fn recovery_snapshot(state: &Arc<AppState>) -> Option<Vec<RecoveryJobDto>> {
    match recovery_job::Entity::find().all(&state.db).await {
        Ok(jobs) => Some(jobs.into_iter().map(Into::into).collect()),
        Err(e) => {
            tracing::error!("Failed to list recovery jobs for browser sync: {e}");
            None
        }
    }
}

pub(crate) async fn broadcast_recovery_update(state: &Arc<AppState>) {
    let Some(recoveries) = recovery_snapshot(state).await else {
        return;
    };
    let _ = state.browser_tx.send(BrowserMessage::Update {
        servers: Vec::new(),
        recoveries: Some(recoveries),
    });
}

fn filter_browser_message(msg: BrowserMessage, is_admin: bool) -> Option<BrowserMessage> {
    if is_admin {
        return Some(msg);
    }

    match msg {
        BrowserMessage::FullSync {
            servers,
            upgrades,
            ..
        } => Some(BrowserMessage::FullSync {
            servers,
            upgrades,
            recoveries: Vec::new(),
        }),
        BrowserMessage::Update { servers, .. } => Some(BrowserMessage::Update {
            servers,
            recoveries: None,
        }),
        other => Some(other),
    }
}

async fn send_browser_message(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &BrowserMessage,
) -> Result<(), axum::Error> {
    let text = serde_json::to_string(msg).map_err(axum::Error::new)?;
    sink.send(Message::Text(text.into())).await
}

impl From<recovery_job::Model> for RecoveryJobDto {
    fn from(value: recovery_job::Model) -> Self {
        Self {
            job_id: value.job_id,
            target_server_id: value.target_server_id,
            source_server_id: value.source_server_id,
            status: recovery_job_status_from_str(&value.status),
            stage: recovery_job_stage_from_str(&value.stage),
            error: value.error,
            started_at: value.started_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
            last_heartbeat_at: value.last_heartbeat_at,
        }
    }
}

fn recovery_job_status_from_str(value: &str) -> RecoveryJobStatus {
    match value {
        "running" => RecoveryJobStatus::Running,
        "failed" => RecoveryJobStatus::Failed,
        "succeeded" => RecoveryJobStatus::Succeeded,
        _ => RecoveryJobStatus::Unknown,
    }
}

fn recovery_job_stage_from_str(value: &str) -> RecoveryJobStage {
    match value {
        "validating" => RecoveryJobStage::Validating,
        "rebinding" => RecoveryJobStage::Rebinding,
        "awaiting_target_online" => RecoveryJobStage::AwaitingTargetOnline,
        "freezing_writes" => RecoveryJobStage::FreezingWrites,
        "merging_history" => RecoveryJobStage::MergingHistory,
        "finalizing" => RecoveryJobStage::Finalizing,
        "succeeded" => RecoveryJobStage::Succeeded,
        "failed" => RecoveryJobStage::Failed,
        _ => RecoveryJobStage::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::server;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};
    use serverbee_common::constants::CAP_DEFAULT;

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

    #[tokio::test]
    async fn full_sync_includes_running_recoveries() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let now = Utc::now();
        recovery_job::ActiveModel {
            job_id: Set("job-1".to_string()),
            target_server_id: Set("target-1".to_string()),
            source_server_id: Set("source-1".to_string()),
            status: Set("running".to_string()),
            stage: Set("rebinding".to_string()),
            checkpoint_json: Set(None),
            error: Set(None),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(Some(now)),
        }
        .insert(&db)
        .await
        .unwrap();

        let message = build_full_sync(&state, true).await;

        match message {
            BrowserMessage::FullSync { recoveries, .. } => {
                assert_eq!(recoveries.len(), 1);
                assert_eq!(recoveries[0].job_id, "job-1");
                assert_eq!(recoveries[0].stage, RecoveryJobStage::Rebinding);
                assert_eq!(recoveries[0].status, RecoveryJobStatus::Running);
            }
            other => panic!("expected full sync, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn full_sync_includes_terminal_recovery_states() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let now = Utc::now();
        recovery_job::ActiveModel {
            job_id: Set("job-failed".to_string()),
            target_server_id: Set("target-1".to_string()),
            source_server_id: Set("source-1".to_string()),
            status: Set("failed".to_string()),
            stage: Set("failed".to_string()),
            checkpoint_json: Set(None),
            error: Set(Some("boom".to_string())),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(Some(now)),
        }
        .insert(&db)
        .await
        .unwrap();
        recovery_job::ActiveModel {
            job_id: Set("job-succeeded".to_string()),
            target_server_id: Set("target-1".to_string()),
            source_server_id: Set("source-1".to_string()),
            status: Set("succeeded".to_string()),
            stage: Set("succeeded".to_string()),
            checkpoint_json: Set(None),
            error: Set(None),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(Some(now)),
        }
        .insert(&db)
        .await
        .unwrap();

        let message = build_full_sync(&state, true).await;

        match message {
            BrowserMessage::FullSync { recoveries, .. } => {
                assert_eq!(recoveries.len(), 2);
                assert!(recoveries.iter().any(|job| {
                    job.job_id == "job-failed"
                        && job.status == RecoveryJobStatus::Failed
                        && job.stage == RecoveryJobStage::Failed
                }));
                assert!(recoveries.iter().any(|job| {
                    job.job_id == "job-succeeded"
                        && job.status == RecoveryJobStatus::Succeeded
                        && job.stage == RecoveryJobStage::Succeeded
                }));
            }
            other => panic!("expected full sync, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn full_sync_hides_recoveries_for_non_admin() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let now = Utc::now();
        recovery_job::ActiveModel {
            job_id: Set("job-1".to_string()),
            target_server_id: Set("target-1".to_string()),
            source_server_id: Set("source-1".to_string()),
            status: Set("running".to_string()),
            stage: Set("rebinding".to_string()),
            checkpoint_json: Set(None),
            error: Set(None),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(Some(now)),
        }
        .insert(&db)
        .await
        .unwrap();

        let message = build_full_sync(&state, false).await;

        match message {
            BrowserMessage::FullSync { recoveries, .. } => assert!(recoveries.is_empty()),
            other => panic!("expected full sync, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn broadcast_recovery_update_includes_terminal_recovery_states() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();
        let mut browser_rx = state.browser_tx.subscribe();

        let now = Utc::now();
        recovery_job::ActiveModel {
            job_id: Set("job-failed".to_string()),
            target_server_id: Set("target-1".to_string()),
            source_server_id: Set("source-1".to_string()),
            status: Set("failed".to_string()),
            stage: Set("failed".to_string()),
            checkpoint_json: Set(None),
            error: Set(Some("boom".to_string())),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(Some(now)),
        }
        .insert(&db)
        .await
        .unwrap();

        broadcast_recovery_update(&state).await;

        let message = browser_rx.recv().await.unwrap();
        match message {
            BrowserMessage::Update {
                recoveries: Some(recoveries),
                ..
            } => {
                assert_eq!(recoveries.len(), 1);
                assert_eq!(recoveries[0].job_id, "job-failed");
                assert_eq!(recoveries[0].status, RecoveryJobStatus::Failed);
                assert_eq!(recoveries[0].stage, RecoveryJobStage::Failed);
            }
            other => panic!("expected update with recoveries, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn full_sync_strips_recoveries_for_non_admin() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "target-1", "Target").await;
        insert_server(&db, "source-1", "Source").await;
        let state = AppState::new(db.clone(), AppConfig::default())
            .await
            .unwrap();

        let now = Utc::now();
        recovery_job::ActiveModel {
            job_id: Set("job-1".to_string()),
            target_server_id: Set("target-1".to_string()),
            source_server_id: Set("source-1".to_string()),
            status: Set("running".to_string()),
            stage: Set("rebinding".to_string()),
            checkpoint_json: Set(None),
            error: Set(None),
            started_at: Set(now),
            created_at: Set(now),
            updated_at: Set(now),
            last_heartbeat_at: Set(Some(now)),
        }
        .insert(&db)
        .await
        .unwrap();

        let message = build_full_sync(&state, false).await;

        match message {
            BrowserMessage::FullSync { recoveries, .. } => assert!(recoveries.is_empty()),
            other => panic!("expected full sync, got {other:?}"),
        }
    }
}
