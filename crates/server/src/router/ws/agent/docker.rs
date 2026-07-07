//! Docker message handling: cache updates, browser fan-out, log-session
//! relay, and availability transitions (including `FeaturesUpdate`).

use std::sync::Arc;

use crate::state::AppState;
use serverbee_common::docker_types::{DockerContainer, DockerContainerStats, DockerSystemInfo};
use serverbee_common::protocol::{AgentMessage, BrowserMessage};

pub(super) fn on_docker_info(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: Option<&String>,
    info: &DockerSystemInfo,
    msg: &AgentMessage,
) {
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

pub(super) fn on_docker_containers(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: Option<&String>,
    containers: &[DockerContainer],
    msg: &AgentMessage,
) {
    state
        .agent_manager
        .update_docker_containers(server_id, containers.to_vec());
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
            containers: containers.to_vec(),
            stats,
        });
}

pub(super) fn on_docker_stats(
    state: &Arc<AppState>,
    server_id: &str,
    stats: &[DockerContainerStats],
) {
    state
        .agent_manager
        .update_docker_stats(server_id, stats.to_vec());
    if let Some(containers) = state.agent_manager.get_docker_containers(server_id) {
        state
            .agent_manager
            .broadcast_browser(BrowserMessage::DockerUpdate {
                server_id: server_id.to_string(),
                containers,
                stats: Some(stats.to_vec()),
            });
    }
}

pub(super) async fn on_docker_event(
    state: &Arc<AppState>,
    server_id: &str,
    event: serverbee_common::docker_types::DockerEventInfo,
) {
    let _ = crate::service::docker::DockerService::save_event(&state.db, server_id, &event).await;
    state
        .agent_manager
        .broadcast_browser(BrowserMessage::DockerEvent {
            server_id: server_id.to_string(),
            event,
        });
}

pub(super) async fn on_docker_unavailable(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: Option<&String>,
    msg: &AgentMessage,
) {
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

    if let Some(msg_id) = msg_id {
        state
            .agent_manager
            .dispatch_pending_response(msg_id, msg.clone());
    }
}

pub(super) async fn on_features_update(state: &Arc<AppState>, server_id: &str, features: &[String]) {
    let _ =
        crate::service::server::ServerService::update_features(&state.db, server_id, features)
            .await;
    state
        .agent_manager
        .update_features(server_id, features.to_vec());
    let docker_available = features.contains(&"docker".to_string());
    state
        .agent_manager
        .broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
            server_id: server_id.to_string(),
            available: docker_available,
        });
}
