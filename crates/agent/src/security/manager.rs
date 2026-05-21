//! Top-level wiring for the security pipeline.
//!
//! Responsibilities:
//! * Honour `CAP_SECURITY_EVENTS` and the agent's `SecurityConfig.enabled`.
//! * Spawn the journal watcher → ssh detector → first-seen → AgentMessage
//!   pipeline.
//! * If `port_scan.enabled`, additionally spawn the conntrack watcher
//!   (which is best-effort — if conntrack cannot be started we log a
//!   warning and continue with brute-force-only detection).
//!
//! On non-Linux platforms `start` is a no-op: no handles are spawned.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serverbee_common::constants::{CAP_SECURITY_EVENTS, has_capability};
use serverbee_common::protocol::AgentMessage;
use serverbee_common::security::{
    DetectorSource, SecurityEventPayload, SecurityEventType, SecurityEvidence, Severity,
    SshAuthMethod,
};
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::SecurityConfig;
use crate::security::conntrack_watcher::{self, ConntrackEvent};
use crate::security::first_seen_store::FirstSeenStore;
use crate::security::journal_watcher;
use crate::security::scan_detector::{ScanDetector, ScanEmit};
use crate::security::ssh_detector::{DetectorEmit, SshDetector};
use crate::security::ssh_parser::AuthAttempt;

const FIRST_SEEN_CAP: usize = 4096;

pub struct SecurityManager {
    handles: Vec<JoinHandle<()>>,
}

impl SecurityManager {
    /// Build a no-op manager. Used both when the feature is disabled and as
    /// the early-return path on non-Linux platforms.
    fn disabled() -> Self {
        Self { handles: vec![] }
    }

    /// Start the security pipeline.
    ///
    /// Returns an empty manager (no handles) when:
    /// * `CAP_SECURITY_EVENTS` is not present in `agent_caps`, or
    /// * `cfg.enabled` is false, or
    /// * the host is not Linux.
    pub async fn start(
        cfg: SecurityConfig,
        agent_caps: u32,
        tx: mpsc::Sender<AgentMessage>,
    ) -> anyhow::Result<Self> {
        if !has_capability(agent_caps, CAP_SECURITY_EVENTS) {
            tracing::info!("CAP_SECURITY_EVENTS not granted locally; SecurityManager disabled");
            return Ok(Self::disabled());
        }
        if !cfg.enabled {
            tracing::info!("SecurityManager disabled by config");
            return Ok(Self::disabled());
        }
        if cfg!(not(target_os = "linux")) {
            tracing::info!("SecurityManager disabled on non-Linux platform");
            return Ok(Self::disabled());
        }

        let mut handles = Vec::new();

        // First-seen store lives on disk so a restart doesn't re-trigger
        // "new IP" events for already-known administrators.
        let first_seen_path = PathBuf::from(&cfg.data_dir).join("first_seen.json");
        let first_seen = Arc::new(Mutex::new(FirstSeenStore::open(
            first_seen_path,
            FIRST_SEEN_CAP,
        )));

        // SSH pipeline: journalctl → AuthAttempt → SshDetector → AgentMessage.
        let (ssh_attempt_tx, ssh_attempt_rx) = mpsc::channel::<AuthAttempt>(256);
        handles.push(tokio::spawn({
            let tx = ssh_attempt_tx.clone();
            async move {
                journal_watcher::run_sshd_stream(tx).await;
            }
        }));

        let ssh_cfg = cfg.ssh.clone();
        let tx_for_ssh = tx.clone();
        let first_seen_for_ssh = first_seen.clone();
        handles.push(tokio::spawn(async move {
            run_ssh_pipeline(ssh_attempt_rx, ssh_cfg, first_seen_for_ssh, tx_for_ssh).await;
        }));

        // Port-scan pipeline is optional — `cfg.port_scan.enabled` gates it
        // and a failure to spawn `conntrack` is non-fatal.
        if cfg.port_scan.enabled {
            let scan_cfg = cfg.port_scan.clone();
            let (conntrack_tx, conntrack_rx) = mpsc::channel::<ConntrackEvent>(256);
            let (blocked_tx, blocked_rx) = mpsc::channel::<String>(128);

            // Try to start conntrack first; if it fails immediately (e.g.
            // missing binary or EPERM), keep brute-force detection on and
            // skip the scan pipeline.
            let conntrack_tx_for_spawn = conntrack_tx.clone();
            handles.push(tokio::spawn(async move {
                if let Err(e) =
                    conntrack_watcher::start_conntrack_stream(conntrack_tx_for_spawn).await
                {
                    tracing::warn!(
                        error = %e,
                        "conntrack stream unavailable; port-scan detection disabled"
                    );
                }
            }));

            // Kernel firewall log stream (best-effort).
            handles.push(tokio::spawn({
                let blocked_tx = blocked_tx.clone();
                async move {
                    journal_watcher::run_kernel_stream(blocked_tx).await;
                }
            }));

            let tx_for_scan = tx.clone();
            handles.push(tokio::spawn(async move {
                run_scan_pipeline(conntrack_rx, blocked_rx, scan_cfg, tx_for_scan).await;
            }));
        }

        Ok(Self { handles })
    }

    /// For tests/diagnostics: number of spawned background tasks.
    pub fn handle_count(&self) -> usize {
        self.handles.len()
    }
}

impl Drop for SecurityManager {
    fn drop(&mut self) {
        for h in self.handles.drain(..) {
            h.abort();
        }
    }
}

