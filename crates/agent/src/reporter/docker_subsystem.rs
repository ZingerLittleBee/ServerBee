//! Docker availability state machine, connection-scoped.
//!
//! One WebSocket connection owns one [`DockerSubsystem`]: daemon detection,
//! the retry/demote lifecycle, stats-poll gating, and the unavailable replies
//! for Docker requests all live here. The shared runtime routes Docker frames
//! in and forwards the returned [`AgentMessage`]s out — nothing more. Methods
//! return the message to send instead of writing to the WebSocket sink, so
//! every availability transition is unit-testable without a Docker daemon.

use std::sync::Arc;
use std::time::Duration;

use serverbee_common::constants::CAP_DOCKER;
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use tokio::sync::{mpsc, watch};

use crate::capability_grants::CapabilityAuthority;
use crate::docker::DockerManager;

const DOCKER_RETRY_BACKOFF_SECS: &[u64] = &[30, 60, 120, 300, 900];

pub(super) enum DockerTick {
    PollStats,
    Retry,
}

pub(super) enum DockerBackgroundResult {
    Stats(anyhow::Result<()>),
    Retry(anyhow::Result<DockerManager>),
}

pub(super) enum DockerEvent {
    Tick(DockerTick),
    Background(DockerBackgroundResult),
}

fn delayed_interval(secs: u64) -> tokio::time::Interval {
    let period = Duration::from_secs(secs);
    let mut iv = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    iv
}

pub(super) struct DockerSubsystem {
    capabilities: Arc<CapabilityAuthority>,
    cap_rx: watch::Receiver<u32>,
    tx: mpsc::Sender<AgentMessage>,
    manager: Option<DockerManager>,
    available: bool,
    retry_backoff_idx: usize,
    stats_interval: Option<tokio::time::Interval>,
    retry_interval: tokio::time::Interval,
    pending_retry: Option<tokio::task::JoinHandle<anyhow::Result<DockerManager>>>,
    pending_stats: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
    #[cfg(test)]
    mock_retry_ok: Option<bool>,
}

impl DockerSubsystem {
    pub(super) fn new(
        tx: mpsc::Sender<AgentMessage>,
        capabilities: Arc<CapabilityAuthority>,
    ) -> Self {
        let cap_rx = capabilities.subscribe_state();
        let retry_interval = delayed_interval(DOCKER_RETRY_BACKOFF_SECS[0]);
        Self {
            capabilities,
            cap_rx,
            tx,
            manager: None,
            available: false,
            retry_backoff_idx: 0,
            stats_interval: None,
            retry_interval,
            pending_retry: None,
            pending_stats: None,
            #[cfg(test)]
            mock_retry_ok: None,
        }
    }

    pub(super) fn features(&self) -> Vec<String> {
        if self.available {
            vec!["docker".to_string()]
        } else {
            Vec::new()
        }
    }

    pub(super) async fn probe(&mut self) {
        if !self.capabilities.has(CAP_DOCKER) {
            return;
        }
        match DockerManager::try_new(self.tx.clone(), Arc::clone(&self.capabilities)) {
            Ok(dm) => match dm.verify_connection().await {
                Ok(()) => {
                    tracing::info!("Docker daemon connected");
                    self.available = true;
                    self.retry_backoff_idx = 0;
                    self.manager = Some(dm);
                }
                Err(e) => {
                    tracing::info!("Docker daemon not available: {e}");
                }
            },
            Err(e) => {
                tracing::info!("Docker not available: {e}");
            }
        }
    }

