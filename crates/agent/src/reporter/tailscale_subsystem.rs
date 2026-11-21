//! Tailscale availability state machine, connection-scoped.
//!
//! Mirrors [`super::docker_subsystem::DockerSubsystem`]: probes `tailscaled` on
//! connection start, advertises `"tailscale"` in `SystemInfo.features`, pushes
//! an initial `AgentMessage::TailscaleStatus` after `SystemInfo` when
//! `CAP_TAILSCALE` is effective, refreshes every 60s while available, retries
//! every 30s while absent, and answers `ServerMessage::TailscaleGetStatus`.
//!
//! Availability transitions report `features_changed = true` to
//! [`super::runtime::ConnectionRuntime`], which computes the combined `features`
//! vector across all subsystems (`docker` + `tailscale`) so one subsystem's
//! retry or demotion never erases another subsystem's feature flag.

use std::sync::Arc;
use std::time::Duration;

use serverbee_common::constants::CAP_TAILSCALE;
use serverbee_common::protocol::AgentMessage;
use serverbee_common::tailscale_types::TailscaleStatusReport;

use crate::capability_grants::CapabilityAuthority;
use crate::tailscale::TailscaleProbe;

const TAILSCALE_REFRESH_SECS: u64 = 60;
const TAILSCALE_RETRY_SECS: u64 = 30;

/// Which housekeeping event [`TailscaleSubsystem::tick`] resolved to.
pub(super) enum TailscaleTick {
    Refresh,
    Retry,
}

/// Outcome of a [`TailscaleSubsystem`] operation. When `features_changed` is
/// `true`, `ConnectionRuntime` emits `AgentMessage::FeaturesUpdate` carrying the
/// combined runtime feature set.
pub(super) struct TailscaleStep {
    pub(super) messages: Vec<AgentMessage>,
    pub(super) features_changed: bool,
    pub(super) emit_features_first: bool,
}

pub(super) struct TailscaleSubsystem {
    capabilities: Arc<CapabilityAuthority>,
    probe: Arc<dyn TailscaleProbe>,
    available: bool,
    initial_status: Option<TailscaleStatusReport>,
    refresh_interval: tokio::time::Interval,
    retry_interval: tokio::time::Interval,
}

fn delayed_interval(secs: u64) -> tokio::time::Interval {
    let period = Duration::from_secs(secs);
    let mut iv = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    iv
}

impl TailscaleSubsystem {
    pub(super) fn new(
        probe: Arc<dyn TailscaleProbe>,
        capabilities: Arc<CapabilityAuthority>,
    ) -> Self {
        Self {
            capabilities,
            probe,
            available: false,
            initial_status: None,
            refresh_interval: delayed_interval(TAILSCALE_REFRESH_SECS),
            retry_interval: delayed_interval(TAILSCALE_RETRY_SECS),
        }
    }

    pub(super) fn features(&self) -> Vec<String> {
        if self.available {
            vec!["tailscale".to_string()]
        } else {
            Vec::new()
        }
    }

    /// Initial daemon detection at connection start. Caches the fetched status
    /// so [`Self::take_initial_status_message`] can emit it immediately after
    /// `SystemInfo` without probing twice.
    pub(super) async fn probe(&mut self) {
        match self.probe.fetch_status().await {
            Ok(status) => {
                tracing::info!(
                    backend_state = %status.summary.backend_state,
                    "Tailscale daemon connected"
                );
                self.available = true;
                self.initial_status = Some(status);
                self.refresh_interval = delayed_interval(TAILSCALE_REFRESH_SECS);
            }
            Err(e) => {
                tracing::info!("Tailscale not available: {e}");
                self.available = false;
                self.initial_status = None;
            }
        }
    }

    /// Consume the startup-probed status and return an unsolicited
    /// `AgentMessage::TailscaleStatus { msg_id: None, .. }` when Tailscale is
    /// available and `CAP_TAILSCALE` is effective.
    pub(super) fn take_initial_status_message(&mut self) -> Option<AgentMessage> {
        let status = self.initial_status.take()?;
        if self.available && self.capabilities.has(CAP_TAILSCALE) {
            Some(AgentMessage::TailscaleStatus {
                msg_id: None,
                status,
            })
        } else {
            None
        }
    }

    /// Wait for the next periodic housekeeping tick: 60s refresh while
    /// available, or 30s retry while absent.
    pub(super) async fn tick(&mut self) -> TailscaleTick {
        if self.available {
            self.refresh_interval.tick().await;
            TailscaleTick::Refresh
        } else {
            self.retry_interval.tick().await;
            TailscaleTick::Retry
        }
    }

