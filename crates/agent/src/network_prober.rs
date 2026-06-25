use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use chrono::Utc;
use rand::Rng;
use serverbee_common::constants::{CAP_PING_HTTP, CAP_PING_ICMP, CAP_PING_TCP, has_capability};
use serverbee_common::types::{NetworkProbeResultData, NetworkProbeTarget};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::probe_utils::{probe_http, probe_icmp_batch, probe_tcp};

/// A running probe task for a single network target.
struct RunningTask {
    handle: JoinHandle<()>,
    target: NetworkProbeTarget,
    interval: u32,
    packet_count: u32,
}

/// Manages per-target network quality probe tasks. Each target runs its own
/// periodic task and sends aggregated results back through an mpsc channel.
pub struct NetworkProber {
    tasks: HashMap<String, RunningTask>,
    tx: mpsc::Sender<NetworkProbeResultData>,
    capabilities: Arc<AtomicU32>,
}

impl NetworkProber {
    pub fn new(tx: mpsc::Sender<NetworkProbeResultData>, capabilities: Arc<AtomicU32>) -> Self {
        Self {
            tasks: HashMap::new(),
            tx,
            capabilities,
        }
    }

    /// Reconcile running tasks with the new target list.
    ///
    /// Targets are filtered by current capability flags. Tasks that are no longer
    /// in the list are stopped; tasks that are new or whose config has changed
    /// are (re)started.
    pub fn sync(&mut self, targets: Vec<NetworkProbeTarget>, interval: u32, packet_count: u32) {
        // Filter by current capability bitmap
        let caps = self.capabilities.load(Ordering::SeqCst);
        let targets: Vec<_> = targets
            .into_iter()
            .filter(|t| {
                probe_type_to_cap(&t.probe_type)
                    .map(|cap| has_capability(caps, cap))
                    .unwrap_or(false)
            })
            .collect();

        let new_ids: HashMap<String, &NetworkProbeTarget> =
            targets.iter().map(|t| (t.target_id.clone(), t)).collect();

        // Stop tasks no longer in the list
        let to_remove: Vec<String> = self
            .tasks
            .keys()
            .filter(|id| !new_ids.contains_key(*id))
            .cloned()
            .collect();
        for id in to_remove {
            if let Some(task) = self.tasks.remove(&id) {
                task.handle.abort();
                tracing::debug!("Stopped network probe task {id}");
            }
        }

        // Start or restart tasks
        for target in targets {
            let should_restart = if let Some(existing) = self.tasks.get(&target.target_id) {
                // Restart if config changed or task exited unexpectedly
                existing.handle.is_finished()
                    || existing.interval != interval
                    || existing.packet_count != packet_count
                    || existing.target.probe_type != target.probe_type
                    || existing.target.target != target.target
            } else {
                true
            };

            if should_restart {
                // Abort old handle if present
                if let Some(old) = self.tasks.remove(&target.target_id) {
                    old.handle.abort();
                }

                let tx = self.tx.clone();
                let target_clone = target.clone();
                let handle = tokio::spawn(run_probe_task(target_clone, interval, packet_count, tx));

                let target_id = target.target_id.clone();
                self.tasks.insert(
                    target.target_id.clone(),
                    RunningTask {
                        handle,
                        target,
                        interval,
                        packet_count,
                    },
                );
                tracing::debug!("Started network probe task {target_id}");
            }
        }

        tracing::info!("Network probe tasks synced: {} active", self.tasks.len());
    }

    /// Abort all running probe tasks.
    pub fn stop_all(&mut self) {
        for (id, task) in self.tasks.drain() {
            task.handle.abort();
            tracing::debug!("Stopped network probe task {id}");
        }
    }
}

/// Background task for a single probe target. Runs on the given interval with
/// an initial random jitter to spread load across targets.
async fn run_probe_task(
    target: NetworkProbeTarget,
    interval: u32,
    packet_count: u32,
    tx: mpsc::Sender<NetworkProbeResultData>,
) {
    // Initial jitter: random delay between 0 and interval seconds
    let jitter_secs: f64 = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0.0..interval.max(1) as f64)
    };
    tokio::time::sleep(Duration::from_secs_f64(jitter_secs)).await;

    let interval_duration = Duration::from_secs(interval.max(10) as u64);
    let mut ticker = tokio::time::interval(interval_duration);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;

        let result = run_multi_probe(&target, packet_count).await;

        if tx.send(result).await.is_err() {
            tracing::debug!(
                "Network probe result channel closed, stopping task {}",
                target.target_id
            );
            break;
        }
    }
}