    pub(super) async fn next_event(&mut self) -> DockerEvent {
        let has_manager = self.manager.is_some();
        let has_cap = self.capabilities.has(CAP_DOCKER);
        tokio::select! {
            tick = async {
                if has_manager {
                    match self.stats_interval.as_mut() {
                        Some(iv) => {
                            iv.tick().await;
                            DockerTick::PollStats
                        }
                        None => std::future::pending().await,
                    }
                } else if !has_cap {
                    loop {
                        if self.cap_rx.changed().await.is_err() {
                            return std::future::pending().await;
                        }
                        if self.capabilities.has(CAP_DOCKER) {
                            self.retry_backoff_idx = 0;
                            self.retry_interval = delayed_interval(DOCKER_RETRY_BACKOFF_SECS[0]);
                            return DockerTick::Retry;
                        }
                    }
                } else {
                    self.retry_interval.tick().await;
                    DockerTick::Retry
                }
            } => DockerEvent::Tick(tick),
            joined = async {
                match self.pending_stats.as_mut() {
                    Some(h) => h.await,
                    None => std::future::pending().await,
                }
            } => {
                self.pending_stats = None;
                let res = match joined {
                    Ok(res) => DockerBackgroundResult::Stats(res),
                    Err(e) => DockerBackgroundResult::Stats(Err(anyhow::anyhow!(
                        "Docker stats task failed: {e}"
                    ))),
                };
                DockerEvent::Background(res)
            }
            joined = async {
                match self.pending_retry.as_mut() {
                    Some(h) => h.await,
                    None => std::future::pending().await,
                }
            } => {
                self.pending_retry = None;
                let res = match joined {
                    Ok(res) => DockerBackgroundResult::Retry(res),
                    Err(e) => DockerBackgroundResult::Retry(Err(anyhow::anyhow!(
                        "Docker retry task failed: {e}"
                    ))),
                };
                DockerEvent::Background(res)
            }
        }
    }

    pub(super) fn start_background_tick(&mut self, tick: DockerTick) {
        match tick {
            DockerTick::PollStats => {
                if self.pending_stats.is_some() {
                    return;
                }
                if let Some(dm) = self.manager.as_ref() {
                    self.pending_stats = Some(dm.spawn_poll_stats());
                }
            }
            DockerTick::Retry => {
                if !self.capabilities.has(CAP_DOCKER) || self.pending_retry.is_some() {
                    return;
                }
                let tx = self.tx.clone();
                let caps = Arc::clone(&self.capabilities);
                self.pending_retry = Some(tokio::spawn(async move {
                    let dm = DockerManager::try_new(tx, caps)?;
                    dm.verify_connection().await?;
                    Ok(dm)
                }));
            }
        }
    }

    pub(super) fn apply_background_result(
        &mut self,
        result: DockerBackgroundResult,
    ) -> Option<AgentMessage> {
        match result {
            DockerBackgroundResult::Stats(Ok(())) => None,
            DockerBackgroundResult::Stats(Err(e)) => {
                tracing::warn!("Docker stats polling failed: {e}");
                self.demote()
            }
            DockerBackgroundResult::Retry(Ok(dm)) => {
                tracing::info!("Docker daemon now available");
                self.manager = Some(dm);
                self.available = true;
                self.retry_backoff_idx = 0;
                Some(AgentMessage::FeaturesUpdate {
                    features: vec!["docker".to_string()],
                })
            }
            DockerBackgroundResult::Retry(Err(e)) => {
                tracing::debug!("Docker still not available: {e}");
                self.advance_retry_backoff();
                None
            }
        }
    }

    #[cfg(test)]
    pub(super) async fn poll_stats(&mut self) -> Option<AgentMessage> {
        if let Some(dm) = self.manager.as_mut()
            && let Err(e) = dm.poll_stats().await
        {
            tracing::warn!("Docker stats polling failed: {e}");
            return self.demote();
        }
        None
    }

    #[cfg(test)]
    pub(super) async fn retry(&mut self) -> Option<AgentMessage> {
        if let Some(ok) = self.mock_retry_ok {
            if ok {
                self.available = true;
                self.retry_backoff_idx = 0;
                return Some(AgentMessage::FeaturesUpdate {
                    features: vec!["docker".to_string()],
                });
            }
            self.advance_retry_backoff();
            return None;
        }

        if !self.capabilities.has(CAP_DOCKER) {
            return None;
        }

        tracing::debug!("Retrying Docker connection...");
        match DockerManager::try_new(self.tx.clone(), Arc::clone(&self.capabilities)) {
            Ok(dm) => match dm.verify_connection().await {
                Ok(()) => {
                    tracing::info!("Docker daemon now available");
                    self.manager = Some(dm);
                    self.available = true;
                    self.retry_backoff_idx = 0;
                    return Some(AgentMessage::FeaturesUpdate {
                        features: vec!["docker".to_string()],
                    });
                }
                Err(e) => {
                    tracing::debug!("Docker still not available: {e}");
                }
            },
            Err(e) => {
                tracing::debug!("Docker still not available: {e}");
            }
        }
        self.advance_retry_backoff();
        None
    }