    /// Periodic 60s status refresh; demotes the subsystem if `tailscaled`
    /// disappeared.
    pub(super) async fn refresh(&mut self) -> TailscaleStep {
        match self.probe.fetch_status().await {
            Ok(status) => {
                let messages = if self.capabilities.has(CAP_TAILSCALE) {
                    vec![AgentMessage::TailscaleStatus {
                        msg_id: None,
                        status,
                    }]
                } else {
                    Vec::new()
                };
                TailscaleStep {
                    messages,
                    features_changed: false,
                    emit_features_first: false,
                }
            }
            Err(e) => {
                tracing::warn!("Tailscale status refresh failed: {e}");
                let changed = self.demote_internal();
                TailscaleStep {
                    messages: Vec::new(),
                    features_changed: changed,
                    emit_features_first: true,
                }
            }
        }
    }

    /// Periodic 30s reconnect attempt while `tailscaled` is absent.
    pub(super) async fn retry(&mut self) -> TailscaleStep {
        tracing::debug!("Retrying Tailscale connection...");
        match self.probe.fetch_status().await {
            Ok(status) => {
                tracing::info!(
                    backend_state = %status.summary.backend_state,
                    "Tailscale daemon now available"
                );
                let changed = !self.available;
                self.available = true;
                self.refresh_interval = delayed_interval(TAILSCALE_REFRESH_SECS);
                let messages = if self.capabilities.has(CAP_TAILSCALE) {
                    vec![AgentMessage::TailscaleStatus {
                        msg_id: None,
                        status,
                    }]
                } else {
                    Vec::new()
                };
                TailscaleStep {
                    messages,
                    features_changed: changed,
                    emit_features_first: true,
                }
            }
            Err(e) => {
                tracing::debug!("Tailscale still not available: {e}");
                TailscaleStep {
                    messages: Vec::new(),
                    features_changed: false,
                    emit_features_first: false,
                }
            }
        }
    }

    /// Handle `ServerMessage::TailscaleGetStatus { msg_id }`. Replies with
    /// `TailscaleUnavailable` when unavailable or when `CAP_TAILSCALE` is off.
    pub(super) async fn handle_get_status(&mut self, msg_id: String) -> TailscaleStep {
        if !self.capabilities.has(CAP_TAILSCALE) {
            tracing::warn!("TailscaleGetStatus denied: capability disabled (msg_id={msg_id})");
            return TailscaleStep {
                messages: vec![AgentMessage::TailscaleUnavailable {
                    msg_id: Some(msg_id),
                    reason: Some("Tailscale capability disabled".to_string()),
                }],
                features_changed: false,
                emit_features_first: false,
            };
        }

        if !self.available {
            tracing::debug!("TailscaleGetStatus received while Tailscale is unavailable");
            return TailscaleStep {
                messages: vec![AgentMessage::TailscaleUnavailable {
                    msg_id: Some(msg_id),
                    reason: Some("Tailscale is not available on this host".to_string()),
                }],
                features_changed: false,
                emit_features_first: false,
            };
        }

        match self.probe.fetch_status().await {
            Ok(status) => TailscaleStep {
                messages: vec![AgentMessage::TailscaleStatus {
                    msg_id: Some(msg_id),
                    status,
                }],
                features_changed: false,
                emit_features_first: false,
            },
            Err(e) => {
                tracing::warn!("Tailscale runtime became unavailable: {e}");
                let changed = self.demote_internal();
                TailscaleStep {
                    messages: vec![AgentMessage::TailscaleUnavailable {
                        msg_id: Some(msg_id),
                        reason: Some(e.to_string()),
                    }],
                    features_changed: changed,
                    emit_features_first: false,
                }
            }
        }
    }

