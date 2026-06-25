use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use chrono::Utc;
use serverbee_common::constants::{has_capability, probe_type_to_cap};
use serverbee_common::types::{PingResult, PingTaskConfig};
use tokio::sync::mpsc;

use crate::probe_utils;

/// Manages running ping probe tasks. Each task runs on its own interval
/// and sends results back through an mpsc channel.
pub struct PingManager {
    tasks: HashMap<String, tokio::task::JoinHandle<()>>,
    result_tx: mpsc::Sender<PingResult>,
    capabilities: Arc<AtomicU32>,
}

impl PingManager {
    pub fn new(result_tx: mpsc::Sender<PingResult>, capabilities: Arc<AtomicU32>) -> Self {
        Self {
            tasks: HashMap::new(),
            result_tx,
            capabilities,
        }
    }

    /// Reconcile running tasks with the new config list.
    /// Stops tasks no longer in the list, starts new ones, restarts changed ones.
    pub fn sync(&mut self, configs: Vec<PingTaskConfig>) {
        // Filter by capability bitmap
        let caps = self.capabilities.load(Ordering::SeqCst);
        let configs: Vec<_> = configs
            .into_iter()
            .filter(|c| {
                probe_type_to_cap(&c.probe_type)
                    .map(|cap| has_capability(caps, cap))
                    .unwrap_or(false)
            })
            .collect();

        let new_ids: HashMap<String, &PingTaskConfig> =
            configs.iter().map(|c| (c.task_id.clone(), c)).collect();

        // Stop tasks not in the new list
        let to_remove: Vec<String> = self
            .tasks
            .keys()
            .filter(|id| !new_ids.contains_key(*id))
            .cloned()
            .collect();
        for id in to_remove {
            if let Some(handle) = self.tasks.remove(&id) {
                handle.abort();
                tracing::debug!("Stopped ping task {id}");
            }
        }

        // Start or restart tasks
        for config in configs {
            let should_start = if let Some(handle) = self.tasks.get(&config.task_id) {
                if handle.is_finished() {
                    self.tasks.remove(&config.task_id);
                    true
                } else {
                    // Task is running — for simplicity, abort and restart
                    // (in case interval or target changed)
                    handle.abort();
                    self.tasks.remove(&config.task_id);
                    true
                }
            } else {
                true
            };

            if should_start {
                let tx = self.result_tx.clone();
                let task_id = config.task_id.clone();
                let handle = tokio::spawn(run_ping_task(config, tx));
                self.tasks.insert(task_id, handle);
            }
        }

        tracing::info!("Ping tasks synced: {} active", self.tasks.len());
    }

    pub fn stop_all(&mut self) {
        for (id, handle) in self.tasks.drain() {
            handle.abort();
            tracing::debug!("Stopped ping task {id}");
        }
    }
}