    pub(super) async fn handle(&mut self, msg: ServerMessage) -> Option<AgentMessage> {
        match msg {
            ServerMessage::DockerStartStats { interval_secs } => {
                if let Some(dm) = self.manager.as_mut() {
                    dm.reset_stats_cache();
                    let secs = interval_secs.max(1);
                    tracing::info!("Starting Docker stats polling every {secs}s");
                    let mut iv = tokio::time::interval(Duration::from_secs(secs as u64));
                    iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    self.stats_interval = Some(iv);
                    None
                } else {
                    tracing::warn!("DockerStartStats received but Docker is not available");
                    Some(AgentMessage::DockerUnavailable { msg_id: None })
                }
            }
            ServerMessage::DockerStopStats => {
                tracing::info!("Stopping Docker stats polling");
                self.stats_interval = None;
                if let Some(handle) = self.pending_stats.take() {
                    handle.abort();
                }
                if let Some(dm) = self.manager.as_mut() {
                    dm.reset_stats_cache();
                }
                None
            }
            msg => {
                if let Some(dm) = self.manager.as_mut() {
                    if let Err(e) = dm.handle_server_message(msg).await {
                        tracing::warn!("Docker runtime became unavailable: {e}");
                        return self.demote();
                    }
                    None
                } else {
                    tracing::warn!("Docker message received but Docker is not available");
                    Some(AgentMessage::DockerUnavailable {
                        msg_id: docker_request_msg_id(&msg),
                    })
                }
            }
        }
    }

    fn advance_retry_backoff(&mut self) {
        self.retry_backoff_idx =
            (self.retry_backoff_idx + 1).min(DOCKER_RETRY_BACKOFF_SECS.len() - 1);
        self.retry_interval = delayed_interval(DOCKER_RETRY_BACKOFF_SECS[self.retry_backoff_idx]);
    }

    fn demote(&mut self) -> Option<AgentMessage> {
        if let Some(handle) = self.pending_stats.take() {
            handle.abort();
        }
        if let Some(dm) = self.manager.as_mut() {
            dm.cleanup();
        }
        self.manager = None;
        self.stats_interval = None;
        self.retry_backoff_idx = 0;
        self.retry_interval = delayed_interval(DOCKER_RETRY_BACKOFF_SECS[0]);

        if self.available {
            self.available = false;
            Some(AgentMessage::FeaturesUpdate { features: vec![] })
        } else {
            None
        }
    }

    #[cfg(test)]
    pub(super) fn stats_polling_active(&self) -> bool {
        self.stats_interval.is_some()
    }

    #[cfg(test)]
    pub(super) fn arm_stats_polling_for_test(&mut self) {
        self.stats_interval = Some(tokio::time::interval(Duration::from_secs(60)));
    }

    #[cfg(test)]
    pub(super) fn set_mock_retry_for_test(&mut self, ok: Option<bool>) {
        self.mock_retry_ok = ok;
    }

    #[cfg(test)]
    pub(super) fn set_available_with_unreachable_daemon_for_test(&mut self) {
        let docker = bollard::Docker::connect_with_http(
            "http://127.0.0.1:1",
            1,
            bollard::API_DEFAULT_VERSION,
        )
        .expect("connect_with_http builds transport without I/O");
        self.manager = Some(DockerManager::from_client_for_test(
            docker,
            self.tx.clone(),
            Arc::clone(&self.capabilities),
        ));
        self.available = true;
    }

    pub(super) fn cleanup(&mut self) {
        if let Some(handle) = self.pending_retry.take() {
            handle.abort();
        }
        if let Some(handle) = self.pending_stats.take() {
            handle.abort();
        }
        if let Some(dm) = self.manager.as_mut() {
            dm.cleanup();
        }
    }
}

