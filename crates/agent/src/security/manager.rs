//! Wires the security pipeline: journal watcher → SSH detector → first-seen
//! lookup → `AgentMessage::SecurityEvent`. Optionally spawns the conntrack
//! watcher and scan detector when `port_scan.enabled`. No-op off-Linux.

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

use crate::capability_grants::CapabilityAuthority;
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

    /// Supervise the pipeline against the capability authority.
    ///
    /// Starts the pipeline when `CAP_SECURITY_EVENTS` is (or becomes)
    /// effective — including via a temporary grant at any point in the
    /// agent's lifetime — and stops it when the capability expires or is
    /// revoked, so a bounded grant tears its running work down. Replaces the
    /// old start-once-at-boot wiring, which could neither start on a later
    /// grant nor stop on expiry.
    pub fn spawn_supervised(
        cfg: SecurityConfig,
        authority: Arc<CapabilityAuthority>,
        tx: mpsc::Sender<AgentMessage>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut state = authority.subscribe_state();
            let mut running: Option<SecurityManager> = None;
            loop {
                let wanted = has_capability(*state.borrow_and_update(), CAP_SECURITY_EVENTS);
                if wanted && running.is_none() {
                    match Self::start(cfg.clone(), authority.effective(), tx.clone()).await {
                        Ok(m) => running = Some(m),
                        Err(e) => tracing::warn!(
                            error = %e,
                            "SecurityManager failed to start; will retry on next capability change"
                        ),
                    }
                } else if !wanted && running.take().is_some() {
                    // Dropping the manager aborts its pipeline tasks.
                    tracing::info!(
                        "security pipeline stopped: CAP_SECURITY_EVENTS no longer effective"
                    );
                }
                if state.changed().await.is_err() {
                    break;
                }
            }
        })
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
    let mut sweep_interval = tokio::time::interval(Duration::from_secs(10));
    sweep_interval.tick().await;
    loop {
        let attempt = tokio::select! {
            maybe = rx.recv() => match maybe {
                Some(a) => a,
                None => return,
            },
            _ = sweep_interval.tick() => {
                detector.sweep();
                continue;
            }
        };
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
    use crate::security::ssh_parser::{AuthMethodHint, AuthOutcome};
    use serverbee_common::constants::CAP_DEFAULT;

    fn login_attempt(username: &str, source_ip: &str) -> AuthAttempt {
        AuthAttempt {
            outcome: AuthOutcome::Success {
                auth_method: AuthMethodHint::Publickey,
            },
            username: username.to_string(),
            source_ip: source_ip.to_string(),
            source_port: Some(22),
        }
    }

    fn failed_attempt(username: &str, source_ip: &str, invalid_user: bool) -> AuthAttempt {
        AuthAttempt {
            outcome: AuthOutcome::Failure { invalid_user },
            username: username.to_string(),
            source_ip: source_ip.to_string(),
            source_port: Some(22),
        }
    }

    async fn recv_security_event(rx: &mut mpsc::Receiver<AgentMessage>) -> SecurityEventPayload {
        match rx.recv().await {
            Some(AgentMessage::SecurityEvent(payload)) => payload,
            other => panic!("expected SecurityEvent, got {other:?}"),
        }
    }

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

    #[tokio::test]
    async fn ssh_pipeline_marks_only_the_first_login_from_an_identity_as_new() {
        let dir = tempfile::tempdir().unwrap();
        let first_seen = Arc::new(Mutex::new(FirstSeenStore::open(
            dir.path().join("first_seen.json"),
            FIRST_SEEN_CAP,
        )));
        let (attempt_tx, attempt_rx) = mpsc::channel(4);
        let (event_tx, mut event_rx) = mpsc::channel(4);
        let task = tokio::spawn(run_ssh_pipeline(
            attempt_rx,
            crate::config::SshDetectorConfig::default(),
            first_seen,
            event_tx,
        ));

        attempt_tx
            .send(login_attempt("root", "203.0.113.10"))
            .await
            .unwrap();
        let first = recv_security_event(&mut event_rx).await;
        assert_eq!(first.event_type, SecurityEventType::SshLogin);
        assert_eq!(first.severity, Severity::Medium);
        assert!(first.first_seen);
        assert_eq!(first.username.as_deref(), Some("root"));
        assert_eq!(first.source_ip, "203.0.113.10");
        assert_eq!(first.source_port, Some(22));
        assert!(matches!(
            first.evidence,
            SecurityEvidence::SshLogin {
                auth_method: SshAuthMethod::Publickey
            }
        ));

        attempt_tx
            .send(login_attempt("root", "203.0.113.10"))
            .await
            .unwrap();
        let repeated = recv_security_event(&mut event_rx).await;
        assert_eq!(repeated.severity, Severity::Info);
        assert!(!repeated.first_seen);

        drop(attempt_tx);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn ssh_pipeline_emits_brute_force_evidence_at_the_configured_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let first_seen = Arc::new(Mutex::new(FirstSeenStore::open(
            dir.path().join("first_seen.json"),
            FIRST_SEEN_CAP,
        )));
        let (attempt_tx, attempt_rx) = mpsc::channel(4);
        let (event_tx, mut event_rx) = mpsc::channel(4);
        let task = tokio::spawn(run_ssh_pipeline(
            attempt_rx,
            crate::config::SshDetectorConfig {
                window_seconds: 30,
                failed_threshold: 2,
            },
            first_seen,
            event_tx,
        ));

        attempt_tx
            .send(failed_attempt("root", "198.51.100.20", false))
            .await
            .unwrap();
        attempt_tx
            .send(failed_attempt("admin", "198.51.100.20", true))
            .await
            .unwrap();

        let payload = recv_security_event(&mut event_rx).await;
        assert_eq!(payload.event_type, SecurityEventType::SshBruteForce);
        assert_eq!(payload.severity, Severity::High);
        assert_eq!(payload.source_ip, "198.51.100.20");
        assert_eq!(payload.source_port, None);
        assert!(!payload.first_seen);
        assert!(payload.started_at <= payload.ended_at);
        match payload.evidence {
            SecurityEvidence::SshBruteForce {
                failed_count,
                distinct_users,
                invalid_user_count,
                window_seconds,
                threshold,
                ..
            } => {
                assert_eq!(failed_count, 2);
                assert_eq!(distinct_users, 2);
                assert_eq!(invalid_user_count, 1);
                assert_eq!(window_seconds, 30);
                assert_eq!(threshold, 2);
            }
            other => panic!("expected SshBruteForce evidence, got {other:?}"),
        }

        drop(attempt_tx);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn scan_pipeline_emits_port_scan_and_records_blocked_attempts() {
        let (conntrack_tx, conntrack_rx) = mpsc::channel(4);
        let (blocked_tx, blocked_rx) = mpsc::channel(1);
        let (event_tx, mut event_rx) = mpsc::channel(4);

        blocked_tx.send("192.0.2.44".to_string()).await.unwrap();
        let task = tokio::spawn(run_scan_pipeline(
            conntrack_rx,
            blocked_rx,
            crate::config::PortScanConfig {
                enabled: true,
                window_seconds: 15,
                distinct_port_threshold: 2,
            },
            event_tx,
        ));

        tokio::time::timeout(Duration::from_secs(1), async {
            while blocked_tx.capacity() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        for dst_port in [22, 443] {
            conntrack_tx
                .send(ConntrackEvent {
                    source_ip: "192.0.2.44".to_string(),
                    dst_port,
                })
                .await
                .unwrap();
        }

        let payload = recv_security_event(&mut event_rx).await;
        assert_eq!(payload.event_type, SecurityEventType::PortScan);
        assert_eq!(payload.severity, Severity::High);
        assert_eq!(payload.detector_source, DetectorSource::Conntrack);
        assert_eq!(payload.source_ip, "192.0.2.44");
        match payload.evidence {
            SecurityEvidence::PortScan {
                distinct_ports,
                total_attempts,
                blocked_count,
                window_seconds,
                threshold,
                ..
            } => {
                assert_eq!(distinct_ports, 2);
                assert_eq!(total_attempts, 2);
                assert_eq!(blocked_count, 1);
                assert_eq!(window_seconds, 15);
                assert_eq!(threshold, 2);
            }
            other => panic!("expected PortScan evidence, got {other:?}"),
        }

        task.abort();
        task.await.unwrap_err();
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