/// Run `count` probe attempts for the given target and aggregate results.
///
/// - ICMP: uses `probe_icmp_batch` (single `ping -c N` invocation).
/// - TCP/HTTP: runs N sequential probes and aggregates statistics.
async fn run_multi_probe(target: &NetworkProbeTarget, count: u32) -> NetworkProbeResultData {
    let timeout = Duration::from_secs(10);
    let count = count.max(1);

    match target.probe_type.as_str() {
        "icmp" => {
            let host = &target.target;
            let total_timeout = timeout + Duration::from_secs(count as u64 * 2);
            let batch = probe_icmp_batch(host, count, total_timeout).await;
            NetworkProbeResultData {
                target_id: target.target_id.clone(),
                avg_latency: batch.avg_latency,
                min_latency: batch.min_latency,
                max_latency: batch.max_latency,
                packet_loss: batch.packet_loss,
                packet_sent: batch.packet_sent,
                packet_received: batch.packet_received,
                timestamp: Utc::now(),
            }
        }
        "tcp" => {
            let mut latencies: Vec<f64> = Vec::new();
            let mut successes = 0u32;

            for _ in 0..count {
                let r = probe_tcp(&target.target, timeout).await;
                if r.success {
                    successes += 1;
                    latencies.push(r.latency_ms);
                }
            }

            aggregate_results(&target.target_id, count, successes, latencies)
        }
        "http" => {
            let mut latencies: Vec<f64> = Vec::new();
            let mut successes = 0u32;

            for _ in 0..count {
                let r = probe_http(&target.target, timeout).await;
                if r.success {
                    successes += 1;
                    latencies.push(r.latency_ms);
                }
            }

            aggregate_results(&target.target_id, count, successes, latencies)
        }
        other => {
            tracing::warn!(
                "Unknown probe type '{}' for target {}",
                other,
                target.target_id
            );
            NetworkProbeResultData {
                target_id: target.target_id.clone(),
                avg_latency: None,
                min_latency: None,
                max_latency: None,
                packet_loss: 1.0,
                packet_sent: count,
                packet_received: 0,
                timestamp: Utc::now(),
            }
        }
    }
}

/// Build a [`NetworkProbeResultData`] from raw per-probe measurements.
fn aggregate_results(
    target_id: &str,
    sent: u32,
    received: u32,
    latencies: Vec<f64>,
) -> NetworkProbeResultData {
    let (avg_latency, min_latency, max_latency) = if latencies.is_empty() {
        (None, None, None)
    } else {
        let sum: f64 = latencies.iter().sum();
        let avg = sum / latencies.len() as f64;
        let min = latencies.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        (Some(avg), Some(min), Some(max))
    };

    let packet_loss = if sent == 0 {
        1.0
    } else {
        (sent - received) as f64 / sent as f64
    };

    NetworkProbeResultData {
        target_id: target_id.to_string(),
        avg_latency,
        min_latency,
        max_latency,
        packet_loss,
        packet_sent: sent,
        packet_received: received,
        timestamp: Utc::now(),
    }
}

/// Map a probe_type string to its capability bit.
fn probe_type_to_cap(probe_type: &str) -> Option<u32> {
    match probe_type {
        "icmp" => Some(CAP_PING_ICMP),
        "tcp" => Some(CAP_PING_TCP),
        "http" => Some(CAP_PING_HTTP),
        _ => None,
    }
}

