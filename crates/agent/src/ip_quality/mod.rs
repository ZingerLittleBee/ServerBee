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
use tokio::sync::{Mutex, Semaphore, mpsc, watch};
use tokio::task::JoinHandle;

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
    /// Handle to the spawned scheduler task — aborted on `stop()` / `Drop`.
    scheduler_handle: JoinHandle<()>,
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

        // Spawn the scheduler task and keep its handle so it can be aborted
        // explicitly on reconnect — see `stop()`.
        let scheduler_handle = tokio::spawn(run_scheduler(
            Arc::clone(&services),
            Arc::clone(&interval_hours),
            run_now_rx,
            ip_changed_rx,
            Arc::clone(&capabilities),
            result_tx,
        ));

        Self {
            services,
            interval_hours,
            run_now_tx,
            ip_changed_tx,
            capabilities,
            scheduler_handle,
        }
    }

    /// Abort the background scheduler task.
    ///
    /// Mirrors `NetworkProber::stop_all()`: the reporter calls this on every
    /// reconnect / close / error path so an orphaned scheduler can never keep
    /// issuing outbound HTTP probes across a connection flap. Aborting is
    /// immediate even if the scheduler is mid-`run_once` (a batch of probes
    /// that can otherwise take tens of seconds).
    pub fn stop(&self) {
        self.scheduler_handle.abort();
        tracing::debug!("UnlockChecker scheduler aborted");
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

impl Drop for UnlockChecker {
    /// Safety net: abort the scheduler if the checker is dropped without an
    /// explicit `stop()`. The reporter calls `stop()` on its teardown paths,
    /// but `Drop` guarantees the task never outlives the checker.
    fn drop(&mut self) {
        self.scheduler_handle.abort();
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

    run_once_with(services, move |def| {
        let client = Arc::clone(&client);
        async move { run_single_service(def, &client).await }
    })
    .await
}

/// Generic core of [`run_once`]: runs `probe` for every service, capped at
/// [`MAX_UNLOCK_CONCURRENT`] concurrent probes via a [`Semaphore`].
///
/// `probe` is the per-service body that runs while a semaphore permit is held;
/// production passes [`run_single_service`], tests inject an instrumented
/// closure so the concurrency cap can be verified directly.
async fn run_once_with<F, Fut>(
    services: &[UnlockServiceDef],
    probe: F,
) -> (Vec<UnlockResultData>, chrono::DateTime<Utc>)
where
    F: Fn(UnlockServiceDef) -> Fut,
    Fut: std::future::Future<Output = UnlockResultData> + Send + 'static,
{
    let semaphore = Arc::new(Semaphore::new(MAX_UNLOCK_CONCURRENT));

    let mut handles = Vec::with_capacity(services.len());

    for def in services {
        let fut = probe(def.clone());
        let sem = Arc::clone(&semaphore);

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            fut.await
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
/// Triggers a run:
///   1. Every `interval_hours`.
///   2. Immediately when `run_now_rx` receives a message.
///   3. Immediately when `ip_changed_rx` delivers an IP that *differs* from the
///      last known one. A watch send carrying the same value is ignored.
async fn run_scheduler(
    services: Arc<Mutex<Vec<UnlockServiceDef>>>,
    interval_hours: Arc<Mutex<u32>>,
    mut run_now_rx: mpsc::Receiver<()>,
    mut ip_changed_rx: watch::Receiver<Option<String>>,
    capabilities: Arc<AtomicU32>,
    result_tx: mpsc::Sender<RunResult>,
) {
    // The last IP value the scheduler has acted on. Updated only by the
    // IP-change arm (the single well-defined update point).
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

        tokio::select! {
            // Periodic trigger.
            _ = tokio::time::sleep(interval_duration) => {
                tracing::debug!("UnlockChecker: interval trigger ({}h)", interval_h);
            }
            // Manual run-now trigger (mpsc: queued, never missed).
            msg = run_now_rx.recv() => {
                if msg.is_none() {
                    // Channel closed — caller dropped the checker; exit.
                    break;
                }
                tracing::info!("UnlockChecker: run-now trigger");
            }
            // IP-change trigger.
            result = ip_changed_rx.changed() => {
                if result.is_err() {
                    break;
                }
                let new_ip = ip_changed_rx.borrow_and_update().clone();
                if new_ip == last_ip {
                    // A watch send carrying the same value is not an IP change —
                    // skip the run entirely (do not fall through to run_once).
                    tracing::debug!("UnlockChecker: IP watch fired but value unchanged, skipping");
                    continue;
                }
                tracing::info!(
                    "UnlockChecker: IP changed ({:?} → {:?}), triggering recheck",
                    last_ip,
                    new_ip
                );
                last_ip = new_ip;
            }
        }

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

        // The egress IP is captured here, at the start of the run. The probes
        // are attributed to this IP even if the egress IP changes mid-run; a
        // subsequent IP-change trigger reruns the whole batch with the new IP.
        let egress_ip = ip_changed_rx.borrow().clone().unwrap_or_default();
        tracing::info!(
            "UnlockChecker: starting run for {} services (egress_ip={})",
            current_services.len(),
            if egress_ip.is_empty() { "<unknown>" } else { &egress_ip }
        );

        let (results, checked_at) = run_once(&current_services).await;

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
    use std::sync::atomic::{AtomicU32, AtomicUsize};

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
        // Throughput check: all services complete and produce exactly one result.
        let n = MAX_UNLOCK_CONCURRENT * 2;
        let services: Vec<_> = (0..n)
            .map(|i| stub_builtin(&format!("svc{i}"), "unknown_xyz"))
            .collect();

        let (results, _) = run_once(&services).await;
        assert_eq!(results.len(), n);
        for r in &results {
            assert_eq!(r.status, UnlockStatus::Unsupported);
        }
    }

    #[tokio::test]
    async fn run_once_caps_concurrency_at_max_unlock_concurrent() {
        // Genuine concurrency-cap test: inject an instrumented per-service body
        // that tracks how many probes are in flight simultaneously. With
        // 2 * MAX_UNLOCK_CONCURRENT services, the observed peak must never
        // exceed MAX_UNLOCK_CONCURRENT — this fails if the Semaphore is removed
        // or its permit count is raised.
        let n = MAX_UNLOCK_CONCURRENT * 2;
        let services: Vec<_> = (0..n)
            .map(|i| stub_builtin(&format!("svc{i}"), "x"))
            .collect();

        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let in_flight_c = Arc::clone(&in_flight);
        let max_seen_c = Arc::clone(&max_seen);

        let (results, _) = run_once_with(&services, move |def| {
            let in_flight = Arc::clone(&in_flight_c);
            let max_seen = Arc::clone(&max_seen_c);
            async move {
                // Entry: bump the in-flight counter, record a new peak.
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                // Hold the permit briefly so concurrent probes overlap.
                tokio::time::sleep(Duration::from_millis(50)).await;
                // Exit: release the slot.
                in_flight.fetch_sub(1, Ordering::SeqCst);
                UnlockResultData {
                    service_id: def.id.clone(),
                    status: UnlockStatus::Unsupported,
                    region: None,
                    latency_ms: None,
                    detail: None,
                }
            }
        })
        .await;

        // All services ran.
        assert_eq!(results.len(), n);
        // The cap was genuinely enforced.
        let peak = max_seen.load(Ordering::SeqCst);
        assert!(
            peak <= MAX_UNLOCK_CONCURRENT,
            "concurrency cap violated: observed peak {peak} > MAX_UNLOCK_CONCURRENT {MAX_UNLOCK_CONCURRENT}"
        );
        // Sanity: with 2x services and brief holds, we should have actually
        // saturated the cap (otherwise the test is not exercising contention).
        assert!(
            peak >= MAX_UNLOCK_CONCURRENT.min(n),
            "expected the cap to be saturated (peak {peak}), test not exercising contention"
        );
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