/// The `msg_id` a `DockerUnavailable` reply should carry so the server can
/// correlate it with the request; log/event stream variants have none.
fn docker_request_msg_id(msg: &ServerMessage) -> Option<String> {
    match msg {
        ServerMessage::DockerListContainers { msg_id }
        | ServerMessage::DockerContainerAction { msg_id, .. }
        | ServerMessage::DockerGetInfo { msg_id }
        | ServerMessage::DockerListNetworks { msg_id }
        | ServerMessage::DockerListVolumes { msg_id } => Some(msg_id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_subsystem() -> DockerSubsystem {
        let (tx, _rx) = mpsc::channel(8);
        let dir = tempfile::tempdir().expect("tempdir");
        let capabilities = CapabilityAuthority::new(
            serverbee_common::constants::CAP_DEFAULT,
            dir.path().join("grants.json"),
        );
        DockerSubsystem::new(tx, capabilities)
    }

    #[test]
    fn test_docker_request_msg_id_extracts_only_request_variants() {
        // Variants that carry a msg_id return it...
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerListContainers {
                msg_id: "a".to_string()
            }),
            Some("a".to_string())
        );
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerGetInfo {
                msg_id: "b".to_string()
            }),
            Some("b".to_string())
        );
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerListNetworks {
                msg_id: "c".to_string()
            }),
            Some("c".to_string())
        );
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerListVolumes {
                msg_id: "d".to_string()
            }),
            Some("d".to_string())
        );
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerContainerAction {
                msg_id: "e".to_string(),
                container_id: "cid".to_string(),
                action: serverbee_common::docker_types::DockerAction::Restart { timeout: None },
            }),
            Some("e".to_string())
        );
        // ...non-request docker variants return None.
        assert_eq!(docker_request_msg_id(&ServerMessage::DockerStopStats), None);
        assert_eq!(docker_request_msg_id(&ServerMessage::Ping), None);
    }

    #[test]
    fn test_docker_request_msg_id_none_for_log_and_event_variants() {
        // Streaming control variants carry no request msg_id -> None.
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerLogsStart {
                session_id: "s".to_string(),
                container_id: "c".to_string(),
                tail: None,
                follow: false,
            }),
            None
        );
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerLogsStop {
                session_id: "s".to_string(),
            }),
            None
        );
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerEventsStart),
            None
        );
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerEventsStop),
            None
        );
        assert_eq!(
            docker_request_msg_id(&ServerMessage::DockerStartStats { interval_secs: 5 }),
            None
        );
    }

    #[tokio::test]
    async fn test_start_stats_unavailable_replies_unavailable_without_arming_interval() {
        let mut docker = make_subsystem();
        let reply = docker
            .handle(ServerMessage::DockerStartStats { interval_secs: 5 })
            .await;
        assert!(matches!(
            reply,
            Some(AgentMessage::DockerUnavailable { msg_id: None })
        ));
        assert!(docker.stats_interval.is_none());
    }

    #[tokio::test]
    async fn test_stop_stats_clears_interval_silently() {
        let mut docker = make_subsystem();
        docker.stats_interval = Some(tokio::time::interval(Duration::from_secs(1)));
        let reply = docker.handle(ServerMessage::DockerStopStats).await;
        assert!(reply.is_none());
        assert!(docker.stats_interval.is_none());
    }

    #[tokio::test]
    async fn test_request_unavailable_reply_carries_msg_id() {
        let mut docker = make_subsystem();
        let reply = docker
            .handle(ServerMessage::DockerGetInfo {
                msg_id: "req-7".into(),
            })
            .await;
        match reply {
            Some(AgentMessage::DockerUnavailable { msg_id }) => {
                assert_eq!(msg_id.as_deref(), Some("req-7"));
            }
            other => panic!("expected DockerUnavailable, got {other:?}"),
        }
    }

    /// Demotion is announce-once: the empty FeaturesUpdate goes out only when
    /// the feature was previously advertised, and a second demotion is silent.
    #[tokio::test]
    async fn test_demote_emits_features_update_only_when_previously_available() {
        let mut docker = make_subsystem();
        assert!(
            docker.demote().is_none(),
            "never-available demote is silent"
        );

        docker.available = true;
        docker.stats_interval = Some(tokio::time::interval(Duration::from_secs(1)));
        match docker.demote() {
            Some(AgentMessage::FeaturesUpdate { features }) => {
                assert!(features.is_empty(), "demotion clears the feature list");
            }
            other => panic!("expected FeaturesUpdate, got {other:?}"),
        }
        assert!(
            docker.stats_interval.is_none(),
            "demotion stops stats polling"
        );
        assert!(docker.demote().is_none(), "second demote is silent");
    }

    #[tokio::test]
    async fn test_features_reflect_availability() {
        let mut docker = make_subsystem();
        assert!(docker.features().is_empty());
        docker.available = true;
        assert_eq!(docker.features(), vec!["docker".to_string()]);
    }
}
