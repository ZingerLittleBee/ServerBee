pub mod containers;
pub mod events;
pub mod logs;
pub mod networks;
pub mod volumes;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use bollard::Docker;
use serverbee_common::constants::{CAP_DOCKER, has_capability};
use serverbee_common::docker_types::DockerAction;
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Interval;

pub struct DockerManager {
    docker: Docker,
    agent_tx: mpsc::Sender<AgentMessage>,
    capabilities: Arc<AtomicU32>,
    stats_interval: Option<Interval>,
    log_sessions: HashMap<String, JoinHandle<()>>,
    event_stream_handle: Option<JoinHandle<()>>,
    /// Container IDs from the most recent listing, used for stats polling.
    running_container_ids: Vec<String>,
}

impl DockerManager {
    /// Attempt to connect to the local Docker daemon.
    pub fn try_new(
        agent_tx: mpsc::Sender<AgentMessage>,
        capabilities: Arc<AtomicU32>,
    ) -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            agent_tx,
            capabilities,
            stats_interval: None,
            log_sessions: HashMap::new(),
            event_stream_handle: None,
            running_container_ids: Vec::new(),
        })
    }

    /// Verify the Docker connection by pinging the daemon.
    pub async fn verify_connection(&self) -> anyhow::Result<()> {
        self.docker.ping().await?;
        Ok(())
    }

    /// Abort all background tasks (log sessions, event stream).
    pub fn cleanup(&mut self) {
        for (session_id, handle) in self.log_sessions.drain() {
            tracing::debug!("Aborting log session {session_id}");
            handle.abort();
        }
        if let Some(handle) = self.event_stream_handle.take() {
            tracing::debug!("Aborting event stream");
            handle.abort();
        }
        self.stats_interval = None;
    }

    /// Get Docker system information.
    pub async fn get_system_info(
        &self,
    ) -> anyhow::Result<serverbee_common::docker_types::DockerSystemInfo> {
        let version = self.docker.version().await?;
        let info = self.docker.info().await?;

        Ok(serverbee_common::docker_types::DockerSystemInfo {
            docker_version: version.version.unwrap_or_default(),
            api_version: version.api_version.unwrap_or_default(),
            os: version.os.unwrap_or_default(),
            arch: version.arch.unwrap_or_default(),
            containers_running: info.containers_running.unwrap_or(0),
            containers_paused: info.containers_paused.unwrap_or(0),
            containers_stopped: info.containers_stopped.unwrap_or(0),
            images: info.images.unwrap_or(0),
            memory_total: info.mem_total.unwrap_or(0) as u64,
        })
    }

    /// Check whether the Docker capability is currently enabled.
    fn is_capable(&self) -> bool {
        let caps = self.capabilities.load(Ordering::SeqCst);
        has_capability(caps, CAP_DOCKER)
    }

    /// Dispatch a Docker-related server message.
    pub async fn handle_server_message(&mut self, msg: ServerMessage) -> anyhow::Result<()> {
        if !self.is_capable() {
            tracing::warn!("Docker capability disabled, ignoring message");
            return Ok(());
        }

        match msg {
            ServerMessage::DockerListContainers { msg_id } => {
                self.handle_list_containers(Some(msg_id)).await?;
            }
            ServerMessage::DockerStartStats { interval_secs } => {
                self.handle_start_stats(interval_secs);
            }
            ServerMessage::DockerStopStats => {
                self.handle_stop_stats();
            }
            ServerMessage::DockerLogsStart {
                session_id,
                container_id,
                tail,
                follow,
            } => {
                self.handle_logs_start(session_id, container_id, tail, follow);
            }
            ServerMessage::DockerLogsStop { session_id } => {
                self.handle_logs_stop(&session_id);
            }
            ServerMessage::DockerEventsStart => {
                self.handle_events_start();
            }
            ServerMessage::DockerEventsStop => {
                self.handle_events_stop();
            }
            ServerMessage::DockerContainerAction {
                msg_id,
                container_id,
                action,
            } => {
                self.handle_container_action(msg_id, container_id, action)
                    .await;
            }
            ServerMessage::DockerGetInfo { msg_id } => {
                self.handle_get_info(msg_id).await?;
            }
            ServerMessage::DockerListNetworks { msg_id } => {
                self.handle_list_networks(msg_id).await?;
            }
            ServerMessage::DockerListVolumes { msg_id } => {
                self.handle_list_volumes(msg_id).await?;
            }
            _ => {
                tracing::debug!("DockerManager received non-docker message, ignoring");
            }
        }

        Ok(())
    }

    /// Called periodically when stats polling is active.
    pub async fn poll_stats(&mut self) -> anyhow::Result<()> {
        if !self.is_capable() {
            return Ok(());
        }

        // Refresh the container list and send it to the server
        let container_list = match containers::list_containers(&self.docker).await {
            Ok(list) => list,
            Err(e) => {
                self.notify_unavailable(None).await;
                tracing::warn!("Failed to list containers for stats: {e}");
                return Err(e);
            }
        };

        self.running_container_ids = container_list
            .iter()
            .filter(|c| c.state == "running")
            .map(|c| c.id.clone())
            .collect();

        // Send container list so the server always has fresh data
        let containers_msg = AgentMessage::DockerContainers {
            msg_id: None,
            containers: container_list,
        };
        let _ = self.agent_tx.send(containers_msg).await;

        if self.running_container_ids.is_empty() {
            let msg = AgentMessage::DockerStats { stats: vec![] };
            let _ = self.agent_tx.send(msg).await;
            return Ok(());
        }

        let stats =
            containers::get_container_stats(&self.docker, &self.running_container_ids).await;
        let msg = AgentMessage::DockerStats { stats };
        if self.agent_tx.send(msg).await.is_err() {
            tracing::debug!("Agent channel closed while sending stats");
        }
        Ok(())
    }

    // --- Private handlers ---

    async fn handle_list_containers(&self, msg_id: Option<String>) -> anyhow::Result<()> {
        match containers::list_containers(&self.docker).await {
            Ok(container_list) => {
                let msg = AgentMessage::DockerContainers {
                    msg_id,
                    containers: container_list,
                };
                let _ = self.agent_tx.send(msg).await;
                Ok(())
            }
            Err(e) => {
                self.notify_unavailable(msg_id).await;
                tracing::error!("Failed to list containers: {e}");
                Err(e)
            }
        }
    }

    fn handle_start_stats(&mut self, interval_secs: u32) {
        let secs = interval_secs.max(1);
        tracing::info!("Starting Docker stats polling every {secs}s");
        let mut interval = tokio::time::interval(Duration::from_secs(secs as u64));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        self.stats_interval = Some(interval);
    }

    fn handle_stop_stats(&mut self) {
        tracing::info!("Stopping Docker stats polling");
        self.stats_interval = None;
    }

    fn handle_logs_start(
        &mut self,
        session_id: String,
        container_id: String,
        tail: Option<u64>,
        follow: bool,
    ) {
        // Abort existing session with the same ID if present
        if let Some(handle) = self.log_sessions.remove(&session_id) {
            handle.abort();
        }

        tracing::info!(
            "Starting log session {session_id} for container {container_id} (tail={tail:?}, follow={follow})"
        );

        let handle = logs::spawn_log_session(
            self.docker.clone(),
            session_id.clone(),
            container_id,
            tail,
            follow,
            self.agent_tx.clone(),
        );

        self.log_sessions.insert(session_id, handle);
    }

    fn handle_logs_stop(&mut self, session_id: &str) {
        if let Some(handle) = self.log_sessions.remove(session_id) {
            tracing::info!("Stopping log session {session_id}");
            handle.abort();
        } else {
            tracing::debug!("Log session {session_id} not found");
        }
    }

    fn handle_events_start(&mut self) {
        // Abort existing event stream if present
        if let Some(handle) = self.event_stream_handle.take() {
            handle.abort();
        }

        tracing::info!("Starting Docker event stream");
        let handle = events::spawn_event_stream(self.docker.clone(), self.agent_tx.clone());
        self.event_stream_handle = Some(handle);
    }

    fn handle_events_stop(&mut self) {
        if let Some(handle) = self.event_stream_handle.take() {
            tracing::info!("Stopping Docker event stream");
            handle.abort();
        }
    }

    async fn handle_container_action(
        &self,
        msg_id: String,
        container_id: String,
        action: DockerAction,
    ) {
        let result = self.execute_container_action(&container_id, &action).await;

        let msg = match result {
            Ok(()) => AgentMessage::DockerActionResult {
                msg_id,
                success: true,
                error: None,
            },
            Err(e) => AgentMessage::DockerActionResult {
                msg_id,
                success: false,
                error: Some(e.to_string()),
            },
        };

        let _ = self.agent_tx.send(msg).await;
    }

    async fn execute_container_action(
        &self,
        container_id: &str,
        action: &DockerAction,
    ) -> anyhow::Result<()> {
        match action {
            DockerAction::Start => {
                tracing::info!("Starting container {container_id}");
                self.docker
                    .start_container::<String>(container_id, None)
                    .await?;
            }
            DockerAction::Stop { timeout } => {
                tracing::info!("Stopping container {container_id} (timeout={timeout:?})");
                let options = bollard::container::StopContainerOptions {
                    t: timeout.unwrap_or(10),
                };
                self.docker
                    .stop_container(container_id, Some(options))
                    .await?;
            }
            DockerAction::Restart { timeout } => {
                tracing::info!("Restarting container {container_id} (timeout={timeout:?})");
                let options = bollard::container::RestartContainerOptions {
                    t: timeout.unwrap_or(10) as isize,
                };
                self.docker
                    .restart_container(container_id, Some(options))
                    .await?;
            }
            DockerAction::Remove { force } => {
                tracing::info!("Removing container {container_id} (force={force})");
                let options = bollard::container::RemoveContainerOptions {
                    force: *force,
                    ..Default::default()
                };
                self.docker
                    .remove_container(container_id, Some(options))
                    .await?;
            }
        }
        Ok(())
    }

    async fn handle_get_info(&self, msg_id: String) -> anyhow::Result<()> {
        match self.get_system_info().await {
            Ok(info) => {
                let msg = AgentMessage::DockerInfo {
                    msg_id: Some(msg_id),
                    info,
                };
                let _ = self.agent_tx.send(msg).await;
                Ok(())
            }
            Err(e) => {
                self.notify_unavailable(Some(msg_id)).await;
                tracing::error!("Failed to get Docker info: {e}");
                Err(e)
            }
        }
    }

    async fn handle_list_networks(&self, msg_id: String) -> anyhow::Result<()> {
        match networks::list_networks(&self.docker).await {
            Ok(network_list) => {
                let msg = AgentMessage::DockerNetworks {
                    msg_id,
                    networks: network_list,
                };
                let _ = self.agent_tx.send(msg).await;
                Ok(())
            }
            Err(e) => {
                self.notify_unavailable(Some(msg_id)).await;
                tracing::error!("Failed to list networks: {e}");
                Err(e)
            }
        }
    }

    async fn handle_list_volumes(&self, msg_id: String) -> anyhow::Result<()> {
        match volumes::list_volumes(&self.docker).await {
            Ok(volume_list) => {
                let msg = AgentMessage::DockerVolumes {
                    msg_id,
                    volumes: volume_list,
                };
                let _ = self.agent_tx.send(msg).await;
                Ok(())
            }
            Err(e) => {
                self.notify_unavailable(Some(msg_id)).await;
                tracing::error!("Failed to list volumes: {e}");
                Err(e)
            }
        }
    }

    async fn notify_unavailable(&self, msg_id: Option<String>) {
        let _ = self
            .agent_tx
            .send(AgentMessage::DockerUnavailable { msg_id })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::{CAP_DEFAULT, CAP_TERMINAL};

    /// Build a DockerManager pointing at a bogus HTTP endpoint.
    ///
    /// `Docker::connect_with_http` only constructs the transport; it performs no
    /// network I/O and never touches a daemon socket, so this is deterministic and
    /// cross-platform. The returned manager is only used to exercise in-memory state
    /// handlers — no method that actually hits the daemon is called.
    fn make_manager(caps: u32) -> (DockerManager, mpsc::Receiver<AgentMessage>) {
        let docker = bollard::Docker::connect_with_http(
            "http://127.0.0.1:1",
            1,
            bollard::API_DEFAULT_VERSION,
        )
        .expect("connect_with_http never connects, only builds a client");
        let (tx, rx) = mpsc::channel(16);
        let manager = DockerManager {
            docker,
            agent_tx: tx,
            capabilities: Arc::new(AtomicU32::new(caps)),
            stats_interval: None,
            log_sessions: HashMap::new(),
            event_stream_handle: None,
            running_container_ids: Vec::new(),
        };
        (manager, rx)
    }

    #[test]
    fn is_capable_true_when_docker_bit_set() {
        // CAP_DOCKER bit present -> capability check passes
        let (manager, _rx) = make_manager(CAP_DOCKER);
        assert!(manager.is_capable());
    }

    #[test]
    fn is_capable_false_when_no_caps() {
        // No capability bits set -> docker is not capable
        let (manager, _rx) = make_manager(0);
        assert!(!manager.is_capable());
    }

    #[test]
    fn is_capable_false_when_only_unrelated_caps() {
        // A different capability set (terminal only) does not enable docker
        let (manager, _rx) = make_manager(CAP_TERMINAL);
        assert!(!manager.is_capable());
    }

    #[test]
    fn is_capable_false_when_default_caps_excludes_docker() {
        // CAP_DEFAULT does not include docker, so the docker manager stays disabled
        let (manager, _rx) = make_manager(CAP_DEFAULT);
        assert!(!manager.is_capable());
    }

    #[test]
    fn is_capable_reflects_atomic_updates() {
        // Toggling the shared atomic flips the capability result without rebuilding
        let (manager, _rx) = make_manager(0);
        assert!(!manager.is_capable());
        manager.capabilities.store(CAP_DOCKER, Ordering::SeqCst);
        assert!(manager.is_capable());
    }

    #[tokio::test]
    async fn start_stats_sets_interval() {
        // Starting stats with a positive interval arms the polling timer
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        assert!(manager.stats_interval.is_none());
        manager.handle_start_stats(5);
        assert!(manager.stats_interval.is_some());
    }

    #[tokio::test]
    async fn start_stats_clamps_zero_interval_to_one() {
        // An interval of 0 is clamped to >= 1s (still produces a valid interval)
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        manager.handle_start_stats(0);
        assert!(manager.stats_interval.is_some());
    }

    #[tokio::test]
    async fn stop_stats_clears_interval() {
        // Stopping stats disarms a previously-armed polling timer
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        manager.handle_start_stats(2);
        assert!(manager.stats_interval.is_some());
        manager.handle_stop_stats();
        assert!(manager.stats_interval.is_none());
    }

    #[tokio::test]
    async fn logs_stop_aborts_known_session() {
        // Stopping a tracked log session removes it from the session map
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        let handle = tokio::spawn(async { std::future::pending::<()>().await });
        manager.log_sessions.insert("sess-1".to_string(), handle);
        assert_eq!(manager.log_sessions.len(), 1);
        manager.handle_logs_stop("sess-1");
        assert!(manager.log_sessions.is_empty());
    }

    #[tokio::test]
    async fn logs_stop_is_noop_for_unknown_session() {
        // Stopping an unknown session id leaves existing sessions untouched
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        let handle = tokio::spawn(async { std::future::pending::<()>().await });
        manager.log_sessions.insert("keep".to_string(), handle);
        manager.handle_logs_stop("missing");
        assert_eq!(manager.log_sessions.len(), 1);
        assert!(manager.log_sessions.contains_key("keep"));
    }

    #[tokio::test]
    async fn events_stop_clears_handle() {
        // Stopping the event stream clears the stored handle
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        let handle = tokio::spawn(async { std::future::pending::<()>().await });
        manager.event_stream_handle = Some(handle);
        manager.handle_events_stop();
        assert!(manager.event_stream_handle.is_none());
    }

    #[tokio::test]
    async fn events_stop_is_noop_when_no_stream() {
        // Stopping with no active stream is a safe no-op
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        assert!(manager.event_stream_handle.is_none());
        manager.handle_events_stop();
        assert!(manager.event_stream_handle.is_none());
    }

    #[tokio::test]
    async fn cleanup_drains_sessions_and_clears_handles() {
        // cleanup() drains all log sessions, the event handle, and the stats interval
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        manager
            .log_sessions
            .insert("a".to_string(), tokio::spawn(async { std::future::pending::<()>().await }));
        manager
            .log_sessions
            .insert("b".to_string(), tokio::spawn(async { std::future::pending::<()>().await }));
        manager.event_stream_handle = Some(tokio::spawn(async { std::future::pending::<()>().await }));
        manager.handle_start_stats(3);

        manager.cleanup();

        assert!(manager.log_sessions.is_empty());
        assert!(manager.event_stream_handle.is_none());
        assert!(manager.stats_interval.is_none());
    }

    #[tokio::test]
    async fn cleanup_is_idempotent_on_empty_state() {
        // Calling cleanup() on an already-empty manager does not panic and stays empty
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        manager.cleanup();
        assert!(manager.log_sessions.is_empty());
        assert!(manager.event_stream_handle.is_none());
        assert!(manager.stats_interval.is_none());
    }

    #[tokio::test]
    async fn handle_server_message_ignored_when_capability_disabled() {
        // With docker capability off, a stats-start message is dropped without arming the timer
        let (mut manager, _rx) = make_manager(0);
        manager
            .handle_server_message(ServerMessage::DockerStartStats { interval_secs: 5 })
            .await
            .expect("disabled capability returns Ok without daemon access");
        assert!(manager.stats_interval.is_none());
    }

    #[tokio::test]
    async fn handle_server_message_dispatches_start_stats_when_capable() {
        // A capable manager routes DockerStartStats to the in-memory interval handler
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        manager
            .handle_server_message(ServerMessage::DockerStartStats { interval_secs: 4 })
            .await
            .expect("start stats does not touch the daemon");
        assert!(manager.stats_interval.is_some());
    }

    #[tokio::test]
    async fn handle_server_message_dispatches_stop_stats_when_capable() {
        // DockerStopStats clears a previously-armed interval via the dispatcher
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        manager.handle_start_stats(2);
        manager
            .handle_server_message(ServerMessage::DockerStopStats)
            .await
            .expect("stop stats does not touch the daemon");
        assert!(manager.stats_interval.is_none());
    }

    #[tokio::test]
    async fn handle_server_message_dispatches_logs_stop_when_capable() {
        // DockerLogsStop routes through the dispatcher and removes the named session
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        manager
            .log_sessions
            .insert("s9".to_string(), tokio::spawn(async { std::future::pending::<()>().await }));
        manager
            .handle_server_message(ServerMessage::DockerLogsStop {
                session_id: "s9".to_string(),
            })
            .await
            .expect("logs stop does not touch the daemon");
        assert!(manager.log_sessions.is_empty());
    }

    #[tokio::test]
    async fn handle_server_message_ignores_non_docker_variant() {
        // A non-docker message hits the catch-all branch and changes no state
        let (mut manager, _rx) = make_manager(CAP_DOCKER);
        manager
            .handle_server_message(ServerMessage::Ack {
                msg_id: "x".to_string(),
            })
            .await
            .expect("unknown variant is ignored");
        assert!(manager.stats_interval.is_none());
        assert!(manager.log_sessions.is_empty());
        assert!(manager.event_stream_handle.is_none());
    }

    #[tokio::test]
    async fn notify_unavailable_sends_message_with_msg_id() {
        // notify_unavailable forwards a DockerUnavailable carrying the provided msg_id
        let (manager, mut rx) = make_manager(CAP_DOCKER);
        manager.notify_unavailable(Some("req-7".to_string())).await;
        match rx.recv().await {
            Some(AgentMessage::DockerUnavailable { msg_id }) => {
                assert_eq!(msg_id, Some("req-7".to_string()));
            }
            other => panic!("expected DockerUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn notify_unavailable_sends_message_with_none_id() {
        // notify_unavailable also handles the None msg_id branch (stats-poll path)
        let (manager, mut rx) = make_manager(CAP_DOCKER);
        manager.notify_unavailable(None).await;
        match rx.recv().await {
            Some(AgentMessage::DockerUnavailable { msg_id }) => assert!(msg_id.is_none()),
            other => panic!("expected DockerUnavailable, got {other:?}"),
        }
    }
}
