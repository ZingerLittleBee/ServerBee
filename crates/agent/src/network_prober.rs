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
    last_targets: Vec<NetworkProbeTarget>,
    last_interval: u32,
    last_packet_count: u32,
}

impl NetworkProber {
    pub fn new(tx: mpsc::Sender<NetworkProbeResultData>, capabilities: Arc<AtomicU32>) -> Self {
        Self {
            tasks: HashMap::new(),
            tx,
            capabilities,
            last_targets: Vec::new(),
            last_interval: 60,
            last_packet_count: 5,
        }
    }

    /// Reconcile running tasks with the new target list.
    ///
    /// Targets are filtered by current capability flags. Tasks that are no longer
    /// in the list are stopped; tasks that are new or whose config has changed
    /// are (re)started.
    pub fn sync(&mut self, targets: Vec<NetworkProbeTarget>, interval: u32, packet_count: u32) {
        // Store for later resync when capabilities change
        self.last_targets = targets.clone();
        self.last_interval = interval;
        self.last_packet_count = packet_count;

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

    /// Re-run sync with the stored last configuration.
    ///
    /// Called when `CapabilitiesSync` updates the capability bitmap so that
    /// newly enabled/disabled probe types take effect immediately.
    pub fn resync_capabilities(&mut self) {
        let targets = self.last_targets.clone();
        let interval = self.last_interval;
        let packet_count = self.last_packet_count;
        self.sync(targets, interval, packet_count);
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
}
