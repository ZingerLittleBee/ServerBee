pub mod containers;
pub mod events;
pub mod logs;
pub mod networks;
pub mod volumes;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bollard::Docker;
use serverbee_common::constants::{has_capability, CAP_DOCKER};
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
    pub async fn get_system_info(&self) -> anyhow::Result<serverbee_common::docker_types::DockerSystemInfo> {
        let version = self.docker.version().await?;
        let info = self.docker.info().await?;

        Ok(serverbee_common::docker_types::DockerSystemInfo {
            docker_version: version.version.unwrap_or_default(),
            api_version: version.api_version.unwrap_or_default(),
            os: version.os.unwrap_or_default(),
            arch: version.arch.unwrap_or_default(),
            containers_running: info.containers_running.unwrap_or(0) as i64,
            containers_paused: info.containers_paused.unwrap_or(0) as i64,
            containers_stopped: info.containers_stopped.unwrap_or(0) as i64,
            images: info.images.unwrap_or(0) as i64,
            memory_total: info.mem_total.unwrap_or(0) as u64,
        })
    }

    /// Check whether the Docker capability is currently enabled.
    fn is_capable(&self) -> bool {
        let caps = self.capabilities.load(Ordering::SeqCst);
        has_capability(caps, CAP_DOCKER)
    }

    /// Dispatch a Docker-related server message.
    pub async fn handle_server_message(&mut self, msg: ServerMessage) {
        if !self.is_capable() {
            tracing::warn!("Docker capability disabled, ignoring message");
            return;
        }

        match msg {
            ServerMessage::DockerListContainers { msg_id } => {
                self.handle_list_containers(Some(msg_id)).await;
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
                self.handle_get_info(msg_id).await;
            }
            ServerMessage::DockerListNetworks { msg_id } => {
                self.handle_list_networks(msg_id).await;
            }
            ServerMessage::DockerListVolumes { msg_id } => {
                self.handle_list_volumes(msg_id).await;
            }
            _ => {
                tracing::debug!("DockerManager received non-docker message, ignoring");
            }
        }
    }

    /// Called periodically when stats polling is active.
    pub async fn poll_stats(&mut self) {
        if !self.is_capable() {
            return;
        }

        // First, refresh the container list to get current running container IDs
        match containers::list_containers(&self.docker).await {
            Ok(container_list) => {
                self.running_container_ids = container_list
                    .iter()
                    .filter(|c| c.state == "running")
                    .map(|c| c.id.clone())
                    .collect();
            }
            Err(e) => {
                tracing::warn!("Failed to list containers for stats: {e}");
                return;
            }
        }

        if self.running_container_ids.is_empty() {
            let msg = AgentMessage::DockerStats { stats: vec![] };
            let _ = self.agent_tx.send(msg).await;
            return;
        }

        let stats =
            containers::get_container_stats(&self.docker, &self.running_container_ids).await;
        let msg = AgentMessage::DockerStats { stats };
        if self.agent_tx.send(msg).await.is_err() {
            tracing::debug!("Agent channel closed while sending stats");
        }
    }

    // --- Private handlers ---

    async fn handle_list_containers(&self, msg_id: Option<String>) {
        match containers::list_containers(&self.docker).await {
            Ok(container_list) => {
                let msg = AgentMessage::DockerContainers {
                    msg_id,
                    containers: container_list,
                };
                let _ = self.agent_tx.send(msg).await;
            }
            Err(e) => {
                tracing::error!("Failed to list containers: {e}");
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
        let handle =
            events::spawn_event_stream(self.docker.clone(), self.agent_tx.clone());
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
        let result = self
            .execute_container_action(&container_id, &action)
            .await;

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

    async fn handle_get_info(&self, msg_id: String) {
        match self.get_system_info().await {
            Ok(info) => {
                let msg = AgentMessage::DockerInfo {
                    msg_id: Some(msg_id),
                    info,
                };
                let _ = self.agent_tx.send(msg).await;
            }
            Err(e) => {
                tracing::error!("Failed to get Docker info: {e}");
            }
        }
    }

    async fn handle_list_networks(&self, msg_id: String) {
        match networks::list_networks(&self.docker).await {
            Ok(network_list) => {
                let msg = AgentMessage::DockerNetworks {
                    msg_id,
                    networks: network_list,
                };
                let _ = self.agent_tx.send(msg).await;
            }
            Err(e) => {
                tracing::error!("Failed to list networks: {e}");
            }
        }
    }

    async fn handle_list_volumes(&self, msg_id: String) {
        match volumes::list_volumes(&self.docker).await {
            Ok(volume_list) => {
                let msg = AgentMessage::DockerVolumes {
                    msg_id,
                    volumes: volume_list,
                };
                let _ = self.agent_tx.send(msg).await;
            }
            Err(e) => {
                tracing::error!("Failed to list volumes: {e}");
            }
        }
    }
}
