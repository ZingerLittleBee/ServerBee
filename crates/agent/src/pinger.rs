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

    #[tokio::test]
    async fn test_tcp_ping_open_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let target = format!("127.0.0.1:{}", addr.port());
        let r = probe_utils::probe_tcp(&target, Duration::from_secs(10)).await;
        assert!(r.success);
        assert!(r.latency_ms > 0.0);
    }

    #[tokio::test]
    async fn test_tcp_ping_closed_port() {
        // Port 1 is reserved and should be closed/refused on loopback
        let r = probe_utils::probe_tcp("127.0.0.1:1", Duration::from_secs(10)).await;
        assert!(!r.success);
    }
}
