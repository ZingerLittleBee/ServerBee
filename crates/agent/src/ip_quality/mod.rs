pub mod detectors;
pub mod http;
pub mod rule_engine;
pub mod ssrf;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use chrono::Utc;
use serverbee_common::constants::{CAP_IP_QUALITY, has_capability};
use serverbee_common::protocol::{UnlockResultData, UnlockServiceDef, UnlockStatus};
use tokio::sync::{Mutex, Semaphore, watch};
use tokio::sync::mpsc;

/// Maximum number of services probed concurrently within a single run.
const MAX_UNLOCK_CONCURRENT: usize = 5;

/// Default check interval used before the server has synced settings.
const DEFAULT_INTERVAL_HOURS: u32 = 12;

/// Result of a single `run_once` call.
pub struct RunResult {
    pub egress_ip: String,
    pub results: Vec<UnlockResultData>,
    pub checked_at: chrono::DateTime<Utc>,
}

/// Manages the IP-quality unlock checker schedule.
///
/// Mirrors the structure of `NetworkProber` in `network_prober.rs`:
///   - `sync()` ingests new service list + interval from `IpQualitySync`.
///   - `run_now()` triggers an immediate check outside the regular schedule.
///   - A background scheduler loop drives periodic runs, IP-change reruns, and
///     manual triggers.  Results are sent via an mpsc channel to the reporter.
///
/// Gated by `CAP_IP_QUALITY` — if the effective capability is absent the
/// checker stays idle and ignores sync/run_now calls.
pub struct UnlockChecker {
    /// Current service definitions (shared with the scheduler task).
    services: Arc<Mutex<Vec<UnlockServiceDef>>>,
    /// Current check interval in hours (shared with the scheduler task).
    interval_hours: Arc<Mutex<u32>>,
    /// Channel sender for signalling a manual run (mpsc: signals queue up, never dropped).
    run_now_tx: mpsc::Sender<()>,
    /// Watch channel sender for IP changes (watch: only the latest value matters).
    ip_changed_tx: watch::Sender<Option<String>>,
    /// Effective capabilities bitmap (shared, updated by `CapabilitiesSync`).
    capabilities: Arc<AtomicU32>,
}

impl UnlockChecker {
    /// Create a new `UnlockChecker` and spawn its scheduler task.
    ///
    /// `result_tx` receives a `RunResult` each time a run completes.
    pub fn new(
        capabilities: Arc<AtomicU32>,
        result_tx: mpsc::Sender<RunResult>,
    ) -> Self {
        let services = Arc::new(Mutex::new(Vec::new()));
        let interval_hours = Arc::new(Mutex::new(DEFAULT_INTERVAL_HOURS));
        // mpsc for run_now: buffered so signals sent before the scheduler starts are not lost.
        let (run_now_tx, run_now_rx) = mpsc::channel::<()>(16);
        let (ip_changed_tx, ip_changed_rx) = watch::channel::<Option<String>>(None);

        let checker = Self {
            services: Arc::clone(&services),
            interval_hours: Arc::clone(&interval_hours),
            run_now_tx,
            ip_changed_tx,
            capabilities: Arc::clone(&capabilities),
        };

        // Spawn the scheduler task.
        tokio::spawn(run_scheduler(
            services,
            interval_hours,
            run_now_rx,
            ip_changed_rx,
            capabilities,
            result_tx,
        ));

        checker
    }

    /// Absorb a fresh `IpQualitySync` service list + interval.
    ///
    /// If `CAP_IP_QUALITY` is not effective this is a no-op (the server only
    /// sends the sync when the capability is effective, but we guard here too
    /// as defence-in-depth).
    pub async fn sync(&self, services: Vec<UnlockServiceDef>, interval_hours: u32) {
        if !has_capability(self.capabilities.load(Ordering::SeqCst), CAP_IP_QUALITY) {
            tracing::debug!("IpQualitySync received but CAP_IP_QUALITY not effective — ignoring");
            return;
        }
        tracing::info!(
            "IpQualitySync: {} services, interval={}h",
            services.len(),
            interval_hours
        );
        *self.services.lock().await = services;
        *self.interval_hours.lock().await = interval_hours;
    }

    /// Trigger an immediate check outside the regular schedule.
    pub fn run_now(&self) {
        if !has_capability(self.capabilities.load(Ordering::SeqCst), CAP_IP_QUALITY) {
            tracing::debug!(
                "IpQualityRunNow received but CAP_IP_QUALITY not effective — ignoring"
            );
            return;
        }
        // Best-effort send: if the channel is full the scheduler already has a pending run.
        let _ = self.run_now_tx.try_send(());
    }