    fn demote_internal(&mut self) -> bool {
        self.initial_status = None;
        if self.available {
            self.available = false;
            self.retry_interval = delayed_interval(TAILSCALE_RETRY_SECS);
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    pub(super) fn set_probe_for_test(&mut self, probe: Arc<dyn TailscaleProbe>) {
        self.probe = probe;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use serverbee_common::constants::CAP_DEFAULT;
    use serverbee_common::tailscale_types::TailscaleNodeSummary;

    use super::*;

    struct MockProbe {
        result: Mutex<Result<TailscaleStatusReport, String>>,
    }

    impl MockProbe {
        fn ok(report: TailscaleStatusReport) -> Arc<Self> {
            Arc::new(Self {
                result: Mutex::new(Ok(report)),
            })
        }

        fn err(msg: &str) -> Arc<Self> {
            Arc::new(Self {
                result: Mutex::new(Err(msg.to_string())),
            })
        }

        fn set(&self, next: Result<TailscaleStatusReport, String>) {
            *self.result.lock().unwrap() = next;
        }
    }

    #[async_trait]
    impl TailscaleProbe for MockProbe {
        async fn fetch_status(&self) -> anyhow::Result<TailscaleStatusReport> {
            match self.result.lock().unwrap().clone() {
                Ok(r) => Ok(r),
                Err(e) => anyhow::bail!(e),
            }
        }
    }

    fn sample_report() -> TailscaleStatusReport {
        TailscaleStatusReport {
            summary: TailscaleNodeSummary {
                backend_state: "Running".into(),
                tailscale_ips: vec!["100.64.0.5".into()],
                dns_name: Some("box.ts.net".into()),
                tailnet_name: Some("ts.net".into()),
                version: Some("1.78.0".into()),
                hostname: Some("box".into()),
                online: true,
                exit_node: false,
                exit_node_option: false,
                key_expiry: None,
            },
            advertised_routes: vec![],
            tags: vec![],
            derp_region: Some("tok".into()),
            peers: vec![],
            health_warnings: vec![],
            collected_at: "2026-11-21T03:35:00Z".into(),
        }
    }

    #[tokio::test]
    async fn probe_populates_features_and_initial_status_when_capable() {
        let probe = MockProbe::ok(sample_report());
        let caps = CapabilityAuthority::fixed(CAP_DEFAULT);
        let mut sub = TailscaleSubsystem::new(probe, caps);

        sub.probe().await;
        assert_eq!(sub.features(), vec!["tailscale".to_string()]);

        let init = sub.take_initial_status_message();
        assert!(matches!(
            init,
            Some(AgentMessage::TailscaleStatus { msg_id: None, .. })
        ));
        assert!(
            sub.take_initial_status_message().is_none(),
            "initial status is consumed once"
        );
    }

    #[tokio::test]
    async fn probe_suppresses_initial_status_when_capability_disabled() {
        let probe = MockProbe::ok(sample_report());
        let caps = CapabilityAuthority::fixed(CAP_DEFAULT & !CAP_TAILSCALE);
        let mut sub = TailscaleSubsystem::new(probe, caps);

        sub.probe().await;
        assert_eq!(sub.features(), vec!["tailscale".to_string()]);
        assert!(sub.take_initial_status_message().is_none());
    }

    #[tokio::test]
    async fn get_status_replies_unavailable_when_capability_disabled_or_absent() {
        let probe = MockProbe::ok(sample_report());
        let caps = CapabilityAuthority::fixed(0);
        let mut sub = TailscaleSubsystem::new(probe, caps);
        sub.probe().await;

        let denied = sub.handle_get_status("req-1".into()).await;
        assert!(!denied.features_changed);
        assert!(matches!(
            &denied.messages[..],
            [AgentMessage::TailscaleUnavailable { msg_id: Some(id), reason: Some(r) }]
            if id == "req-1" && r.contains("disabled")
        ));

        let absent_probe = MockProbe::err("no socket");
        let mut absent =
            TailscaleSubsystem::new(absent_probe, CapabilityAuthority::fixed(CAP_DEFAULT));
        absent.probe().await;
        let step = absent.handle_get_status("req-2".into()).await;
        assert!(!step.features_changed);
        assert!(matches!(
            &step.messages[..],
            [AgentMessage::TailscaleUnavailable { msg_id: Some(id), .. }] if id == "req-2"
        ));
    }

    #[tokio::test]
    async fn refresh_and_retry_signal_features_changed_on_transitions() {
        let probe = MockProbe::err("offline");
        let mut sub = TailscaleSubsystem::new(
            Arc::clone(&probe) as Arc<dyn TailscaleProbe>,
            CapabilityAuthority::fixed(CAP_DEFAULT),
        );
        sub.probe().await;
        assert!(sub.features().is_empty());

        // Daemon comes online during retry -> features_changed = true + status push.
        probe.set(Ok(sample_report()));
        let promoted = sub.retry().await;
        assert!(promoted.features_changed);
        assert!(promoted.emit_features_first);
        assert_eq!(promoted.messages.len(), 1);
        assert_eq!(sub.features(), vec!["tailscale".to_string()]);

        // Daemon goes offline during refresh -> features_changed = true.
        probe.set(Err("crashed".into()));
        let demoted = sub.refresh().await;
        assert!(demoted.features_changed);
        assert!(sub.features().is_empty());
    }
}
