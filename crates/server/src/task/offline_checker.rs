use std::sync::Arc;
use std::time::Duration;

use crate::state::AppState;

/// Periodically checks for agents that have stopped reporting and marks them offline.
/// Runs every 10 seconds with a 30-second threshold.
/// Also cleans up expired pending requests (older than 60s).
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        let offline_candidates = state.agent_manager.stale_connection_candidates(30);
        for (server_id, connection_id) in offline_candidates {
            let server_lock = state.agent_manager.server_cleanup_lock(&server_id);
            let _guard = server_lock.lock().await;

            if state
                .agent_manager
                .remove_connection_if_current(&server_id, connection_id)
            {
                tracing::info!("Agent {server_id} marked offline (no report for 30s)");
                crate::service::agent_manager::cleanup_disconnected_docker_state(
                    &state, &server_id,
                )
                .await;
            }
        }

        // Clean up expired pending requests (HTTP→WS relay) based on per-entry TTL
        state.agent_manager.cleanup_expired_requests();

        // Clean up expired traceroute results (older than 120s)
        state.agent_manager.cleanup_traceroute_results();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::server;
    use crate::service::auth::AuthService;
    use crate::service::server::ServerService;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};
    use serverbee_common::constants::CAP_DEFAULT;
    use serverbee_common::protocol::BrowserMessage;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::sync::{broadcast, mpsc};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }

    async fn insert_test_server(db: &sea_orm::DatabaseConnection, id: &str, name: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash_password should succeed");
        let now = Utc::now();
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
        .expect("insert test server should succeed");
    }

    async fn register_connection_for_test(
        state: Arc<AppState>,
        server_id: &str,
        server_name: &str,
        tx: mpsc::Sender<serverbee_common::protocol::ServerMessage>,
    ) {
        let server_lock = state.agent_manager.server_cleanup_lock(server_id);
        let _guard = server_lock.lock().await;
        state.agent_manager.add_connection(
            server_id.to_string(),
            server_name.to_string(),
            tx,
            test_addr(),
        );
    }

    #[tokio::test]
    async fn cleanup_docker_waits_for_serialized_reconnect_and_noops() {
        let (db, _tmp) = setup_test_db().await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();
        let (tx1, _) = mpsc::channel(1);
        let (tx2, _) = mpsc::channel(1);

        state
            .agent_manager
            .add_connection("s1".into(), "Srv".into(), tx1, test_addr());
        state
            .agent_manager
            .update_features("s1", vec!["docker".into(), "process".into()]);
        state.docker_viewers.add_viewer("s1", "conn1");

        let stale_candidates = state.agent_manager.stale_connection_candidates(0);
        assert_eq!(stale_candidates.len(), 1);
        let (candidate_server_id, candidate_connection_id) = stale_candidates[0].clone();
        assert_eq!(candidate_server_id, "s1");

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
        drop(held_guard);

        reconnect_task.await.unwrap();

        let server_lock = state.agent_manager.server_cleanup_lock("s1");
        let _guard = server_lock.lock().await;
        if state
            .agent_manager
            .remove_connection_if_current(&candidate_server_id, candidate_connection_id)
        {
            crate::service::agent_manager::cleanup_disconnected_docker_state(&state, "s1").await;
        }

        assert!(matches!(
            rx.try_recv(),
            Ok(BrowserMessage::ServerOnline { server_id }) if server_id == "s1"
        ));
        assert!(matches!(
            rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert!(state.docker_viewers.has_viewers("s1"));
        assert!(state.agent_manager.has_feature("s1", "docker"));
        assert!(state.agent_manager.is_online("s1"));
    }

    #[tokio::test]
    async fn cleanup_disconnected_docker_state_removes_runtime_and_persisted_docker_state() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "s1", "Srv").await;
        let state = AppState::new(db, AppConfig::default()).await.unwrap();
        state
            .agent_manager
            .update_features("s1", vec!["docker".into(), "process".into()]);
        state.docker_viewers.add_viewer("s1", "conn1");
        let mut rx = state.browser_tx.subscribe();

        crate::service::agent_manager::cleanup_disconnected_docker_state(&state, "s1").await;

        let server = ServerService::get_server(&state.db, "s1").await.unwrap();
        let persisted_features: Vec<String> = serde_json::from_str(&server.features).unwrap();

        assert!(!state.docker_viewers.has_viewers("s1"));
        assert_eq!(
            state.agent_manager.get_features("s1"),
            vec!["process".to_string()]
        );
        assert_eq!(persisted_features, vec!["process".to_string()]);
        assert!(matches!(
            rx.try_recv(),
            Ok(BrowserMessage::DockerAvailabilityChanged { server_id, available })
            if server_id == "s1" && !available
        ));
    }
}