    /// Notify the scheduler that the agent's egress IP has changed.
    ///
    /// If the new IP differs from the last known value the scheduler will
    /// immediately trigger a fresh run.
    pub fn notify_ip_changed(&self, new_ip: Option<String>) {
        let _ = self.ip_changed_tx.send(new_ip);
    }
}

/// Run a single complete unlock check for all configured services.
///
/// Returns the per-service results and the timestamp at which the run completed.
/// The egress IP is tracked externally (by the reporter) and is not required here.
pub async fn run_once(
    services: &[UnlockServiceDef],
) -> (Vec<UnlockResultData>, chrono::DateTime<Utc>) {
    let client = match http::build_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to build IP quality HTTP client: {e}");
            return (Vec::new(), Utc::now());
        }
    };
    let client = Arc::new(client);
    let semaphore = Arc::new(Semaphore::new(MAX_UNLOCK_CONCURRENT));

    let mut handles = Vec::with_capacity(services.len());

    for def in services {
        let def = def.clone();
        let client = Arc::clone(&client);
        let sem = Arc::clone(&semaphore);

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            run_single_service(def, &client).await
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => {
                tracing::warn!("UnlockChecker task panicked: {e}");
            }
        }
    }

    let checked_at = Utc::now();
    (results, checked_at)
}

/// Run the probe for a single `UnlockServiceDef` and map to a `UnlockResultData`.
async fn run_single_service(
    def: UnlockServiceDef,
    client: &reqwest::Client,
) -> UnlockResultData {
    // Determine whether this is a built-in (detector key) or custom (request + rules) service.
    if let Some(ref detector_key) = def.detector {
        // Built-in path: dispatch to the hardcoded detector.
        let (status, region, latency_ms) = detectors::dispatch(detector_key, client).await;
        tracing::debug!(
            "Detector '{}' (service_id={}) → {:?} region={:?} latency={}ms",
            detector_key,
            def.id,
            status,
            region,
            latency_ms
        );
        UnlockResultData {
            service_id: def.id.clone(),
            status,
            region,
            latency_ms: Some(latency_ms),
            detail: None,
        }
    } else if let (Some(request), Some(rules)) = (def.request.clone(), def.rules.clone()) {
        // Custom path: HTTP fetch + rule engine.
        use std::time::Instant;
        let start = Instant::now();
        match http::fetch(client, &request).await {
            Ok(outcome) => {
                let latency_ms = start.elapsed().as_millis() as u32;
                let status = rule_engine::apply_rules(&outcome, &rules);
                tracing::debug!(
                    "Custom service (service_id={}) → {:?} latency={}ms",
                    def.id,
                    status,
                    latency_ms
                );
                UnlockResultData {
                    service_id: def.id.clone(),
                    status,
                    region: None,
                    latency_ms: Some(latency_ms),
                    detail: None,
                }
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis() as u32;
                tracing::debug!(
                    "Custom service (service_id={}) fetch failed: {e}",
                    def.id
                );
                UnlockResultData {
                    service_id: def.id.clone(),
                    status: UnlockStatus::Failed,
                    region: None,
                    latency_ms: Some(latency_ms),
                    detail: Some(e.to_string()),
                }
            }
        }
    } else {
        // Misconfigured service (neither built-in nor custom).
        tracing::warn!(
            "Service '{}' has neither a detector key nor a request+rules pair — reporting Unsupported",
            def.id
        );
        UnlockResultData {
            service_id: def.id.clone(),
            status: UnlockStatus::Unsupported,
            region: None,
            latency_ms: None,
            detail: Some("misconfigured: no detector or request+rules".to_string()),
        }
    }
}

