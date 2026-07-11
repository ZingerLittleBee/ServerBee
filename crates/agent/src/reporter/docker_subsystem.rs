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

use serverbee_common::protocol::{AgentMessage, ServerMessage};
use tokio::sync::mpsc;

use crate::capability_grants::CapabilityAuthority;
use crate::docker::DockerManager;

const DOCKER_RETRY_SECS: u64 = 30;

/// Which housekeeping event [`DockerSubsystem::tick`] resolved to: poll
/// container stats, or retry the daemon connection.
pub(super) enum DockerTick {
    PollStats,
    Retry,
}

pub(super) struct DockerSubsystem {
    capabilities: Arc<CapabilityAuthority>,
    /// Sender cloned into every spawned `DockerManager` task; the matching
    /// receiver is drained by the select! loop in `mod.rs`.
    tx: mpsc::Sender<AgentMessage>,
    manager: Option<DockerManager>,
    available: bool,
    stats_interval: Option<tokio::time::Interval>,
    retry_interval: tokio::time::Interval,
}

impl DockerSubsystem {
    pub(super) fn new(
        tx: mpsc::Sender<AgentMessage>,
        capabilities: Arc<CapabilityAuthority>,
    ) -> Self {
        // First retry fires one full period out — connection start already
        // probes explicitly, so there is no immediate tick to consume.
        let retry_period = Duration::from_secs(DOCKER_RETRY_SECS);
        let retry_interval =
            tokio::time::interval_at(tokio::time::Instant::now() + retry_period, retry_period);
        Self {
            capabilities,
            tx,
            manager: None,
            available: false,
            stats_interval: None,
            retry_interval,
        }
    }

    /// Feature list advertised in `SystemInfo`, derived from what is live.
    pub(super) fn features(&self) -> Vec<String> {
        if self.available {
            vec!["docker".to_string()]
        } else {
            Vec::new()
        }
    }

    /// Initial daemon detection at connection start. Absence is informational
    /// — the retry tick keeps looking.
    pub(super) async fn probe(&mut self) {
        match DockerManager::try_new(self.tx.clone(), Arc::clone(&self.capabilities)) {
            Ok(dm) => match dm.verify_connection().await {
                Ok(()) => {
                    tracing::info!("Docker daemon connected");
                    self.available = true;
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

    /// Wait for the next housekeeping event: a stats-poll tick while the
    /// daemon is connected, or a retry tick while it is absent. Pending when
    /// connected with stats polling off.
    pub(super) async fn tick(&mut self) -> DockerTick {
        if self.manager.is_some() {
            match self.stats_interval.as_mut() {
                Some(iv) => {
                    iv.tick().await;
                    DockerTick::PollStats
                }
                None => std::future::pending().await,
            }
        } else {
            self.retry_interval.tick().await;
            DockerTick::Retry
        }
    }

    /// Poll container stats once; a failing daemon demotes the subsystem.
    /// Returns the `FeaturesUpdate` to send when availability changed.
    pub(super) async fn poll_stats(&mut self) -> Option<AgentMessage> {
        if let Some(dm) = self.manager.as_mut()
            && let Err(e) = dm.poll_stats().await
        {
            tracing::warn!("Docker stats polling failed: {e}");
            return self.demote();
        }
        None
    }

    /// Retry the daemon connection; success returns the `FeaturesUpdate`
    /// re-advertising the feature.
    pub(super) async fn retry(&mut self) -> Option<AgentMessage> {
        tracing::debug!("Retrying Docker connection...");
        match DockerManager::try_new(self.tx.clone(), Arc::clone(&self.capabilities)) {
            Ok(dm) => match dm.verify_connection().await {
                Ok(()) => {
                    tracing::info!("Docker daemon now available");
                    self.manager = Some(dm);
                    self.available = true;
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
        None
    }

    /// Handle one Docker-family server message (stats control or a request
    /// forwarded to the daemon). Returns the reply to send, if any.
    pub(super) async fn handle(&mut self, msg: ServerMessage) -> Option<AgentMessage> {
        match msg {
            ServerMessage::DockerStartStats { interval_secs } => {
                if self.manager.is_some() {
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

    /// Drop the daemon connection and stop stats polling. Returns the empty
    /// `FeaturesUpdate` when the feature was previously advertised.
    fn demote(&mut self) -> Option<AgentMessage> {
        if let Some(dm) = self.manager.as_mut() {
            dm.cleanup();
        }
        self.manager = None;
        self.stats_interval = None;

        if self.available {
            self.available = false;
            Some(AgentMessage::FeaturesUpdate { features: vec![] })
        } else {
            None
        }
    }

    /// Whether a stats-poll interval is armed — observability for the
    /// dispatch harness in `runtime::tests`.
    #[cfg(test)]
    pub(super) fn stats_polling_active(&self) -> bool {
        self.stats_interval.is_some()
    }

    /// Pre-arm stats polling so tests can observe it being cleared.
    #[cfg(test)]
    pub(super) fn arm_stats_polling_for_test(&mut self) {
        self.stats_interval = Some(tokio::time::interval(Duration::from_secs(60)));
    }

    /// Tear down the daemon connection without emitting anything; part of
    /// connection shutdown where the socket is already gone.
    pub(super) fn cleanup(&mut self) {
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