async fn run_ssh_pipeline(
    mut rx: mpsc::Receiver<AuthAttempt>,
    cfg: crate::config::SshDetectorConfig,
    first_seen: Arc<Mutex<FirstSeenStore>>,
    tx: mpsc::Sender<AgentMessage>,
) {
    let mut detector = SshDetector::new(
        Duration::from_secs(cfg.window_seconds as u64),
        cfg.failed_threshold,
    );
    while let Some(attempt) = rx.recv().await {
        let emit = detector.observe(attempt);
        match emit {
            DetectorEmit::None => {}
            DetectorEmit::Login {
                username,
                source_ip,
                source_port,
                auth_method,
            } => {
                let now = chrono::Utc::now().timestamp();
                let (first_seen_flag, evidence) =
                    build_ssh_login_payload(&first_seen, &username, &source_ip, auth_method, now)
                        .await;
                let payload = SecurityEventPayload {
                    event_type: SecurityEventType::SshLogin,
                    severity: if first_seen_flag {
                        Severity::Medium
                    } else {
                        Severity::Info
                    },
                    source_ip,
                    source_port,
                    username: Some(username),
                    started_at: now,
                    ended_at: now,
                    first_seen: first_seen_flag,
                    detector_source: DetectorSource::Journal,
                    evidence,
                };
                let _ = tx.send(AgentMessage::SecurityEvent(payload)).await;
            }
            DetectorEmit::BruteForce {
                source_ip,
                severity,
                evidence,
                ..
            } => {
                let now = chrono::Utc::now().timestamp();
                let payload = SecurityEventPayload {
                    event_type: SecurityEventType::SshBruteForce,
                    severity,
                    source_ip,
                    source_port: None,
                    username: None,
                    started_at: now.saturating_sub(cfg.window_seconds as i64),
                    ended_at: now,
                    first_seen: false,
                    detector_source: DetectorSource::Journal,
                    evidence,
                };
                let _ = tx.send(AgentMessage::SecurityEvent(payload)).await;
            }
        }
    }
}

async fn build_ssh_login_payload(
    first_seen: &Arc<Mutex<FirstSeenStore>>,
    username: &str,
    source_ip: &str,
    auth_method: SshAuthMethod,
    now: i64,
) -> (bool, SecurityEvidence) {
    let mut guard = first_seen.lock().await;
    let is_first = guard.mark(username, source_ip, now);
    if let Err(e) = guard.flush() {
        tracing::warn!(error = %e, "failed to flush first_seen store");
    }
    (is_first, SecurityEvidence::SshLogin { auth_method })
}

async fn run_scan_pipeline(
    mut conntrack_rx: mpsc::Receiver<ConntrackEvent>,
    mut blocked_rx: mpsc::Receiver<String>,
    cfg: crate::config::PortScanConfig,
    tx: mpsc::Sender<AgentMessage>,
) {
    let mut detector = ScanDetector::new(
        Duration::from_secs(cfg.window_seconds as u64),
        cfg.distinct_port_threshold,
    );
    let mut sweep_interval = tokio::time::interval(Duration::from_secs(10));
    sweep_interval.tick().await;
    loop {
        tokio::select! {
            Some(ev) = conntrack_rx.recv() => {
                let emit = detector.observe(ev.source_ip, ev.dst_port);
                if let ScanEmit::PortScan { source_ip, evidence, .. } = emit {
                    let now = chrono::Utc::now().timestamp();
                    let payload = SecurityEventPayload {
                        event_type: SecurityEventType::PortScan,
                        severity: Severity::High,
                        source_ip,
                        source_port: None,
                        username: None,
                        started_at: now.saturating_sub(cfg.window_seconds as i64),
                        ended_at: now,
                        first_seen: false,
                        detector_source: DetectorSource::Conntrack,
                        evidence,
                    };
                    let _ = tx.send(AgentMessage::SecurityEvent(payload)).await;
                }
            }
            Some(ip) = blocked_rx.recv() => {
                detector.record_blocked(&ip);
            }
            _ = sweep_interval.tick() => {
                detector.sweep();
            }
            else => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::CAP_DEFAULT;

    #[tokio::test]
    async fn start_returns_empty_when_capability_missing() {
        let cfg = SecurityConfig::default();
        let (tx, _rx) = mpsc::channel(8);
        let caps = CAP_DEFAULT & !CAP_SECURITY_EVENTS;
        let mgr = SecurityManager::start(cfg, caps, tx).await.unwrap();
        assert_eq!(mgr.handle_count(), 0);
    }

    #[tokio::test]
    async fn start_returns_empty_when_disabled_in_config() {
        let cfg = SecurityConfig {
            enabled: false,
            ..SecurityConfig::default()
        };
        let (tx, _rx) = mpsc::channel(8);
        let mgr = SecurityManager::start(cfg, CAP_DEFAULT, tx).await.unwrap();
        assert_eq!(mgr.handle_count(), 0);
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn start_returns_empty_on_non_linux() {
        let cfg = SecurityConfig::default();
        let (tx, _rx) = mpsc::channel(8);
        let mgr = SecurityManager::start(cfg, CAP_DEFAULT, tx).await.unwrap();
        assert_eq!(mgr.handle_count(), 0);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn start_spawns_handles_when_enabled_on_linux() {
        let cfg = SecurityConfig::default();
        let (tx, _rx) = mpsc::channel(8);
        let mgr = SecurityManager::start(cfg, CAP_DEFAULT, tx).await.unwrap();
        // At least the journal watcher + ssh pipeline → 2 handles.
        assert!(mgr.handle_count() >= 2);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn start_skips_conntrack_when_port_scan_disabled() {
        let mut cfg = SecurityConfig::default();
        cfg.port_scan.enabled = false;
        let (tx, _rx) = mpsc::channel(8);
        let mgr = SecurityManager::start(cfg, CAP_DEFAULT, tx).await.unwrap();
        // 2 handles when scan disabled (journal_watcher + ssh pipeline).
        assert_eq!(mgr.handle_count(), 2);
    }
}