async fn run_ping_task(config: PingTaskConfig, tx: mpsc::Sender<PingResult>) {
    let interval_duration = Duration::from_secs(config.interval.max(5) as u64);
    let mut ticker = tokio::time::interval(interval_duration);
    ticker.tick().await; // consume first immediate tick

    loop {
        ticker.tick().await;

        let timeout = Duration::from_secs(10);
        let result = match config.probe_type.as_str() {
            "icmp" => {
                let r = probe_utils::probe_icmp(&config.target, timeout).await;
                PingResult {
                    task_id: config.task_id.clone(),
                    latency: r.latency_ms,
                    success: r.success,
                    error: r.error,
                    time: Utc::now(),
                }
            }
            "tcp" => {
                let r = probe_utils::probe_tcp(&config.target, timeout).await;
                PingResult {
                    task_id: config.task_id.clone(),
                    latency: r.latency_ms,
                    success: r.success,
                    error: r.error,
                    time: Utc::now(),
                }
            }
            "http" => {
                let r = probe_utils::probe_http(&config.target, timeout).await;
                PingResult {
                    task_id: config.task_id.clone(),
                    latency: r.latency_ms,
                    success: r.success,
                    error: r.error,
                    time: Utc::now(),
                }
            }
            other => PingResult {
                task_id: config.task_id.clone(),
                latency: 0.0,
                success: false,
                error: Some(format!("Unknown probe type: {other}")),
                time: Utc::now(),
            },
        };

        if tx.send(result).await.is_err() {
            tracing::debug!(
                "Ping result channel closed, stopping task {}",
                config.task_id
            );
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise the connect logic directly via `probe_tcp_addrs` against a
    // validated loopback address. The public `probe_tcp` now runs the SSRF guard
    // first, which (correctly) blocks raw loopback targets — that guard is
    // covered by the tests in `probe_utils`.
    #[tokio::test]
    async fn test_tcp_ping_open_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let r = probe_utils::probe_tcp_addrs(&[addr], Duration::from_secs(10)).await;
        assert!(r.success);
    }

    #[tokio::test]
    async fn test_tcp_ping_closed_port() {
        // Port 1 is reserved and should be closed/refused on loopback
        let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        let r = probe_utils::probe_tcp_addrs(&[addr], Duration::from_secs(10)).await;
        assert!(!r.success);
    }

    use serverbee_common::constants::{CAP_PING_HTTP, CAP_PING_ICMP, CAP_PING_TCP};

    fn config(id: &str, probe_type: &str, target: &str, interval: u32) -> PingTaskConfig {
        PingTaskConfig {
            task_id: id.to_string(),
            probe_type: probe_type.to_string(),
            target: target.to_string(),
            interval,
        }
    }

    #[tokio::test]
    async fn test_ping_manager_new_starts_empty() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_ICMP));
        let manager = PingManager::new(tx, caps);
        assert_eq!(manager.tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_ping_manager_sync_starts_filtered_tasks() {
        let (tx, _rx) = mpsc::channel(16);
        // Only TCP capability enabled.
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut manager = PingManager::new(tx, caps);

        let configs = vec![
            config("icmp1", "icmp", "1.1.1.1", 30),
            config("tcp1", "tcp", "1.1.1.1:80", 30),
            config("http1", "http", "http://1.1.1.1", 30),
        ];
        manager.sync(configs);

        // Only the TCP task survives the capability filter.
        assert_eq!(manager.tasks.len(), 1);
        assert!(manager.tasks.contains_key("tcp1"));
        assert!(!manager.tasks.contains_key("icmp1"));
        assert!(!manager.tasks.contains_key("http1"));

        manager.stop_all();
    }

    #[tokio::test]
    async fn test_ping_manager_sync_filters_unknown_probe_type() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP));
        let mut manager = PingManager::new(tx, caps);

        // Unknown probe type maps to None -> filtered out.
        manager.sync(vec![config("dns1", "dns", "example.com", 30)]);
        assert_eq!(manager.tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_ping_manager_sync_filters_all_when_no_caps() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(0));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![
            config("icmp1", "icmp", "1.1.1.1", 30),
            config("tcp1", "tcp", "1.1.1.1:80", 30),
        ]);
        assert_eq!(manager.tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_ping_manager_sync_stops_removed_tasks() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![config("tcp1", "tcp", "1.1.1.1:80", 30)]);
        assert_eq!(manager.tasks.len(), 1);

        // Empty list -> the running task is stopped and removed.
        manager.sync(vec![]);
        assert_eq!(manager.tasks.len(), 0);
    }

    #[tokio::test]
    async fn test_ping_manager_sync_restarts_running_task() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![config("tcp1", "tcp", "1.1.1.1:80", 30)]);
        assert_eq!(manager.tasks.len(), 1);

        // Re-sync with the same id but a changed target: the running task is
        // aborted and a fresh one is started (still exactly one task).
        manager.sync(vec![config("tcp1", "tcp", "8.8.8.8:80", 60)]);
        assert_eq!(manager.tasks.len(), 1);
        assert!(manager.tasks.contains_key("tcp1"));

        manager.stop_all();
    }

    #[tokio::test]
    async fn test_ping_manager_sync_keeps_unrelated_and_adds_new() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP | CAP_PING_HTTP));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![
            config("a", "tcp", "1.1.1.1:80", 30),
            config("b", "http", "http://1.1.1.1", 30),
        ]);
        assert_eq!(manager.tasks.len(), 2);

        // Drop "a", keep "b", add "c".
        manager.sync(vec![
            config("b", "http", "http://1.1.1.1", 30),
            config("c", "tcp", "1.1.1.1:80", 30),
        ]);
        assert_eq!(manager.tasks.len(), 2);
        assert!(!manager.tasks.contains_key("a"));
        assert!(manager.tasks.contains_key("b"));
        assert!(manager.tasks.contains_key("c"));

        manager.stop_all();
    }

    #[tokio::test]
    async fn test_ping_manager_sync_restarts_finished_task() {
        // A task whose handle has already finished must be removed and a new
        // one started on the next sync. We force a finished handle by closing
        // the result channel (the task exits once it tries to send) — but since
        // the loop only sends after a tick, we instead drive the finished-handle
        // branch by aborting via stop_all then re-syncing.
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![config("tcp1", "tcp", "1.1.1.1:80", 30)]);
        assert_eq!(manager.tasks.len(), 1);

        // Abort the task and let the runtime register it as finished.
        for handle in manager.tasks.values() {
            handle.abort();
        }
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Re-sync with the same config: whether the handle is reported finished
        // or still running, the result is exactly one active task for the id.
        manager.sync(vec![config("tcp1", "tcp", "1.1.1.1:80", 30)]);
        assert_eq!(manager.tasks.len(), 1);
        assert!(manager.tasks.contains_key("tcp1"));

        manager.stop_all();
    }

    #[tokio::test]
    async fn test_ping_manager_stop_all_clears_tasks() {
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_TCP | CAP_PING_HTTP));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![
            config("a", "tcp", "1.1.1.1:80", 30),
            config("b", "http", "http://1.1.1.1", 30),
        ]);
        assert_eq!(manager.tasks.len(), 2);

        manager.stop_all();
        assert_eq!(manager.tasks.len(), 0);

        // stop_all on an empty manager is a safe no-op.
        manager.stop_all();
        assert_eq!(manager.tasks.len(), 0);
    }

    // ── run_ping_task: the only network-free branch (unknown probe type) ──────
    // The icmp/tcp/http arms all reach into `probe_utils::probe_*` (process /
    // socket / HTTP); the `other =>` arm builds a failed PingResult purely from
    // the config without touching the network. Deterministic time control
    // (`start_paused`) lets us advance past the clamped interval without waiting.

    #[tokio::test(start_paused = true)]
    async fn run_ping_task_unknown_probe_type_builds_failed_result() {
        // An unrecognized probe_type hits the catch-all arm: success=false,
        // latency=0, and a formatted "Unknown probe type" error naming the type.
        let (tx, mut rx) = mpsc::channel(4);
        let cfg = config("dns1", "dns", "example.com", 5);
        let handle = tokio::spawn(run_ping_task(cfg, tx));

        // First (immediate) tick is consumed by the task; advance one interval
        // (clamped to >= 5s) so the second tick fires and produces a result.
        tokio::time::advance(Duration::from_secs(5)).await;
        let result = rx.recv().await.expect("a result should be produced");

        assert_eq!(result.task_id, "dns1");
        assert!(!result.success);
        assert!((result.latency - 0.0).abs() < f64::EPSILON);
        assert_eq!(
            result.error.as_deref(),
            Some("Unknown probe type: dns")
        );
        handle.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn run_ping_task_clamps_sub_minimum_interval_to_five_seconds() {
        // interval=0 is clamped to the 5s floor (`config.interval.max(5)`):
        // advancing only 4s yields no result, advancing the remaining 1s does.
        let (tx, mut rx) = mpsc::channel(4);
        let cfg = config("slow", "bogus", "host", 0);
        let handle = tokio::spawn(run_ping_task(cfg, tx));

        tokio::time::advance(Duration::from_secs(4)).await;
        assert!(
            rx.try_recv().is_err(),
            "no tick should fire before the clamped 5s interval elapses"
        );

        tokio::time::advance(Duration::from_secs(1)).await;
        let result = rx.recv().await.expect("result after the clamped interval");
        assert_eq!(result.task_id, "slow");
        assert!(!result.success);
        handle.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn run_ping_task_emits_repeatedly_on_each_interval() {
        // The loop re-arms after each tick: two interval advances yield two
        // results, each carrying the same task_id and unknown-type error.
        let (tx, mut rx) = mpsc::channel(4);
        let cfg = config("rep", "nope", "host", 5);
        let handle = tokio::spawn(run_ping_task(cfg, tx));

        tokio::time::advance(Duration::from_secs(5)).await;
        let first = rx.recv().await.expect("first result");
        tokio::time::advance(Duration::from_secs(5)).await;
        let second = rx.recv().await.expect("second result");

        assert_eq!(first.task_id, "rep");
        assert_eq!(second.task_id, "rep");
        assert_eq!(first.error.as_deref(), Some("Unknown probe type: nope"));
        assert_eq!(second.error.as_deref(), Some("Unknown probe type: nope"));
        handle.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn run_ping_task_exits_when_result_channel_closed() {
        // After `tx.send` fails (the receiver is gone) the task takes the
        // channel-closed early-return branch instead of looping forever.
        let (tx, mut rx) = mpsc::channel(1);
        let cfg = config("orphan", "unknown", "host", 5);
        let handle = tokio::spawn(run_ping_task(cfg, tx));

        // Drive one full iteration with the channel OPEN so the task is past its
        // first immediate tick and actively running the loop (mirrors the
        // sibling interval tests, which `recv` to let the spawned task progress).
        tokio::time::advance(Duration::from_secs(5)).await;
        let _ = rx.recv().await.expect("first result before the channel is closed");

        // Now close the channel; the NEXT interval tick produces a result whose
        // send fails, so the task returns instead of looping.
        drop(rx);
        tokio::time::advance(Duration::from_secs(5)).await;

        // The spawned task should complete (return) rather than run forever.
        let joined = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(
            joined.is_ok(),
            "task must exit after the result channel is closed"
        );
    }

    // ── sync capability filter: probe_type_to_cap / has_capability boundaries ──

    #[tokio::test]
    async fn sync_probe_type_matching_is_case_sensitive() {
        // probe_type_to_cap only maps lowercase "icmp"/"tcp"/"http"; an
        // uppercase variant maps to None and is filtered out even with all caps.
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![
            config("up", "ICMP", "1.1.1.1", 30),
            config("ws", "tcp ", "1.1.1.1:80", 30),
        ]);
        assert_eq!(manager.tasks.len(), 0);
    }

    #[tokio::test]
    async fn sync_keeps_only_tasks_whose_single_cap_is_enabled() {
        // Exactly one capability bit set: only the matching probe_type survives,
        // exercising has_capability against a single-bit mask for every type.
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(CAP_PING_HTTP));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![
            config("i", "icmp", "1.1.1.1", 30),
            config("t", "tcp", "1.1.1.1:80", 30),
            config("h", "http", "http://1.1.1.1", 30),
        ]);
        assert_eq!(manager.tasks.len(), 1);
        assert!(manager.tasks.contains_key("h"));

        manager.stop_all();
    }

    #[tokio::test]
    async fn sync_empty_probe_type_is_filtered_out() {
        // An empty probe_type string maps to None (catch-all in
        // probe_type_to_cap) and is dropped regardless of the capability mask.
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(u32::MAX));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![config("blank", "", "1.1.1.1", 30)]);
        assert_eq!(manager.tasks.len(), 0);
    }

    #[tokio::test]
    async fn sync_with_all_bits_set_keeps_every_valid_probe() {
        // u32::MAX enables every bit, so all three valid probe types pass the
        // capability filter together.
        let (tx, _rx) = mpsc::channel(16);
        let caps = Arc::new(AtomicU32::new(u32::MAX));
        let mut manager = PingManager::new(tx, caps);

        manager.sync(vec![
            config("i", "icmp", "1.1.1.1", 30),
            config("t", "tcp", "1.1.1.1:80", 30),
            config("h", "http", "http://1.1.1.1", 30),
        ]);
        assert_eq!(manager.tasks.len(), 3);

        manager.stop_all();
    }
}