/// Background scheduler task.
///
/// Triggers:
///   1. Every `interval_hours`.
///   2. Immediately when `run_now_rx` receives a message.
///   3. Immediately when `ip_changed_rx` delivers a different IP value.
async fn run_scheduler(
    services: Arc<Mutex<Vec<UnlockServiceDef>>>,
    interval_hours: Arc<Mutex<u32>>,
    mut run_now_rx: mpsc::Receiver<()>,
    mut ip_changed_rx: watch::Receiver<Option<String>>,
    capabilities: Arc<AtomicU32>,
    result_tx: mpsc::Sender<RunResult>,
) {
    let mut last_ip: Option<String> = None;
    // Mark the initial value as seen so an IP watch set before the scheduler
    // starts only triggers if the IP actually changes from that initial value.
    ip_changed_rx.mark_unchanged();

    loop {
        // Read current interval (default 12h if somehow 0).
        let interval_h = {
            let h = *interval_hours.lock().await;
            if h == 0 { DEFAULT_INTERVAL_HOURS } else { h }
        };
        let interval_duration = Duration::from_secs(u64::from(interval_h) * 3600);

        let trigger_ip_change;

        tokio::select! {
            // Periodic trigger.
            _ = tokio::time::sleep(interval_duration) => {
                tracing::debug!("UnlockChecker: interval trigger ({}h)", interval_h);
                trigger_ip_change = false;
            }
            // Manual run-now trigger (mpsc: queued, never missed).
            msg = run_now_rx.recv() => {
                if msg.is_none() {
                    // Channel closed — caller dropped the checker; exit.
                    break;
                }
                tracing::info!("UnlockChecker: run-now trigger");
                trigger_ip_change = false;
            }
            // IP-change trigger.
            result = ip_changed_rx.changed() => {
                if result.is_err() {
                    break;
                }
                let new_ip = ip_changed_rx.borrow_and_update().clone();
                if new_ip == last_ip {
                    trigger_ip_change = false;
                } else {
                    tracing::info!(
                        "UnlockChecker: IP changed ({:?} → {:?}), triggering recheck",
                        last_ip,
                        new_ip
                    );
                    last_ip = new_ip;
                    trigger_ip_change = true;
                }
            }
        }

        // If the IP changed to the same value we already knew, nothing to do.
        // (For run-now and interval triggers, trigger_ip_change is false and we always proceed.)
        let _ = trigger_ip_change; // used only for logging; always proceed past here

        // Re-check capability before each run.
        if !has_capability(capabilities.load(Ordering::SeqCst), CAP_IP_QUALITY) {
            tracing::debug!("UnlockChecker: CAP_IP_QUALITY not effective, skipping run");
            continue;
        }

        let current_services = services.lock().await.clone();
        if current_services.is_empty() {
            tracing::debug!("UnlockChecker: no services configured, skipping run");
            continue;
        }

        let egress_ip = ip_changed_rx.borrow().clone().unwrap_or_default();
        tracing::info!(
            "UnlockChecker: starting run for {} services (egress_ip={})",
            current_services.len(),
            if egress_ip.is_empty() { "<unknown>" } else { &egress_ip }
        );

        let (results, checked_at) = run_once(&current_services).await;

        // Capture the current IP after the run completes.
        last_ip = ip_changed_rx.borrow().clone();

        let run_result = RunResult { egress_ip, results, checked_at };

        if result_tx.send(run_result).await.is_err() {
            tracing::debug!("UnlockChecker: result channel closed, stopping scheduler");
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU32;

    use serverbee_common::constants::CAP_IP_QUALITY;
    use serverbee_common::protocol::{UnlockServiceDef, UnlockStatus};
    use tokio::sync::mpsc;

    use super::*;

    fn make_caps(bits: u32) -> Arc<AtomicU32> {
        Arc::new(AtomicU32::new(bits))
    }

    fn stub_builtin(id: &str, key: &str) -> UnlockServiceDef {
        UnlockServiceDef {
            id: id.to_string(),
            key: key.to_string(),
            detector: Some(key.to_string()),
            request: None,
            rules: None,
        }
    }

    // ── sync stores services + interval ──────────────────────────────────────

    #[tokio::test]
    async fn sync_stores_services_and_interval() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = make_caps(CAP_IP_QUALITY);
        let checker = UnlockChecker::new(caps, tx);

        let services = vec![stub_builtin("s1", "netflix"), stub_builtin("s2", "chatgpt")];
        checker.sync(services.clone(), 6).await;

        let stored = checker.services.lock().await;
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].id, "s1");
        assert_eq!(stored[1].id, "s2");
        drop(stored);

        let stored_interval = *checker.interval_hours.lock().await;
        assert_eq!(stored_interval, 6);
    }

    #[tokio::test]
    async fn sync_ignores_when_capability_absent() {
        let (tx, _rx) = mpsc::channel(16);
        // No CAP_IP_QUALITY in effective caps.
        let caps = make_caps(0);
        let checker = UnlockChecker::new(caps, tx);

        let services = vec![stub_builtin("s1", "netflix")];
        checker.sync(services, 6).await;

        // Services should remain empty because the capability is absent.
        let stored = checker.services.lock().await;
        assert!(stored.is_empty(), "sync must be a no-op when capability absent");
    }

    // ── run_once with a stub (unknown key → Unsupported, not dropped) ─────────

    #[tokio::test]
    async fn run_once_unknown_detector_returns_unsupported_not_dropped() {
        // An unknown detector key must still produce a result row with Unsupported.
        let services = vec![stub_builtin("svc1", "unknown_detector_xyz")];
        let (results, _) = run_once(&services).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].service_id, "svc1");
        assert_eq!(results[0].status, UnlockStatus::Unsupported);
    }

    #[tokio::test]
    async fn run_once_misconfigured_service_returns_unsupported() {
        // A service with neither detector nor request+rules is misconfigured.
        let svc = UnlockServiceDef {
            id: "mis1".to_string(),
            key: "misc".to_string(),
            detector: None,
            request: None,
            rules: None,
        };
        let (results, _) = run_once(&[svc]).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, UnlockStatus::Unsupported);
    }

    #[tokio::test]
    async fn run_once_returns_result_for_every_service() {
        // Three unknown services — all should produce exactly one result each.
        let services = vec![
            stub_builtin("a", "unknown_a"),
            stub_builtin("b", "unknown_b"),
            stub_builtin("c", "unknown_c"),
        ];
        let (results, _) = run_once(&services).await;
        assert_eq!(results.len(), 3);
    }

    // ── concurrency cap — MAX_UNLOCK_CONCURRENT ───────────────────────────────

    // Verify the constant is in a sane range at compile time.
    const _: () = assert!(MAX_UNLOCK_CONCURRENT >= 1);
    const _: () = assert!(MAX_UNLOCK_CONCURRENT <= 10);

    #[tokio::test]
    async fn run_once_semaphore_allows_more_services_than_concurrent_limit() {
        // Create more services than MAX_UNLOCK_CONCURRENT to exercise the semaphore path.
        // All services must complete and produce exactly one result each.
        let n = MAX_UNLOCK_CONCURRENT * 2;
        let services: Vec<_> = (0..n)
            .map(|i| stub_builtin(&format!("svc{i}"), "unknown_xyz"))
            .collect();

        let (results, _) = run_once(&services).await;
        // All services must produce a result (none dropped due to semaphore).
        assert_eq!(results.len(), n);
        for r in &results {
            assert_eq!(r.status, UnlockStatus::Unsupported);
        }
    }

    // ── run_now triggers the scheduler ───────────────────────────────────────

    #[tokio::test]
    async fn run_now_triggers_scheduler_run_when_capability_present() {
        // run_now with capability present should cause the scheduler to deliver a result.
        // (This duplicates scheduler_delivers_result_on_run_now but verifies the
        // capability-present branch specifically.)
        let (tx, mut rx) = mpsc::channel(16);
        let caps = make_caps(CAP_IP_QUALITY);
        let checker = UnlockChecker::new(caps, tx);

        checker.sync(vec![stub_builtin("x", "unknown_xyz")], 12).await;
        checker.notify_ip_changed(Some("1.2.3.4".to_string()));
        checker.run_now();

        let result = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await;
        assert!(result.is_ok(), "run_now must trigger a scheduler run when capability present");
        assert!(result.unwrap().is_some());
    }

    #[tokio::test]
    async fn run_now_no_op_when_capability_absent() {
        let (tx, mut rx) = mpsc::channel(16);
        let caps = make_caps(0); // no CAP_IP_QUALITY
        let checker = UnlockChecker::new(caps, tx);

        checker.run_now(); // should be no-op — capability absent

        // The scheduler should not deliver any result within a short window.
        let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(
            result.is_err(),
            "run_now must not trigger a run when capability absent"
        );
    }

    // ── notify_ip_changed updates the watch ──────────────────────────────────

    #[tokio::test]
    async fn notify_ip_changed_updates_watch() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = make_caps(CAP_IP_QUALITY);
        let checker = UnlockChecker::new(caps, tx);

        checker.notify_ip_changed(Some("203.0.113.1".to_string()));
        let current = checker.ip_changed_tx.subscribe().borrow().clone();
        assert_eq!(current, Some("203.0.113.1".to_string()));
    }

    // ── scheduler delivers results via channel ────────────────────────────────

    #[tokio::test]
    async fn scheduler_delivers_result_on_run_now() {
        let (tx, mut rx) = mpsc::channel(16);
        let caps = make_caps(CAP_IP_QUALITY);
        let checker = UnlockChecker::new(caps, tx);

        // Provide one service (unknown detector → Unsupported, runs instantly).
        let services = vec![stub_builtin("svc1", "unknown_xyz")];
        checker.sync(services, 12).await;

        // Provide an egress IP so the scheduler doesn't skip.
        checker.notify_ip_changed(Some("1.2.3.4".to_string()));
        checker.run_now();

        // Wait for the result with a generous timeout.
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            rx.recv(),
        )
        .await
        .expect("timed out waiting for UnlockChecker result")
        .expect("channel closed before result");

        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].status, UnlockStatus::Unsupported);
    }
}
