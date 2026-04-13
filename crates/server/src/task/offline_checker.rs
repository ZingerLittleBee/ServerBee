use std::sync::Arc;
use std::time::Duration;

use serverbee_common::protocol::BrowserMessage;

use crate::state::AppState;

/// Periodically checks for agents that have stopped reporting and marks them offline.
/// Runs every 10 seconds with a 30-second threshold.
/// Also cleans up expired pending requests (older than 60s).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        let offline_ids = state.agent_manager.check_offline(30);
        for id in &offline_ids {
            tracing::info!("Agent {id} marked offline (no report for 30s)");
            cleanup_docker_for_server(&state, id).await;
        }

        // Clean up expired pending requests (HTTP→WS relay) based on per-entry TTL
        state.agent_manager.cleanup_expired_requests();

        // Clean up expired traceroute results (older than 120s)
        state.agent_manager.cleanup_traceroute_results();
    }
}

async fn cleanup_docker_for_server(state: &AppState, server_id: &str) {
    let server_lock = state.agent_manager.server_cleanup_lock(server_id);
    let _guard = server_lock.lock().await;

    if state.agent_manager.is_online(server_id) {
        return;
    }

    let mut features = state.agent_manager.get_features(server_id);
    features.retain(|feature| feature != "docker");
    let _ = crate::service::server::ServerService::update_features(&state.db, server_id, &features)
        .await;

    state.docker_viewers.remove_all_for_server(server_id);
    state.agent_manager.update_features(server_id, features);

    state
        .agent_manager
        .broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
            server_id: server_id.to_string(),
            available: false,
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::test_utils::setup_test_db;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::sync::{broadcast, mpsc};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    async fn register_connection_for_test(
        state: Arc<AppState>,
        server_id: &str,
        server_name: &str,
        tx: mpsc::Sender<serverbee_common::protocol::ServerMessage>,
    ) {
        let server_lock = state.agent_manager.server_cleanup_lock(server_id);
        let _guard = server_lock.lock().await;
        state
            .agent_manager
            .add_connection(server_id.to_string(), server_name.to_string(), tx, test_addr());
    }

    #[tokio::test]
    async fn cleanup_docker_waits_for_serialized_reconnect_and_noops() {
        let (db, _tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();
        let (tx1, _) = mpsc::channel(1);
        let (tx2, _) = mpsc::channel(1);

        state.agent_manager.add_connection("s1".into(), "Srv".into(), tx1, test_addr());
        state
            .agent_manager
            .update_features("s1", vec!["docker".into(), "process".into()]);
        state.docker_viewers.add_viewer("s1", "conn1");

        let offline_ids = state.agent_manager.check_offline(0);
        assert_eq!(offline_ids, vec!["s1"]);

        let server_lock = state.agent_manager.server_cleanup_lock("s1");
        let held_guard = server_lock.lock().await;
        let mut rx = state.browser_tx.subscribe();

        let reconnect_state = Arc::clone(&state);
        let reconnect_task = tokio::spawn(async move {
            register_connection_for_test(reconnect_state, "s1", "Srv", tx2).await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(state.docker_viewers.has_viewers("s1"));
        assert!(state.agent_manager.has_feature("s1", "docker"));

        let cleanup_state = Arc::clone(&state);
        let cleanup_task = tokio::spawn(async move {
            cleanup_docker_for_server(&cleanup_state, "s1").await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(held_guard);

        reconnect_task.await.unwrap();
        cleanup_task.await.unwrap();

        assert!(matches!(
            rx.try_recv(),
            Ok(BrowserMessage::ServerOnline { server_id }) if server_id == "s1"
        ));
        assert!(matches!(rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
        assert!(state.docker_viewers.has_viewers("s1"));
        assert!(state.agent_manager.has_feature("s1", "docker"));
        assert!(state.agent_manager.is_online("s1"));
    }
}