/// Split a "host:port" string, returning `(host, port)`.
/// If no port is specified, `default_port` (80) is returned.
#[allow(dead_code)]
fn parse_host_port(target: &str) -> (&str, u16) {
    if let Some(colon_pos) = target.rfind(':') {
        let host = &target[..colon_pos];
        let port_str = &target[colon_pos + 1..];
        if let Ok(port) = port_str.parse::<u16>() {
            return (host, port);
        }
    }
    (target, 80)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_host_port_with_port() {
        let (host, port) = parse_host_port("example.com:443");
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_parse_host_port_without_port() {
        let (host, port) = parse_host_port("example.com");
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
    }

    #[test]
    fn test_parse_host_port_ipv4_with_port() {
        let (host, port) = parse_host_port("192.168.1.1:8080");
        assert_eq!(host, "192.168.1.1");
        assert_eq!(port, 8080);
    }

    #[test]
    fn test_aggregate_results_all_success() {
        let latencies = vec![10.0, 20.0, 30.0];
        let result = aggregate_results("target1", 3, 3, latencies);
        assert_eq!(result.packet_sent, 3);
        assert_eq!(result.packet_received, 3);
        assert!((result.packet_loss - 0.0).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 20.0).abs() < 0.001);
        assert!((result.min_latency.unwrap() - 10.0).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_aggregate_results_partial_loss() {
        let latencies = vec![15.0, 25.0];
        let result = aggregate_results("target2", 4, 2, latencies);
        assert_eq!(result.packet_sent, 4);
        assert_eq!(result.packet_received, 2);
        assert!((result.packet_loss - 0.5).abs() < 0.001);
        assert!(result.avg_latency.is_some());
    }

    #[test]
    fn test_aggregate_results_total_loss() {
        let result = aggregate_results("target3", 5, 0, vec![]);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < 0.001);
        assert!(result.avg_latency.is_none());
        assert!(result.min_latency.is_none());
        assert!(result.max_latency.is_none());
    }

    #[test]
    fn test_probe_type_to_cap() {
        assert_eq!(probe_type_to_cap("icmp"), Some(CAP_PING_ICMP));
        assert_eq!(probe_type_to_cap("tcp"), Some(CAP_PING_TCP));
        assert_eq!(probe_type_to_cap("http"), Some(CAP_PING_HTTP));
        assert_eq!(probe_type_to_cap("unknown"), None);
    }

    #[tokio::test]
    async fn test_network_prober_sync_stops_removed_tasks() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP));
        let mut prober = NetworkProber::new(tx, caps);

        let targets = vec![NetworkProbeTarget {
            target_id: "t1".to_string(),
            name: "Test 1".to_string(),
            target: "127.0.0.1".to_string(),
            probe_type: "icmp".to_string(),
        }];
        prober.sync(targets, 60, 3);
        assert_eq!(prober.tasks.len(), 1);

        // Sync with empty list — task should be stopped
        prober.sync(vec![], 60, 3);
        assert_eq!(prober.tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_network_prober_capability_filter() {
        let (tx, _rx) = mpsc::channel(16);
        // Only TCP capability enabled
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut prober = NetworkProber::new(tx, caps);

        let targets = vec![
            NetworkProbeTarget {
                target_id: "icmp1".to_string(),
                name: "ICMP target".to_string(),
                target: "1.1.1.1".to_string(),
                probe_type: "icmp".to_string(),
            },
            NetworkProbeTarget {
                target_id: "tcp1".to_string(),
                name: "TCP target".to_string(),
                target: "1.1.1.1:80".to_string(),
                probe_type: "tcp".to_string(),
            },
        ];
        prober.sync(targets, 60, 3);

        // Only TCP task should be started
        assert_eq!(prober.tasks.len(), 1);
        assert!(prober.tasks.contains_key("tcp1"));
        assert!(!prober.tasks.contains_key("icmp1"));

        prober.stop_all();
    }

    fn target(id: &str, probe_type: &str, addr: &str) -> NetworkProbeTarget {
        NetworkProbeTarget {
            target_id: id.to_string(),
            name: format!("name-{id}"),
            target: addr.to_string(),
            probe_type: probe_type.to_string(),
        }
    }

    // ── probe_type_to_cap: unknown/empty variants ────────────────────────────

    #[test]
    fn test_probe_type_to_cap_empty_and_other() {
        assert_eq!(probe_type_to_cap(""), None);
        assert_eq!(probe_type_to_cap("ICMP"), None); // case-sensitive
        assert_eq!(probe_type_to_cap("dns"), None);
    }

    // ── parse_host_port: extra fall-through branches ─────────────────────────

    #[test]
    fn test_parse_host_port_non_numeric_port_falls_through() {
        // rfind finds a colon but the port doesn't parse: fall through to default.
        let (host, port) = parse_host_port("example.com:notaport");
        assert_eq!(host, "example.com:notaport");
        assert_eq!(port, 80);
    }

    #[test]
    fn test_parse_host_port_trailing_colon() {
        // Trailing colon -> empty port string -> parse fails -> default.
        let (host, port) = parse_host_port("example.com:");
        assert_eq!(host, "example.com:");
        assert_eq!(port, 80);
    }

    #[test]
    fn test_parse_host_port_uses_last_colon() {
        // rfind returns the LAST colon; the bare IPv6 here has its port parsed
        // off the final segment only when that segment is numeric.
        let (host, port) = parse_host_port("[::1]:8443");
        assert_eq!(host, "[::1]");
        assert_eq!(port, 8443);
    }

    // ── aggregate_results: single sample + zero-sent edge cases ───────────────

    #[test]
    fn test_aggregate_results_single_sample() {
        let result = aggregate_results("solo", 1, 1, vec![42.5]);
        assert_eq!(result.target_id, "solo");
        assert_eq!(result.packet_sent, 1);
        assert_eq!(result.packet_received, 1);
        assert!((result.packet_loss - 0.0).abs() < f64::EPSILON);
        assert!((result.avg_latency.unwrap() - 42.5).abs() < 0.001);
        assert!((result.min_latency.unwrap() - 42.5).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 42.5).abs() < 0.001);
    }

    #[test]
    fn test_aggregate_results_zero_sent_is_total_loss() {
        // sent == 0 hits the dedicated `1.0` branch (avoids divide-by-zero).
        let result = aggregate_results("z", 0, 0, vec![]);
        assert_eq!(result.packet_sent, 0);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
        assert!(result.avg_latency.is_none());
    }

    #[test]
    fn test_aggregate_results_min_max_unordered_input() {
        // Latencies arrive out of order; min/max must still be correct.
        let result = aggregate_results("u", 5, 5, vec![30.0, 5.0, 99.0, 12.0, 50.0]);
        assert!((result.min_latency.unwrap() - 5.0).abs() < 0.001);
        assert!((result.max_latency.unwrap() - 99.0).abs() < 0.001);
        assert!((result.avg_latency.unwrap() - 39.2).abs() < 0.001);
    }

    // ── run_multi_probe: unknown probe type returns total-loss result ─────────

    #[tokio::test]
    async fn test_run_multi_probe_unknown_type_total_loss() {
        let t = target("u1", "dns", "example.com");
        let result = run_multi_probe(&t, 4).await;
        assert_eq!(result.target_id, "u1");
        assert_eq!(result.packet_sent, 4);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
        assert!(result.avg_latency.is_none());
        assert!(result.min_latency.is_none());
        assert!(result.max_latency.is_none());
    }

    #[tokio::test]
    async fn test_run_multi_probe_unknown_type_clamps_count_to_one() {
        // count.max(1): a 0 count becomes 1 for the reported packet_sent.
        let t = target("u2", "whatever", "host");
        let result = run_multi_probe(&t, 0).await;
        assert_eq!(result.packet_sent, 1);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_run_multi_probe_icmp_blocked_loopback_total_loss() {
        // ICMP path: SSRF guard blocks loopback inside probe_icmp_batch, so the
        // aggregated result reports total loss without any real ping.
        let t = target("icmp-lb", "icmp", "127.0.0.1");
        let result = run_multi_probe(&t, 3).await;
        assert_eq!(result.target_id, "icmp-lb");
        assert_eq!(result.packet_sent, 3);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
        assert!(result.avg_latency.is_none());
    }

    #[tokio::test]
    async fn test_run_multi_probe_tcp_blocked_loopback_total_loss() {
        // TCP path: each probe_tcp is SSRF-blocked (no outbound network), so
        // successes stays 0 and aggregate_results reports total loss.
        let t = target("tcp-lb", "tcp", "127.0.0.1:1");
        let result = run_multi_probe(&t, 2).await;
        assert_eq!(result.target_id, "tcp-lb");
        assert_eq!(result.packet_sent, 2);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
        assert!(result.avg_latency.is_none());
    }

    #[tokio::test]
    async fn test_run_multi_probe_http_blocked_loopback_total_loss() {
        // HTTP path: each probe_http is SSRF-blocked before any connect.
        let t = target("http-lb", "http", "http://127.0.0.1:1");
        let result = run_multi_probe(&t, 2).await;
        assert_eq!(result.target_id, "http-lb");
        assert_eq!(result.packet_sent, 2);
        assert_eq!(result.packet_received, 0);
        assert!((result.packet_loss - 1.0).abs() < f64::EPSILON);
    }

    // ── sync: restart-on-config-change branches ──────────────────────────────

    #[tokio::test]
    async fn test_sync_restarts_when_interval_changes() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 60, 3);
        assert_eq!(prober.tasks.len(), 1);
        let first_interval = prober.tasks.get("t1").unwrap().interval;
        assert_eq!(first_interval, 60);

        // Same target, different interval -> task is restarted with new interval.
        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 120, 3);
        assert_eq!(prober.tasks.len(), 1);
        assert_eq!(prober.tasks.get("t1").unwrap().interval, 120);

        prober.stop_all();
    }

    #[tokio::test]
    async fn test_sync_restarts_when_packet_count_changes() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 60, 3);
        assert_eq!(prober.tasks.get("t1").unwrap().packet_count, 3);

        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 60, 7);
        assert_eq!(prober.tasks.len(), 1);
        assert_eq!(prober.tasks.get("t1").unwrap().packet_count, 7);

        prober.stop_all();
    }

    #[tokio::test]
    async fn test_sync_restarts_when_target_address_changes() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 60, 3);
        assert_eq!(prober.tasks.get("t1").unwrap().target.target, "1.1.1.1:80");

        // Same id, different target address -> restart.
        prober.sync(vec![target("t1", "tcp", "8.8.8.8:80")], 60, 3);
        assert_eq!(prober.tasks.len(), 1);
        assert_eq!(prober.tasks.get("t1").unwrap().target.target, "8.8.8.8:80");

        prober.stop_all();
    }

    #[tokio::test]
    async fn test_sync_restarts_when_probe_type_changes() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP | CAP_PING_HTTP));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 60, 3);
        assert_eq!(prober.tasks.get("t1").unwrap().target.probe_type, "tcp");

        // Same id, different probe_type -> restart.
        prober.sync(vec![target("t1", "http", "1.1.1.1:80")], 60, 3);
        assert_eq!(prober.tasks.len(), 1);
        assert_eq!(prober.tasks.get("t1").unwrap().target.probe_type, "http");

        prober.stop_all();
    }

    #[tokio::test]
    async fn test_sync_no_restart_when_unchanged() {
        // Re-syncing with an identical target keeps the same handle (no restart).
        // We can't compare handle identity, but the task count and config stay
        // stable across the unchanged sync.
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 60, 3);
        assert_eq!(prober.tasks.len(), 1);

        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 60, 3);
        assert_eq!(prober.tasks.len(), 1);
        assert_eq!(prober.tasks.get("t1").unwrap().interval, 60);
        assert_eq!(prober.tasks.get("t1").unwrap().packet_count, 3);

        prober.stop_all();
    }

    #[tokio::test]
    async fn test_sync_adds_and_keeps_multiple_targets() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP | CAP_PING_HTTP));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(
            vec![
                target("a", "tcp", "1.1.1.1:80"),
                target("b", "http", "1.1.1.1:80"),
            ],
            60,
            3,
        );
        assert_eq!(prober.tasks.len(), 2);
        assert!(prober.tasks.contains_key("a"));
        assert!(prober.tasks.contains_key("b"));

        // Drop "a", keep "b", add "c".
        prober.sync(
            vec![
                target("b", "http", "1.1.1.1:80"),
                target("c", "tcp", "1.1.1.1:80"),
            ],
            60,
            3,
        );
        assert_eq!(prober.tasks.len(), 2);
        assert!(!prober.tasks.contains_key("a"));
        assert!(prober.tasks.contains_key("b"));
        assert!(prober.tasks.contains_key("c"));

        prober.stop_all();
    }

    #[tokio::test]
    async fn test_sync_filters_all_when_no_capabilities() {
        // Zero capability bitmap -> every target is filtered out.
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(0));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(
            vec![
                target("a", "icmp", "1.1.1.1"),
                target("b", "tcp", "1.1.1.1:80"),
                target("c", "http", "http://1.1.1.1"),
            ],
            60,
            3,
        );
        assert_eq!(prober.tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_sync_filters_unknown_probe_type() {
        // An unknown probe_type maps to None -> filtered out even with full caps.
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(vec![target("weird", "dns", "example.com")], 60, 3);
        assert_eq!(prober.tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_stop_all_clears_tasks() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut prober = NetworkProber::new(tx, caps);

        prober.sync(vec![target("t1", "tcp", "1.1.1.1:80")], 60, 3);
        assert_eq!(prober.tasks.len(), 1);

        prober.stop_all();
        assert_eq!(prober.tasks.len(), 0);

        // stop_all on an already-empty prober is a no-op.
        prober.stop_all();
        assert_eq!(prober.tasks.len(), 0);
    }
}
